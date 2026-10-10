//! `sochain mcp`: registers the SoChain MCP server (hosted at sochain.com;
//! nothing runs locally) with AI agents, using each agent's own command-line
//! tool. The CLI never edits another program's config files; for agents
//! without such a tool, `--print` shows what to add by hand.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde::Serialize;
use serde_json::{Value, json};

use crate::auth::ENV_KEY;
use crate::error::{CliError, Result};

pub const SERVER_NAME: &str = "sochain";
pub const SERVER_URL: &str = "https://sochain.com/mcp";

/// How the agent authenticates to the server.
#[derive(Clone)]
pub enum Auth {
    /// No key: the keyless allowance per IP address.
    None,
    /// The agent reads SOCHAIN_API_KEY from its environment when it starts
    /// (Codex; Claude Code project files).
    EnvVar,
    /// The key itself is written to the agent's own user config (Claude Code
    /// user scope, which has no env-var reference for headers).
    Key(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agent {
    Codex,
    Claude,
}

/// Codex first, then Claude Code.
const AGENTS: [Agent; 2] = [Agent::Codex, Agent::Claude];

impl Agent {
    fn id(self) -> &'static str {
        match self {
            Agent::Codex => "codex",
            Agent::Claude => "claude",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Agent::Codex => "Codex",
            Agent::Claude => "Claude Code",
        }
    }

    fn program(self) -> &'static str {
        self.id()
    }

    /// Codex has no per-project MCP configuration.
    fn supports_project(self) -> bool {
        matches!(self, Agent::Claude)
    }

    fn add_args(self, project: bool, auth: &Auth) -> Vec<String> {
        let mut args: Vec<String> = match self {
            Agent::Codex => ["mcp", "add", SERVER_NAME, "--url", SERVER_URL]
                .map(String::from)
                .to_vec(),
            Agent::Claude => [
                "mcp",
                "add",
                "--transport",
                "http",
                "--scope",
                if project { "project" } else { "user" },
                SERVER_NAME,
                SERVER_URL,
            ]
            .map(String::from)
            .to_vec(),
        };
        match (self, auth) {
            (_, Auth::None) => {}
            (Agent::Codex, _) => {
                args.extend(["--bearer-token-env-var".into(), ENV_KEY.into()]);
            }
            (Agent::Claude, Auth::EnvVar) => {
                // Claude Code expands ${VAR} in project .mcp.json files.
                args.extend(["--header".into(), format!("API-KEY: ${{{ENV_KEY}}}")]);
            }
            (Agent::Claude, Auth::Key(key)) => {
                args.extend(["--header".into(), format!("API-KEY: {key}")]);
            }
        }
        args
    }

    fn remove_args(self, project: bool) -> Vec<&'static str> {
        match self {
            Agent::Codex => vec!["mcp", "remove", SERVER_NAME],
            Agent::Claude => vec![
                "mcp",
                "remove",
                "--scope",
                if project { "project" } else { "user" },
                SERVER_NAME,
            ],
        }
    }

    /// The URL of the agent's `sochain` server, if it has one.
    fn configured_url(self, program: &Path) -> Result<Option<String>> {
        match self {
            Agent::Codex => {
                let out = run(program, &["mcp", "get", SERVER_NAME, "--json"])?;
                if !out.status.success() {
                    return Ok(None);
                }
                if String::from_utf8_lossy(&out.stdout).contains(SERVER_URL) {
                    return Ok(Some(SERVER_URL.to_owned()));
                }
                let v: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
                let url = v["transport"]["url"].as_str().or(v["url"].as_str());
                Ok(Some(url.unwrap_or_default().to_owned()))
            }
            Agent::Claude => {
                let out = run(program, &["mcp", "get", SERVER_NAME])?;
                if !out.status.success() {
                    return Ok(None);
                }
                let text = String::from_utf8_lossy(&out.stdout);
                if text.contains(SERVER_URL) {
                    return Ok(Some(SERVER_URL.to_owned()));
                }
                Ok(Some(
                    text.lines()
                        .find_map(|l| l.trim().strip_prefix("URL:"))
                        .map(|u| u.trim().to_owned())
                        .unwrap_or_default(),
                ))
            }
        }
    }
}

/// Finds a program on PATH, including Windows `.exe` and `.cmd` shims.
fn find_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: Vec<String> = if cfg!(windows) {
        vec![
            format!("{name}.exe"),
            format!("{name}.cmd"),
            name.to_owned(),
        ]
    } else {
        vec![name.to_owned()]
    };
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

fn run<S: AsRef<std::ffi::OsStr>>(program: &Path, args: &[S]) -> Result<Output> {
    Command::new(program)
        .args(args)
        .output()
        .map_err(|e| CliError::Other(format!("could not run {}: {e}", program.display())))
}

#[derive(Serialize)]
struct Outcome {
    agent: &'static str,
    result: String,
}

fn report(outcomes: &[Outcome], json: bool, out: &mut dyn Write) -> Result<()> {
    if json {
        writeln!(
            out,
            "{:#}",
            json!({ "status": "success", "data": { "server": SERVER_URL, "agents": outcomes } })
        )?;
    } else {
        for o in outcomes {
            let name = AGENTS
                .iter()
                .find(|a| a.id() == o.agent)
                .map(|a| a.name())
                .unwrap_or(o.agent);
            writeln!(out, "{name}: {}", o.result)?;
        }
    }
    Ok(())
}

fn found_agents() -> Result<Vec<(Agent, PathBuf)>> {
    let found: Vec<_> = AGENTS
        .iter()
        .filter_map(|&a| find_program(a.program()).map(|p| (a, p)))
        .collect();
    if found.is_empty() {
        return Err(CliError::NotFound(
            "neither `codex` nor `claude` was found on PATH; \
             run `sochain mcp install --print` for other apps"
                .into(),
        ));
    }
    Ok(found)
}

/// The agent's own error, with anything key-shaped kept out of it.
fn command_failed(agent: Agent, out: &Output, auth: &Auth) -> CliError {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let detail = stderr
        .lines()
        .chain(stdout.lines())
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no details");
    let detail = match auth {
        Auth::Key(key) => detail.replace(key.as_str(), "<key>"),
        _ => detail.to_owned(),
    };
    CliError::Other(format!(
        "{} could not update its MCP servers: {detail}",
        agent.name()
    ))
}

/// The auth an agent gets: Claude Code's user scope stores the key itself,
/// everything else references SOCHAIN_API_KEY.
fn auth_for(agent: Agent, project: bool, auth: &Auth) -> Auth {
    match (agent, auth) {
        (Agent::Claude, Auth::Key(_)) if project => Auth::EnvVar,
        (Agent::Codex, Auth::Key(_)) => Auth::EnvVar,
        _ => auth.clone(),
    }
}

pub fn install(
    project: bool,
    auth: Auth,
    force: bool,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let mut outcomes = Vec::new();
    let mut skipped = false;
    let keyed = !matches!(auth, Auth::None);
    for (agent, program) in found_agents()? {
        if project && !agent.supports_project() {
            outcomes.push(Outcome {
                agent: agent.id(),
                result: "skipped: no per-project setting; run without --project".into(),
            });
            continue;
        }
        let auth = auth_for(agent, project, &auth);
        let result = match agent.configured_url(&program)? {
            // Re-adding with a key replaces a keyless entry.
            Some(url) if url == SERVER_URL && !keyed && !force => "already added".to_owned(),
            Some(url) if url != SERVER_URL && !force => {
                skipped = true;
                format!(
                    "skipped: a server named {SERVER_NAME} already points to {} (use --force to replace)",
                    if url.is_empty() {
                        "something else"
                    } else {
                        &url
                    }
                )
            }
            existing => {
                if existing.is_some() {
                    let removed = run(&program, &agent.remove_args(project))?;
                    if !removed.status.success() {
                        return Err(command_failed(agent, &removed, &auth));
                    }
                }
                let added = run(&program, &agent.add_args(project, &auth))?;
                if !added.status.success() {
                    return Err(command_failed(agent, &added, &auth));
                }
                let how = match auth {
                    Auth::None => "",
                    Auth::EnvVar => ", reading the key from SOCHAIN_API_KEY",
                    Auth::Key(_) => ", with your API key",
                };
                format!(
                    "{}{how}",
                    if existing.is_some() {
                        "replaced"
                    } else {
                        "added"
                    }
                )
            }
        };
        outcomes.push(Outcome {
            agent: agent.id(),
            result,
        });
    }
    report(&outcomes, json, out)?;
    if !json {
        writeln!(
            out,
            "Other apps (Cursor, VS Code, Claude.ai, ChatGPT): sochain mcp install --print"
        )?;
    }
    if skipped {
        return Err(CliError::Other(
            "some agents were not changed because they already had a different sochain server"
                .into(),
        ));
    }
    Ok(())
}

pub fn status(json: bool, out: &mut dyn Write) -> Result<()> {
    let mut outcomes = Vec::new();
    for agent in AGENTS {
        let result = match find_program(agent.program()) {
            None => "not found on PATH".to_owned(),
            Some(program) => match agent.configured_url(&program)? {
                None => "not added".to_owned(),
                Some(url) if url == SERVER_URL => format!("added ({SERVER_URL})"),
                Some(url) => format!("a server named {SERVER_NAME} points to {url}"),
            },
        };
        outcomes.push(Outcome {
            agent: agent.id(),
            result,
        });
    }
    report(&outcomes, json, out)
}

pub fn uninstall(project: bool, force: bool, json: bool, out: &mut dyn Write) -> Result<()> {
    let mut outcomes = Vec::new();
    let mut skipped = false;
    for (agent, program) in found_agents()? {
        if project && !agent.supports_project() {
            outcomes.push(Outcome {
                agent: agent.id(),
                result: "skipped: no per-project setting".into(),
            });
            continue;
        }
        let result = match agent.configured_url(&program)? {
            None => "not added".to_owned(),
            Some(url) if url != SERVER_URL && !force => {
                skipped = true;
                format!("skipped: {SERVER_NAME} points to {url}, not SoChain (use --force)")
            }
            Some(_) => {
                let removed = run(&program, &agent.remove_args(project))?;
                if !removed.status.success() {
                    return Err(command_failed(agent, &removed, &Auth::None));
                }
                "removed".to_owned()
            }
        };
        outcomes.push(Outcome {
            agent: agent.id(),
            result,
        });
    }
    report(&outcomes, json, out)?;
    if skipped {
        return Err(CliError::Other(
            "some agents were not changed because their sochain server is not SoChain's".into(),
        ));
    }
    Ok(())
}

/// Setup for every supported app, for copying by hand. Changes nothing, and
/// never prints a key: commands use $SOCHAIN_API_KEY.
pub fn print(project: bool, with_key: bool, json: bool, out: &mut dyn Write) -> Result<()> {
    let header = format!("API-KEY: ${ENV_KEY}");
    let codex = if with_key {
        format!("codex mcp add {SERVER_NAME} --url {SERVER_URL} --bearer-token-env-var {ENV_KEY}")
    } else {
        format!("codex mcp add {SERVER_NAME} --url {SERVER_URL}")
    };
    let scope = if project { " --scope project" } else { "" };
    let claude = if with_key {
        format!(
            "claude mcp add --transport http{scope} {SERVER_NAME} {SERVER_URL} --header \"{header}\""
        )
    } else {
        format!("claude mcp add --transport http{scope} {SERVER_NAME} {SERVER_URL}")
    };
    let headers = json!({ "API-KEY": "<your SoChain API key>" });
    let (cursor, vscode) = if with_key {
        (
            json!({ "mcpServers": { SERVER_NAME: { "url": SERVER_URL, "headers": headers } } }),
            json!({ "servers": { SERVER_NAME: { "type": "http", "url": SERVER_URL, "headers": headers } } }),
        )
    } else {
        (
            json!({ "mcpServers": { SERVER_NAME: { "url": SERVER_URL } } }),
            json!({ "servers": { SERVER_NAME: { "type": "http", "url": SERVER_URL } } }),
        )
    };
    let cursor_file = if project {
        ".cursor/mcp.json"
    } else {
        "~/.cursor/mcp.json"
    };
    if json {
        let data = json!({
            "server": SERVER_URL,
            "codex": { "command": codex },
            "claude_code": { "command": claude },
            "cursor": { "file": cursor_file, "add": cursor },
            "vscode": { "file": ".vscode/mcp.json", "add": vscode },
            "claude_app": "Settings → Connectors → Add custom connector → the server URL (no key)",
            "chatgpt": "Settings → Apps & Connectors → Advanced settings → turn on Developer mode, then Create → the server URL, No authentication",
        });
        writeln!(out, "{:#}", json!({ "status": "success", "data": data }))?;
        return Ok(());
    }
    let auth = if with_key {
        "with your API key"
    } else {
        "no key needed; add one with --use-api-key for your plan's limits"
    };
    writeln!(
        out,
        "SoChain MCP server: {SERVER_URL} (Streamable HTTP; {auth})\n"
    )?;
    writeln!(out, "Codex:\n  {codex}\n")?;
    writeln!(out, "Claude Code:\n  {claude}\n")?;
    writeln!(out, "Cursor: add to {cursor_file}\n  {cursor}\n")?;
    writeln!(out, "VS Code: add to .vscode/mcp.json\n  {vscode}\n")?;
    writeln!(
        out,
        "Claude.ai and Claude Desktop: Settings → Connectors → Add custom connector → {SERVER_URL}\n"
    )?;
    writeln!(
        out,
        "ChatGPT: Settings → Apps & Connectors → Advanced settings → turn on Developer mode, \
         then Create → {SERVER_URL}, No authentication"
    )?;
    writeln!(
        out,
        "\nMerge these into existing files rather than replacing them."
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_never_gets_the_key_itself() {
        let args = Agent::Codex.add_args(
            false,
            &auth_for(Agent::Codex, false, &Auth::Key("k".into())),
        );
        assert!(args.ends_with(&["--bearer-token-env-var".into(), ENV_KEY.into()]));
        assert!(!args.iter().any(|a| a == "k"));
    }

    #[test]
    fn project_files_reference_the_env_var() {
        let auth = auth_for(Agent::Claude, true, &Auth::Key("secret".into()));
        let args = Agent::Claude.add_args(true, &auth);
        assert!(args.contains(&"API-KEY: ${SOCHAIN_API_KEY}".to_owned()));
        assert!(!args.iter().any(|a| a.contains("secret")));
    }

    #[test]
    fn keyless_adds_no_header() {
        let args = Agent::Claude.add_args(false, &Auth::None);
        assert!(!args.iter().any(|a| a == "--header"));
    }
}
