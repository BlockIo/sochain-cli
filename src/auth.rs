//! The SoChain API key: where it comes from, and `sochain auth`.
//!
//! The key is read from SOCHAIN_API_KEY or, failing that, from a file saved
//! by `sochain auth login` (readable only by you). It is sent only to
//! sochain.com, in the `API-KEY` header, and never printed.

use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::Api;
use crate::error::{CliError, Result};

pub const ENV_KEY: &str = "SOCHAIN_API_KEY";
const FILE_NAME: &str = "credentials.json";
const MAX_KEY_LEN: usize = 256;

#[derive(Debug, Clone)]
pub enum Source {
    Env,
    File(PathBuf),
}

impl Source {
    fn describe(&self) -> String {
        match self {
            Source::Env => format!("from {ENV_KEY}"),
            Source::File(p) => format!("saved in {}", p.display()),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Credentials {
    api_key: String,
}

/// The key to use, if any: SOCHAIN_API_KEY first, then the saved key.
pub fn resolve() -> Result<Option<(String, Source)>> {
    if let Ok(key) = std::env::var(ENV_KEY) {
        let key = key.trim().to_owned();
        if !key.is_empty() {
            check_shape(&key)?;
            return Ok(Some((key, Source::Env)));
        }
    }
    let Some(path) = credentials_path() else {
        return Ok(None);
    };
    let Ok(text) = fs::read(&path) else {
        return Ok(None);
    };
    let creds: Credentials = serde_json::from_slice(&text).map_err(|_| {
        CliError::Other(format!(
            "{} is not a saved key; run `sochain auth logout` and log in again",
            path.display()
        ))
    })?;
    check_shape(&creds.api_key)?;
    Ok(Some((creds.api_key, Source::File(path))))
}

/// `$SOCHAIN_CONFIG_DIR`, else the platform's config folder plus `sochain`:
/// `$XDG_CONFIG_HOME/sochain` or `~/.config/sochain`, `%APPDATA%\sochain` on
/// Windows.
pub fn config_dir() -> Option<PathBuf> {
    let from_env = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = from_env("SOCHAIN_CONFIG_DIR") {
        return Some(dir);
    }
    let base = if cfg!(windows) {
        from_env("APPDATA")
    } else {
        from_env("XDG_CONFIG_HOME").or_else(|| std::env::home_dir().map(|h| h.join(".config")))
    };
    base.map(|b| b.join("sochain"))
}

fn credentials_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(FILE_NAME))
}

/// Keys are short runs of visible ASCII. Anything else is a paste mistake,
/// and would not survive as an HTTP header anyway.
fn check_shape(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > MAX_KEY_LEN || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(CliError::BadInput(
            "that doesn't look like a SoChain API key (one line, no spaces)".into(),
        ));
    }
    Ok(())
}

/// Reads the key: hidden at a prompt on a terminal, else one line of stdin.
fn read_key() -> Result<String> {
    let key = if io::stdin().is_terminal() {
        eprintln!("Paste your SoChain API key (from https://sochain.com/dashboard).");
        rpassword::prompt_password("API key: ")
            .map_err(|e| CliError::Other(format!("could not read the key: {e}")))?
    } else {
        let mut line = String::new();
        io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| CliError::Other(format!("could not read the key: {e}")))?;
        line
    };
    let key = key.trim().to_owned();
    check_shape(&key)?;
    Ok(key)
}

pub fn login(
    make_api: impl Fn(Option<String>) -> Result<Api>,
    verify: bool,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let key = read_key()?;
    let account = if verify {
        Some(make_api(Some(key.clone()))?.get_plain(&["account_info"])?)
    } else {
        None
    };
    let path = credentials_path()
        .ok_or_else(|| CliError::Other("could not find a folder to save the key in".into()))?;
    save(&path, &key)?;
    let plan = account
        .as_ref()
        .and_then(|a| a["plan"]["name"].as_str().map(str::to_owned));
    let key_name = account
        .as_ref()
        .and_then(|a| a["key"]["name"].as_str().map(str::to_owned));
    if json {
        let data =
            json!({ "saved_to": path, "verified": verify, "plan": plan, "key_name": key_name });
        writeln!(out, "{:#}", json!({ "status": "success", "data": data }))?;
        return Ok(());
    }
    match (plan, key_name) {
        (Some(plan), Some(name)) => {
            writeln!(out, "Logged in with the key \"{name}\" ({plan} plan).")?
        }
        (Some(plan), None) => writeln!(out, "Logged in ({plan} plan).")?,
        _ => writeln!(out, "Saved the key without checking it.")?,
    }
    writeln!(out, "Saved to {} (readable only by you).", path.display())?;
    if std::env::var(ENV_KEY).is_ok_and(|v| !v.trim().is_empty()) {
        writeln!(
            out,
            "Note: {ENV_KEY} is set, and takes precedence over the saved key."
        )?;
    }
    Ok(())
}

fn save(path: &PathBuf, key: &str) -> Result<()> {
    let fail = |e: io::Error| CliError::Other(format!("could not save {}: {e}", path.display()));
    let dir = path.parent().expect("credentials path has a folder");
    fs::create_dir_all(dir).map_err(fail)?;
    let body = serde_json::to_vec_pretty(&Credentials {
        api_key: key.to_owned(),
    })
    .expect("credentials serialize");
    // Write to a temporary file with owner-only permissions, then move it
    // into place, so the key is never readable by others even briefly.
    let tmp = dir.join(format!(".{FILE_NAME}.tmp"));
    let _ = fs::remove_file(&tmp);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    let mut file = options.open(&tmp).map_err(fail)?;
    file.write_all(&body).map_err(fail)?;
    file.sync_all().map_err(fail)?;
    drop(file);
    fs::rename(&tmp, path).map_err(fail)
}

pub fn logout(json: bool, out: &mut dyn Write) -> Result<()> {
    let path = credentials_path()
        .ok_or_else(|| CliError::Other("could not find the config folder".into()))?;
    let removed = match fs::remove_file(&path) {
        Ok(()) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(CliError::Other(format!(
                "could not remove {}: {e}",
                path.display()
            )));
        }
    };
    let env_set = std::env::var(ENV_KEY).is_ok_and(|v| !v.trim().is_empty());
    if json {
        let data = json!({ "removed": removed, "path": path, "env_key_set": env_set });
        writeln!(out, "{:#}", json!({ "status": "success", "data": data }))?;
        return Ok(());
    }
    if removed {
        writeln!(out, "Removed the saved key ({}).", path.display())?;
    } else {
        writeln!(out, "No key was saved.")?;
    }
    if env_set {
        writeln!(out, "{ENV_KEY} is still set in your environment.")?;
    }
    Ok(())
}

pub fn status(
    key: Option<(String, Source)>,
    make_api: impl Fn(Option<String>) -> Result<Api>,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let Some((key, source)) = key else {
        if json {
            let data = json!({ "key": null });
            writeln!(out, "{:#}", json!({ "status": "success", "data": data }))?;
        } else {
            writeln!(
                out,
                "No API key. Lookups still work: at least 2,500 a day per IP address, more when there's room.\n\
                 Get a key at https://sochain.com/signin, then run `sochain auth login` \
                 or set {ENV_KEY}."
            )?;
        }
        return Ok(());
    };
    let account = make_api(Some(key))?.get_plain(&["account_info"])?;
    if json {
        let source = match &source {
            Source::Env => json!({ "kind": "env", "name": ENV_KEY }),
            Source::File(p) => json!({ "kind": "file", "path": p }),
        };
        let data = json!({ "key": { "source": source }, "account": account });
        writeln!(out, "{:#}", json!({ "status": "success", "data": data }))?;
        return Ok(());
    }
    let name = account["key"]["name"]
        .as_str()
        .map(|n| format!(" \"{n}\""))
        .unwrap_or_default();
    let access = match account["key"]["access"].as_str() {
        Some("read_only") => ", read-only",
        _ => "",
    };
    writeln!(out, "Using the key{name}{access}, {}.", source.describe())?;
    if let Some(plan) = account["plan"]["name"].as_str() {
        writeln!(out, "Plan: {plan}. Run `sochain account` for usage.")?;
    }
    Ok(())
}
