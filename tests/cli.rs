//! End-to-end tests: the real binary against a local mock of the API.
//!
//! The binary only accepts a different API base in debug builds, so these
//! tests are skipped under `--release`.
#![cfg(debug_assertions)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use tempfile::TempDir;

const HASH: &str = "00000000000000000000ae63ac51b71f605ca502649009a2793fd5fce070ce5a";
const TXID: &str = "5420695c11ddebff09fd2786c16b8481b2187cb750458880ddd9e71cbf0da0e8";
const ADDR: &str = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh";
const OTHER: &str = "bc1q4599jxvf9hj4m84lfdll26jjzxgrrep33j36t9";
const KEY: &str = "test-key-0123456789";

struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    content_type: &'static str,
    body: String,
}

fn reply(status: u16, body: Value) -> Reply {
    Reply {
        status,
        headers: vec![],
        content_type: "application/json",
        body: body.to_string(),
    }
}

fn ok(data: Value) -> Reply {
    reply(200, json!({ "status": "success", "data": data }))
}

fn fail(status: u16, message: &str) -> Reply {
    reply(
        status,
        json!({ "status": "fail", "data": { "error_message": message } }),
    )
}

fn with_header(mut r: Reply, name: &'static str, value: &str) -> Reply {
    r.headers.push((name, value.to_owned()));
    r
}

/// One request as the mock saw it.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    /// Header names lower-cased.
    headers: Vec<(String, String)>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Serves `replies` in order, one per request, and records each request.
struct Mock {
    base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Mock {
    fn paths(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.path.clone())
            .collect()
    }

    fn request(&self, i: usize) -> Seen {
        self.seen.lock().unwrap()[i].clone()
    }
}

fn mock(replies: Vec<Reply>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    thread::spawn(move || {
        for r in replies {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_owned();
            let mut headers = vec![];
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 2 {
                if let Some((k, v)) = line.trim_end().split_once(':') {
                    headers.push((k.to_ascii_lowercase(), v.trim().to_owned()));
                }
                line.clear();
            }
            log.lock().unwrap().push(Seen { path, headers });
            let mut head = format!(
                "HTTP/1.1 {} X\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n",
                r.status,
                r.content_type,
                r.body.len()
            );
            for (k, v) in &r.headers {
                head += &format!("{k}: {v}\r\n");
            }
            let _ = stream.write_all(format!("{head}\r\n{}", r.body).as_bytes());
        }
    });
    Mock { base, seen }
}

/// The binary pointed at the mock, with a private config folder and none of
/// the caller's SOCHAIN_* settings.
fn sochain(m: &Mock, config: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("sochain").unwrap();
    cmd.env("SOCHAIN_TEST_API_BASE", &m.base)
        .env("SOCHAIN_CONFIG_DIR", config.path())
        .env_remove("SOCHAIN_JSON")
        .env_remove("SOCHAIN_TIMEOUT")
        .env_remove("SOCHAIN_CURRENCY")
        .env_remove("SOCHAIN_API_KEY");
    cmd
}

fn config() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn price() -> Value {
    json!({ "value": "80000.00000000", "currency": "USD" })
}

fn block() -> Value {
    json!({
        "height": 968995, "time": 1_790_598_850, "confirmations": 3, "num_txs": 4866,
        "version": 537395200, "nonce": 1971860836, "size": 1642284, "weight": 3994770,
        "hash": HASH, "previous_block_hash": HASH, "next_block_hash": null, "is_orphan": false,
        "difficulty": "132757073449487.5", "merkleroot": HASH, "reward": "3.12500000",
        "fees": "0.01661977", "value_out": "4710.77665999", "bits": "17021ec5",
        "txs": [TXID], "price": price()
    })
}

fn transaction() -> Value {
    json!({
        "hash": TXID, "confirmations": 0, "size": 470, "vsize": 308, "weight": 1229,
        "version": 2, "locktime": 0, "block_hash": null, "block_height": null,
        "time": 1_790_598_850, "inputs_n": 1, "outputs_n": 2, "inputs_value": "0.00112405",
        "outputs_value": "0.00111725", "fee": "0.00000680", "price": price(),
        "inputs": [{ "index": 0, "value": "0.00112405", "address": OTHER,
                     "scriptSig": { "hex": "", "asm": "" }, "witness": [],
                     "previous_output": { "hash": TXID, "index": 1 } }],
        "outputs": [
            { "index": 0, "value": "0.00000000", "address": "op_return", "type": "nulldata",
              "script": { "hex": "6a", "asm": "OP_RETURN" }, "spent": null, "is_change": null },
            { "index": 1, "value": "0.00111725", "address": ADDR, "type": "witness_v0_keyhash",
              "script": { "hex": "00", "asm": "0" }, "spent": null, "is_change": null }
        ]
    })
}

fn history_row(change: &str) -> Value {
    json!({
        "hash": TXID, "value_sent": "0.00000000", "value_received": change,
        "balance_change": change, "time": 1_790_598_850, "block": 968995, "price": price()
    })
}

#[test]
fn block_text_output_shows_full_hashes() {
    let m = mock(vec![ok(block())]);
    sochain(&m, &config())
        .args(["block", "BTC", "968995"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Bitcoin block 968,995 · 4,866 transactions",
        ))
        .stdout(predicate::str::contains(format!("Hash {HASH}")))
        .stdout(predicate::str::contains("4,710.78 BTC moved"))
        .stdout(predicate::str::contains("Price then 1 BTC = $80,000.00"))
        .stdout(predicate::str::contains("Next block not mined yet"));
    assert_eq!(m.paths(), ["/api/v3/block/BTC/968995"]);
}

#[test]
fn networks_are_case_insensitive() {
    let m = mock(vec![ok(
        json!({ "confirmed": "1.00000000", "unconfirmed": "0.00000000" }),
    )]);
    sochain(&m, &config())
        .args(["balance", "ltc", "LQTpS3VaYTjCr4s9Y1t5zbeY26zevf7Fb3"])
        .assert()
        .success()
        .stdout("Balance 1 LTC\n");
    assert_eq!(
        m.paths(),
        ["/api/v3/balance/LTC/LQTpS3VaYTjCr4s9Y1t5zbeY26zevf7Fb3"]
    );
}

#[test]
fn unknown_networks_are_rejected_locally() {
    let m = mock(vec![]);
    sochain(&m, &config())
        .args(["balance", "ETH", ADDR, "--json"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains("\"code\":\"BAD_INPUT\""));
    assert!(m.paths().is_empty());
}

#[test]
fn latest_block_looks_up_the_best_hash_first() {
    let m = mock(vec![ok(json!({ "hash": HASH })), ok(block())]);
    sochain(&m, &config())
        .args(["block", "BTC", "latest"])
        .assert()
        .success();
    assert_eq!(
        m.paths(),
        [
            "/api/v3/best_block_hash/BTC".to_owned(),
            format!("/api/v3/block/BTC/{HASH}")
        ]
    );
}

#[test]
fn json_output_is_the_api_reply_unchanged() {
    let m = mock(vec![ok(block())]);
    let out = sochain(&m, &config())
        .args(["block", "BTC", "968995", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v, json!({ "status": "success", "data": block() }));
    // Field order is kept too.
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.find("\"status\"").unwrap() < text.find("\"data\"").unwrap());
    assert!(text.find("\"height\"").unwrap() < text.find("\"hash\"").unwrap());
    assert!(out.stderr.is_empty());
}

#[test]
fn json_env_var_turns_on_json() {
    let m = mock(vec![ok(json!({ "hash": HASH }))]);
    let out = sochain(&m, &config())
        .env("SOCHAIN_JSON", "1")
        .args(["best-block-hash", "DOGE"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["hash"], HASH);
}

#[test]
fn user_input_cannot_escape_the_api_path() {
    let m = mock(vec![fail(404, "Unable to fetch block data.")]);
    sochain(&m, &config())
        .args(["block", "BTC", "../../x?y=1"])
        .assert()
        .code(3);
    let paths = m.paths();
    assert!(paths[0].starts_with("/api/v3/block/BTC/"), "{paths:?}");
    assert!(!paths[0].contains('?'), "{paths:?}");
}

#[test]
fn not_found_exits_3() {
    let m = mock(vec![fail(404, "Transaction data not found.")]);
    sochain(&m, &config())
        .args(["transaction", "BTC", TXID])
        .assert()
        .code(3)
        .stdout("")
        .stderr("sochain: Transaction data not found.\n");
}

#[test]
fn errors_are_json_on_stderr_with_json() {
    let m = mock(vec![fail(400, "Bad direction.")]);
    let out = sochain(&m, &config())
        .args(["transactions", "BTC", ADDR, "--direction", "in", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let v: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        v,
        json!({ "status": "fail", "data": { "error_message": "Bad direction.", "code": "BAD_INPUT" } })
    );
}

#[test]
fn unauthorized_exits_6_and_suggests_a_key() {
    let m = mock(vec![fail(401, "API Key invalid.")]);
    sochain(&m, &config())
        .args(["balance", "BTC", ADDR])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("API Key invalid."))
        .stderr(predicate::str::contains("sochain auth login"));
}

#[test]
fn unavailable_exits_4() {
    let m = mock(vec![fail(503, "Briefly unavailable.")]);
    sochain(&m, &config())
        .args(["network-info", "BTC"])
        .assert()
        .code(4)
        .stderr("sochain: Briefly unavailable.\n");

    let mut page = reply(502, json!(null));
    page.content_type = "text/html";
    page.body = "<html>bad gateway</html>".into();
    let m = mock(vec![page]);
    sochain(&m, &config())
        .args(["network-info", "BTC"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("unavailable right now (HTTP 502)"));
}

#[test]
fn short_rate_limits_are_waited_out() {
    let busy = with_header(fail(429, "Too many requests."), "retry-after", "1");
    let m = mock(vec![busy, ok(json!({ "hash": HASH }))]);
    sochain(&m, &config())
        .args(["best-block-hash", "BTC"])
        .assert()
        .success()
        .stdout(format!("{HASH}\n"))
        .stderr(predicate::str::contains("retrying in 1 second"));
    assert_eq!(m.paths().len(), 2);
}

#[test]
fn long_rate_limits_exit_5_at_once() {
    let limited = with_header(fail(429, "Daily limit reached."), "retry-after", "3600");
    let m = mock(vec![limited]);
    let out = sochain(&m, &config())
        .args(["best-block-hash", "BTC", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    let v: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(v["data"]["code"], "RATE_LIMITED");
    assert_eq!(v["data"]["retry_after_secs"], 3600);
    assert!(
        v["data"]["error_message"]
            .as_str()
            .unwrap()
            .contains("Daily limit reached")
    );
    assert_eq!(m.paths().len(), 1);
}

#[test]
fn secrets_are_refused_without_a_request() {
    let m = mock(vec![]);
    sochain(&m, &config())
        .args([
            "balance",
            "BTC",
            "KwDiBf89QgGbjEhKnhXJuH7LrciVrZi3qYjgd9M7rFU73sVHnoWn",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("looks like a private key"));
    assert!(m.paths().is_empty());
}

#[test]
fn the_key_is_sent_in_the_api_key_header() {
    let m = mock(vec![ok(json!({ "hash": HASH }))]);
    sochain(&m, &config())
        .env("SOCHAIN_API_KEY", KEY)
        .args(["best-block-hash", "BTC"])
        .assert()
        .success();
    let r = m.request(0);
    assert_eq!(r.header("api-key"), Some(KEY));
    let agent = r.header("user-agent").unwrap();
    assert!(agent.starts_with("sochain-cli/"), "{agent}");
}

#[test]
fn no_key_header_without_a_key() {
    let m = mock(vec![ok(json!({ "hash": HASH }))]);
    sochain(&m, &config())
        .args(["best-block-hash", "BTC"])
        .assert()
        .success();
    assert_eq!(m.request(0).header("api-key"), None);
}

fn account() -> Value {
    json!({
        "last_login_at": 1_790_000_000,
        "plan": { "name": "free", "max_daily_requests": 2500,
                  "current_period_ends_at": null, "will_renew_at_period_end": false },
        "num_requests_used": { "today": 12, "yesterday": 3400 },
        "key": { "name": "laptop", "access": "read_only" }
    })
}

#[test]
fn login_checks_and_saves_the_key_then_lookups_use_it() {
    let dir = config();
    let m = mock(vec![reply(200, account()), ok(json!({ "hash": HASH }))]);
    sochain(&m, &dir)
        .args(["auth", "login"])
        .write_stdin(format!("{KEY}\n"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Logged in with the key \"laptop\" (free plan).",
        ))
        .stdout(predicate::str::contains(KEY).not());
    assert_eq!(m.request(0).path, "/api/v3/account_info");
    assert_eq!(m.request(0).header("api-key"), Some(KEY));

    let file = dir.path().join("credentials.json");
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["api_key"], KEY);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    sochain(&m, &dir)
        .args(["best-block-hash", "BTC"])
        .assert()
        .success();
    assert_eq!(m.request(1).header("api-key"), Some(KEY));

    let m = mock(vec![]);
    sochain(&m, &dir)
        .args(["auth", "logout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed the saved key"));
    assert!(!file.exists());
}

#[test]
fn login_does_not_save_a_rejected_key() {
    let dir = config();
    let m = mock(vec![fail(401, "API Key invalid.")]);
    sochain(&m, &dir)
        .args(["auth", "login"])
        .write_stdin("nope-nope\n")
        .assert()
        .code(6);
    assert!(!dir.path().join("credentials.json").exists());
}

#[test]
fn login_rejects_malformed_keys() {
    let dir = config();
    let m = mock(vec![]);
    sochain(&m, &dir)
        .args(["auth", "login", "--no-verify"])
        .write_stdin("two words\n")
        .assert()
        .code(2);
    assert!(m.paths().is_empty());
    assert!(!dir.path().join("credentials.json").exists());
}

#[test]
fn account_needs_a_key() {
    let m = mock(vec![]);
    sochain(&m, &config())
        .arg("account")
        .assert()
        .code(6)
        .stderr(predicate::str::contains("needs an API key"));
    assert!(m.paths().is_empty());
}

#[test]
fn account_prints_usage_and_json_without_envelope() {
    let m = mock(vec![reply(200, account()), reply(200, account())]);
    sochain(&m, &config())
        .env("SOCHAIN_API_KEY", KEY)
        .arg("account")
        .assert()
        .success()
        .stdout(predicate::str::contains("Plan free"))
        .stdout(predicate::str::contains(
            "Requests today 12 · yesterday 3,400",
        ))
        .stdout(predicate::str::contains("Key \"laptop\" (read-only)"));
    let out = sochain(&m, &config())
        .env("SOCHAIN_API_KEY", KEY)
        .args(["account", "--json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v, account());
}

#[test]
fn address_history_pages_by_number() {
    let rows: Vec<Value> = (0..10).map(|_| history_row("0.50000000")).collect();
    let m = mock(vec![ok(json!({ "transactions": rows }))]);
    sochain(&m, &config())
        .args(["transactions", "BTC", ADDR, "--page", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("+0.5 BTC ($40,000)"))
        .stdout(predicate::str::contains(TXID))
        .stdout(predicate::str::contains(format!(
            "More: sochain transactions BTC {ADDR} --page 3"
        )));
    assert_eq!(m.paths(), [format!("/api/v3/transactions/BTC/{ADDR}/2")]);
}

#[test]
fn filtered_history_sends_the_filters_and_prints_the_cursor() {
    let m = mock(vec![ok(json!({
        "transactions": [history_row("-0.25000000")],
        "next_cursor": "abc123", "page_size": 1, "from_height": 0, "to_height": null,
        "direction": "out", "unconfirmed": "include", "tip_height": 970000
    }))]);
    sochain(&m, &config())
        .args([
            "transactions",
            "BTC",
            ADDR,
            "--from-time",
            "2025-01-01",
            "--direction",
            "out",
            "--page-size",
            "1",
            "--currency",
            "eur",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("-0.25 BTC"))
        .stdout(predicate::str::contains(format!(
            "More: sochain transactions BTC {ADDR} --cursor abc123"
        )));
    let path = &m.paths()[0];
    assert!(
        path.starts_with(&format!("/api/v3/transactions/BTC/{ADDR}?")),
        "{path}"
    );
    for part in [
        "currency=EUR",
        "from_time=2025-01-01",
        "direction=out",
        "page_size=1",
    ] {
        assert!(path.contains(part), "{path} lacks {part}");
    }
}

#[test]
fn csv_history_goes_to_stdout_and_the_cursor_to_stderr() {
    let csv = "time_utc,block,txid\n2026-09-28T12:34:10Z,968995,abc\n";
    let mut r = with_header(reply(200, json!(null)), "x-next-cursor", "next1");
    r.content_type = "text/csv";
    r.body = csv.into();
    let m = mock(vec![r]);
    sochain(&m, &config())
        .args(["transactions", "BTC", ADDR, "--from-height", "1", "--csv"])
        .assert()
        .success()
        .stdout(csv)
        .stderr(predicate::str::contains("--csv --cursor next1"));
    let seen = m.request(0);
    assert!(seen.path.contains("format=csv"), "{}", seen.path);
    assert_eq!(seen.header("accept"), Some("text/csv"));
}

#[test]
fn page_and_filters_do_not_mix() {
    let m = mock(vec![]);
    sochain(&m, &config())
        .args([
            "transactions",
            "BTC",
            ADDR,
            "--page",
            "2",
            "--direction",
            "in",
        ])
        .assert()
        .code(2);
    assert!(m.paths().is_empty());
}

#[test]
fn transaction_text_names_inputs_outputs_and_fee() {
    let m = mock(vec![ok(transaction())]);
    sochain(&m, &config())
        .args(["tx", "BTC", TXID, "--currency", "USD"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Transaction {TXID} on Bitcoin"
        )))
        .stdout(predicate::str::contains(
            "Status waiting in the mempool (unconfirmed)",
        ))
        .stdout(predicate::str::contains(format!(
            "  {OTHER}  0.00112405 BTC"
        )))
        .stdout(predicate::str::contains("  data (OP_RETURN)  0 BTC"))
        .stdout(predicate::str::contains(
            "Fee 0.0000068 BTC ($0.54) · 470 bytes, 308 vbytes",
        ));
    assert_eq!(
        m.paths(),
        [format!("/api/v3/transaction/BTC/{TXID}?currency=USD")]
    );
}

#[test]
fn transaction_status_explains_replacements() {
    let m = mock(vec![ok(json!({
        "network": "BTC", "hash": TXID, "status": "replaced", "block_height": null,
        "confirmations": null, "replaced_by": HASH, "replaced_by_status": "mined",
        "replaced_by_height": 970001, "replaced_ancestor": null, "removed_at": 1_790_000_000
    }))]);
    sochain(&m, &config())
        .args(["status", "BTC", TXID])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Replaced: another transaction spends one of its inputs (a fee bump or double spend): {HASH}, mined in block 970,001."
        )));
}

#[test]
fn price_at_a_date_uses_its_unix_time() {
    let m = mock(vec![ok(
        json!({ "value": "42000.00000000", "currency": "USD" }),
    )]);
    sochain(&m, &config())
        .args(["price", "BTC", "--time", "2024-01-01"])
        .assert()
        .success()
        .stdout("1 BTC = $42,000.00 (that UTC day's opening price)\n");
    assert_eq!(m.paths(), ["/api/v3/price/BTC/1704067200"]);
}

fn network_info() -> Value {
    json!({
        "block_count": 970710, "best_block_hash": HASH, "hashrate": "925644484990023500000",
        "mempool": { "mempool_txs": 56010, "mempool_size": 35987720, "updated_at": 1_791_599_830,
            "blocks": [
                { "block_num": 1, "min_fee_rate": 1, "max_fee_rate": 140, "median_fee_rate": 1,
                  "min_fee_rate_possible": 1, "weight": 3990487, "size": 1578709, "num_txs": 4080,
                  "min_fee_rate_precise": "0.10", "max_fee_rate_precise": "140.00",
                  "median_fee_rate_precise": "0.28", "min_fee_rate_possible_precise": "0.10" }
            ] }
    })
}

#[test]
fn fees_show_the_suggested_rate() {
    let m = mock(vec![ok(network_info()), ok(network_info())]);
    sochain(&m, &config())
        .args(["fees", "BTC"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Suggested fee rate 0.28 sat/vB (the next block's median).",
        ));
    let out = sochain(&m, &config())
        .args(["fees", "BTC", "--json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["unit"], "sat/vB");
    assert_eq!(v["data"]["suggested_fee_rate"], "0.28");
    assert_eq!(m.paths(), ["/api/v3/network_info/BTC"; 2]);
}

#[test]
fn warning_headers_are_noted_in_text_mode_only() {
    let behind = || {
        with_header(
            ok(json!({ "hash": HASH })),
            "warning",
            r#"199 - "data is 2 blocks behind""#,
        )
    };
    let m = mock(vec![behind(), behind()]);
    sochain(&m, &config())
        .args(["best-block-hash", "BTC"])
        .assert()
        .success()
        .stderr("sochain: note from SoChain: data is 2 blocks behind\n");
    sochain(&m, &config())
        .args(["best-block-hash", "BTC", "--json"])
        .assert()
        .success()
        .stderr("");
}

#[test]
fn text_from_the_api_cannot_drive_the_terminal() {
    let m = mock(vec![fail(404, "no \u{1b}[31msuch\u{7} block")]);
    sochain(&m, &config())
        .args(["block", "BTC", "1"])
        .assert()
        .code(3)
        .stderr("sochain: no [31msuch block\n");
}

#[test]
fn chart_text_shows_the_latest_points() {
    let points: Vec<Value> = (0..40)
        .map(|i| json!({ "t": 1_790_000_000 + i * 86_400, "v": 1000 + i }))
        .collect();
    let m = mock(vec![ok(
        json!({ "series": "tx_count", "interval": "day", "points": points }),
    )]);
    sochain(&m, &config())
        .args([
            "chart",
            "DOGE",
            "tx_count",
            "--last",
            "2",
            "--from",
            "1790000000",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "DOGE tx_count by day, last 2 of 40 points",
        ))
        .stdout(predicate::str::contains("1,039"));
    assert_eq!(
        m.paths(),
        ["/api/v3/charts/DOGE/tx_count/day?from=1790000000"]
    );
}

#[test]
fn skill_install_verifies_the_digest_and_sends_no_key() {
    let skill = "---\nname: sochain-blockchain-data\ndescription: x\n---\n# SoChain\n";
    let digest = {
        use sha2::{Digest, Sha256};
        let h = Sha256::digest(skill.as_bytes());
        format!(
            "sha256:{}",
            h.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )
    };
    let index = json!({ "skills": [{
        "name": "sochain-blockchain-data", "type": "skill-md",
        "url": "/.well-known/agent-skills/sochain-blockchain-data/SKILL.md", "digest": digest
    }] });
    let mut md = reply(200, json!(null));
    md.content_type = "text/markdown";
    md.body = skill.into();
    let m = mock(vec![reply(200, index), md]);
    let dir = tempfile::tempdir().unwrap();
    sochain(&m, &config())
        .env("SOCHAIN_API_KEY", KEY)
        .args(["skill", "install", "--dir"])
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("installed"));
    let installed = dir.path().join("sochain-blockchain-data").join("SKILL.md");
    assert_eq!(std::fs::read_to_string(installed).unwrap(), skill);
    for i in 0..2 {
        assert_eq!(m.request(i).header("api-key"), None);
    }
}

#[test]
fn skill_install_refuses_a_mismatched_digest() {
    let index = json!({ "skills": [{
        "name": "sochain-blockchain-data", "type": "skill-md",
        "url": "/skill.md", "digest": "sha256:00"
    }] });
    let mut md = reply(200, json!(null));
    md.body = "---\nname: sochain-blockchain-data\n---\n".into();
    let m = mock(vec![reply(200, index), md]);
    let dir = tempfile::tempdir().unwrap();
    sochain(&m, &config())
        .args(["skill", "install", "--dir"])
        .arg(dir.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "does not match its published digest",
        ));
    assert!(!dir.path().join("sochain-blockchain-data").exists());
}

#[test]
fn schema_and_guide_need_no_network() {
    let m = mock(vec![]);
    let out = sochain(&m, &config()).arg("schema").output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["name"], "sochain");
    assert!(v["commands"].as_object().unwrap().len() > 20);
    sochain(&m, &config())
        .arg("guide")
        .assert()
        .success()
        .stdout(predicate::str::contains("Using `sochain` from scripts"));
    assert!(m.paths().is_empty());
}

#[test]
fn help_and_version_exit_0() {
    let m = mock(vec![]);
    sochain(&m, &config()).arg("--help").assert().success();
    sochain(&m, &config())
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("sochain "));
}

#[test]
fn skill_files_must_be_on_the_same_site() {
    let index = json!({ "skills": [{
        "name": "sochain-blockchain-data", "type": "skill-md",
        "url": "https://example.com/SKILL.md", "digest": "sha256:00"
    }] });
    let m = mock(vec![reply(200, index)]);
    sochain(&m, &config())
        .args(["skill", "install", "--print"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not on sochain.com"));
    assert_eq!(m.paths().len(), 1);
}
