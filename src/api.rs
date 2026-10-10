//! HTTP client for the SoChain API (https://sochain.com/api/v3).

use std::thread::sleep;
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::Url;
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, RETRY_AFTER, WARNING};
use serde_json::Value;

use crate::error::{CliError, Result};
use crate::sanitize;

/// The only host the CLI talks to.
pub const SITE: &str = "https://sochain.com";

/// How many times a rate-limited request is retried, and the longest
/// Retry-After the CLI will sit out before giving up.
const RATE_LIMIT_RETRIES: u32 = 3;
const MAX_RETRY_WAIT_SECS: u64 = 30;

pub struct Api {
    base: Url,
    http: Client,
    timeout: Duration,
    key: Option<String>,
    /// Print notes (rate-limit waits, `Warning` headers) on stderr. Off for
    /// --json, where stderr carries only the JSON error.
    notes: bool,
}

/// A successful reply, before its body is interpreted.
pub struct Raw {
    pub body: Vec<u8>,
    pub headers: HeaderMap,
}

impl Api {
    pub fn new(timeout: Duration, key: Option<String>, notes: bool) -> Result<Api> {
        let base = Url::parse(&base_url())
            .map_err(|e| CliError::Other(format!("invalid API URL: {e}")))?;
        // Follow redirects only within the same host, so neither a request
        // nor the API key can be sent elsewhere.
        let redirects = reqwest::redirect::Policy::custom(|attempt| {
            let same_host = attempt
                .previous()
                .first()
                .is_some_and(|first| first.host_str() == attempt.url().host_str());
            if attempt.previous().len() > 5 || !same_host {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });
        let http = Client::builder()
            .user_agent(concat!("sochain-cli/", env!("CARGO_PKG_VERSION")))
            .redirect(redirects)
            .connect_timeout(timeout)
            .build()
            .map_err(|e| CliError::Other(format!("could not start the HTTP client: {e}")))?;
        Ok(Api {
            base,
            http,
            timeout,
            key,
            notes,
        })
    }

    /// `/api/v3/<segments…>`, each segment percent-encoded so user input
    /// cannot change the path.
    fn url(&self, segments: &[&str], query: &[(&str, String)]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("API base URL has a path")
            .pop_if_empty()
            .extend(["api", "v3"])
            .extend(segments);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        url
    }

    /// GETs an endpoint and returns the whole success envelope
    /// (`{"status":"success","data":…}`).
    pub fn get(&self, segments: &[&str], query: &[(&str, String)]) -> Result<Value> {
        let raw = self.fetch(&self.url(segments, query), "application/json")?;
        let body: Value = serde_json::from_slice(&raw.body).map_err(|_| not_api_data(200))?;
        if body["status"] != "success" {
            return Err(CliError::Other(
                api_message(&body).unwrap_or_else(|| "unexpected response from SoChain".into()),
            ));
        }
        Ok(body)
    }

    /// GETs an endpoint that answers without the envelope (`/account_info`).
    pub fn get_plain(&self, segments: &[&str]) -> Result<Value> {
        let raw = self.fetch(&self.url(segments, &[]), "application/json")?;
        serde_json::from_slice(&raw.body).map_err(|_| not_api_data(200))
    }

    /// GETs an endpoint as CSV, returning the body and the reply's headers
    /// (the next page is in `Link` and `X-Next-Cursor`).
    pub fn get_csv(&self, segments: &[&str], query: &[(&str, String)]) -> Result<Raw> {
        self.fetch(&self.url(segments, query), "text/csv")
    }

    /// GETs a file from the site itself (not the JSON API), such as the
    /// agent-skills index. `path` must be an absolute path, or a URL on the
    /// same origin; replies larger than `max_bytes` are refused. The API key
    /// is not sent.
    pub fn fetch_site_file(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>> {
        let refuse = || CliError::Other(format!("refusing to fetch {path:?}: not on sochain.com"));
        let url = if path.starts_with('/') && !path.starts_with("//") {
            self.base.join(path).map_err(|_| refuse())?
        } else {
            let url = Url::parse(path).map_err(|_| refuse())?;
            if url.origin() != self.base.origin() {
                return Err(refuse());
            }
            url
        };
        let resp = self.send(&url, "*/*", false)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(match status {
                StatusCode::NOT_FOUND => CliError::NotFound(format!("{path} not found")),
                s if is_unavailable(s) => CliError::Unavailable(format!(
                    "SoChain is unavailable right now (HTTP {}); try again later",
                    s.as_u16()
                )),
                s => CliError::Other(format!("{path}: HTTP {}", s.as_u16())),
            });
        }
        let body = resp
            .bytes()
            .map_err(|e| CliError::Unavailable(format!("could not read {path}: {e}")))?;
        if body.len() > max_bytes {
            return Err(CliError::Other(format!(
                "{path} is larger than expected ({} bytes)",
                body.len()
            )));
        }
        Ok(body.to_vec())
    }

    /// Sends a GET, waiting out short rate limits, and maps failures to
    /// errors with the API's own message where it gives one.
    fn fetch(&self, url: &Url, accept: &str) -> Result<Raw> {
        let mut attempt = 0;
        let resp = loop {
            let resp = self.send(url, accept, true)?;
            if resp.status() != StatusCode::TOO_MANY_REQUESTS {
                break resp;
            }
            let wait = retry_after(&resp).unwrap_or(1);
            attempt += 1;
            if attempt > RATE_LIMIT_RETRIES || wait > MAX_RETRY_WAIT_SECS {
                let message = resp
                    .json::<Value>()
                    .ok()
                    .and_then(|b| api_message(&b))
                    .unwrap_or_else(|| "too many requests".into());
                return Err(CliError::RateLimited {
                    message: format!(
                        "rate limited by SoChain: {}; try again in {}",
                        message.trim_end_matches('.'),
                        seconds(wait)
                    ),
                    retry_after_secs: wait,
                });
            }
            self.note(&format!(
                "too many requests at once; retrying in {}",
                seconds(wait)
            ));
            sleep(Duration::from_secs(wait));
        };
        let status = resp.status();
        let headers = resp.headers().clone();
        if let Some(w) = headers.get(WARNING).and_then(|v| v.to_str().ok()) {
            self.note(&format!("note from SoChain: {}", warning_text(w)));
        }
        let body = resp
            .bytes()
            .map_err(|e| CliError::Unavailable(format!("the reply was cut off: {e}")))?
            .to_vec();
        if status.is_success() {
            return Ok(Raw { body, headers });
        }
        let message = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|b| api_message(&b));
        Err(match status {
            StatusCode::BAD_REQUEST => {
                CliError::BadInput(message.unwrap_or_else(|| "SoChain rejected the request".into()))
            }
            StatusCode::NOT_FOUND => {
                CliError::NotFound(message.unwrap_or_else(|| "not found".into()))
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                let message = message.unwrap_or_else(|| {
                    format!("SoChain refused the request (HTTP {})", status.as_u16())
                });
                CliError::Unauthorized(if self.key.is_some() {
                    message
                } else {
                    format!(
                        "{message} This needs an API key: run `sochain auth login` or set SOCHAIN_API_KEY."
                    )
                })
            }
            s if is_unavailable(s) => CliError::Unavailable(message.unwrap_or_else(|| {
                format!(
                    "SoChain is unavailable right now (HTTP {}); try again later",
                    s.as_u16()
                )
            })),
            s if message.is_none() && !s.is_client_error() => not_api_data(s.as_u16()),
            s => CliError::Other(
                message.unwrap_or_else(|| format!("SoChain returned HTTP {}", s.as_u16())),
            ),
        })
    }

    fn send(&self, url: &Url, accept: &str, with_key: bool) -> Result<Response> {
        let mut req = self
            .http
            .get(url.clone())
            .timeout(self.timeout)
            .header(ACCEPT, accept);
        if let (true, Some(key)) = (with_key, &self.key) {
            let mut value = HeaderValue::from_str(key).map_err(|_| {
                CliError::BadInput("the API key has characters a key can't have".into())
            })?;
            value.set_sensitive(true);
            req = req.header("API-KEY", value);
        }
        req.send().map_err(|e| unreachable_error(&e))
    }

    fn note(&self, text: &str) {
        if self.notes {
            eprintln!("sochain: {}", sanitize::clean(text));
        }
    }
}

/// Tests point the binary at a local server. Only debug builds read this, so
/// released binaries always talk to sochain.com.
fn base_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(base) = std::env::var("SOCHAIN_TEST_API_BASE") {
        return base;
    }
    SITE.to_owned()
}

fn api_message(body: &Value) -> Option<String> {
    body["data"]["error_message"].as_str().map(str::to_owned)
}

/// Something answered, but not the API: e.g. a web page or proxy error in
/// front of it.
fn not_api_data(status: u16) -> CliError {
    CliError::Unavailable(format!(
        "SoChain did not return API data (HTTP {status}); try again later"
    ))
}

fn is_unavailable(s: StatusCode) -> bool {
    matches!(
        s,
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
    )
}

/// Seconds from a Retry-After header in its delay form.
fn retry_after(resp: &Response) -> Option<u64> {
    resp.headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// `1 second`, `45 seconds`, `3 h 20 min`.
fn seconds(s: u64) -> String {
    match s {
        1 => "1 second".into(),
        0..120 => format!("{s} seconds"),
        120..3_600 => format!("{} min", s / 60),
        _ => format!("{} h {} min", s / 3_600, s % 3_600 / 60),
    }
}

/// The text of a `Warning: 199 - "text"` header, or the whole value.
fn warning_text(w: &str) -> &str {
    match (w.find('"'), w.rfind('"')) {
        (Some(a), Some(b)) if b > a => &w[a + 1..b],
        _ => w,
    }
}

fn unreachable_error(e: &reqwest::Error) -> CliError {
    let what = if e.is_timeout() {
        "timed out waiting for SoChain".to_owned()
    } else {
        "could not reach SoChain".to_owned()
    };
    // The innermost cause is the useful part ("connection refused", DNS, TLS).
    let mut cause: &dyn std::error::Error = e;
    while let Some(next) = cause.source() {
        cause = next;
    }
    CliError::Unavailable(format!("{what}: {cause}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_waits() {
        assert_eq!(seconds(1), "1 second");
        assert_eq!(seconds(30), "30 seconds");
        assert_eq!(seconds(600), "10 min");
        assert_eq!(seconds(12_000), "3 h 20 min");
    }

    #[test]
    fn reads_warning_headers() {
        assert_eq!(
            warning_text(r#"199 - "data is 2 blocks behind""#),
            "data is 2 blocks behind"
        );
        assert_eq!(warning_text("plain"), "plain");
    }
}
