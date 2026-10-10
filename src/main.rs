//! `sochain`: Bitcoin, Litecoin, Dogecoin and Zcash blockchain data from your
//! terminal, via SoChain.

mod amount;
mod api;
mod auth;
mod cli;
mod commands;
mod error;
mod mcp;
mod model;
mod network;
mod sanitize;
mod schema;
mod secret;
mod skill;
mod time;

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind;

use crate::cli::Cli;
use crate::error::{CliError, exit};

/// Printed by `sochain guide`; the same text is AGENTS.md in the repository.
pub const GUIDE: &str = include_str!("../AGENTS.md");

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => return usage_error(e),
    };
    let json = cli.json;
    let stdout = io::BufWriter::new(io::stdout().lock());
    let mut out: Box<dyn Write> = if json {
        Box::new(stdout)
    } else {
        Box::new(sanitize::Sanitized(stdout))
    };
    let result = commands::run(cli, &mut out).and_then(|()| Ok(out.flush()?));
    match result {
        Ok(()) | Err(CliError::OutputClosed) => ExitCode::from(exit::OK),
        Err(e) => {
            // Whatever was printed before the failure still goes out.
            let _ = out.flush();
            report(&e, json);
            ExitCode::from(e.exit_code())
        }
    }
}

fn report(e: &CliError, json: bool) {
    let mut err = io::stderr().lock();
    let _ = if json {
        writeln!(err, "{}", e.to_json())
    } else {
        writeln!(err, "sochain: {}", sanitize::clean(&e.to_string()))
    };
}

/// Help and --version exit 0. Other argument errors exit 2, as JSON on
/// stderr when --json was asked for.
fn usage_error(e: clap::Error) -> ExitCode {
    if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
        let _ = e.print();
        return ExitCode::from(exit::OK);
    }
    if json_requested() {
        let message = e.kind().to_string();
        let detail = e.to_string();
        let first_line = detail
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("error: "))
            .unwrap_or(&message);
        report(&CliError::BadInput(first_line.to_owned()), true);
    } else {
        let _ = e.print();
    }
    ExitCode::from(exit::BAD_INPUT)
}

/// Whether --json was given, read without clap because parsing failed.
fn json_requested() -> bool {
    let flag = std::env::args_os()
        .skip(1)
        .take_while(|a| a != "--")
        .any(|a| a == "--json");
    let env = std::env::var("SOCHAIN_JSON")
        .map(|v| {
            !matches!(
                v.to_ascii_lowercase().as_str(),
                "" | "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(false);
    flag || env
}
