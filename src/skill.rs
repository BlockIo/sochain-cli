//! `sochain skill`: installs the SoChain agent skill (a SKILL.md published by
//! sochain.com) into AI coding agents' skill folders.
//!
//! The skill is always fetched from the site's agent-skills index and checked
//! against the digest the index lists. A small marker file next to each
//! installed SKILL.md records what was written, so an update never overwrites
//! a file someone has edited (unless `--force`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::api::Api;
use crate::error::{CliError, Result};

const INDEX_PATH: &str = "/.well-known/agent-skills/index.json";
const SKILL_NAME: &str = "sochain-blockchain-data";
const MAX_INDEX_BYTES: usize = 64 * 1024;
const MAX_SKILL_BYTES: usize = 64 * 1024;
const MARKER: &str = ".sochain-skill.json";

/// An agent that loads skills from `<root>/<skill name>/SKILL.md`.
#[derive(Debug, Clone, Copy)]
pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    /// A folder in the home directory whose presence means the agent is
    /// installed.
    detect: &'static str,
    /// Skills root under the home directory.
    user_root: &'static [&'static str],
    /// Skills root under a project directory.
    project_root: &'static [&'static str],
}

// Claude Code: code.claude.com/docs/en/skills. Codex reads the cross-agent
// `.agents/skills` folders (see agentskills.io).
pub const AGENTS: &[Agent] = &[
    Agent {
        id: "claude",
        name: "Claude Code",
        detect: ".claude",
        user_root: &[".claude", "skills"],
        project_root: &[".claude", "skills"],
    },
    Agent {
        id: "codex",
        name: "Codex",
        detect: ".codex",
        user_root: &[".agents", "skills"],
        project_root: &[".agents", "skills"],
    },
];

/// Where to install, as chosen on the command line.
pub enum Scope {
    /// Every detected agent's user-level skills folder.
    User,
    /// Every agent's folder inside the current directory.
    Project,
    /// One explicit skills root.
    Dir(PathBuf),
}

#[derive(Debug, Clone, Serialize)]
pub struct Target {
    pub agent: String,
    /// The skill folder (containing SKILL.md).
    pub path: PathBuf,
}

pub struct Skill {
    pub name: &'static str,
    pub digest: String,
    pub bytes: Vec<u8>,
}

#[derive(Deserialize)]
struct Index {
    skills: Vec<IndexEntry>,
}

#[derive(Deserialize)]
struct IndexEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    digest: String,
}

#[derive(Serialize, Deserialize)]
struct Marker {
    digest: String,
    installed_by: String,
}

/// Fetches the published skill and verifies it against the index digest.
pub fn fetch(api: &Api) -> Result<Skill> {
    let index = api.fetch_site_file(INDEX_PATH, MAX_INDEX_BYTES)?;
    let index: Index = serde_json::from_slice(&index)
        .map_err(|e| CliError::Other(format!("unexpected agent-skills index: {e}")))?;
    let entry = index
        .skills
        .into_iter()
        .find(|s| s.name == SKILL_NAME && s.kind == "skill-md")
        .ok_or_else(|| CliError::NotFound("the SoChain skill is not published".into()))?;
    let bytes = api.fetch_site_file(&entry.url, MAX_SKILL_BYTES)?;
    let digest = sha256(&bytes);
    if !entry.digest.eq_ignore_ascii_case(&digest) {
        return Err(CliError::Other(format!(
            "the downloaded skill does not match its published digest \
             (expected {}, got {digest}); nothing was installed",
            entry.digest
        )));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| CliError::Other("the skill is not valid UTF-8".into()))?;
    if front_matter_name(text) != Some(SKILL_NAME) {
        return Err(CliError::Other(format!(
            "the downloaded skill is not named {SKILL_NAME:?}; nothing was installed"
        )));
    }
    Ok(Skill {
        name: SKILL_NAME,
        digest,
        bytes,
    })
}

pub fn sha256(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// The `name:` field of a SKILL.md's YAML front matter.
fn front_matter_name(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("---")?.trim_start_matches(['\r', '\n']);
    let (front, _) = rest.split_once("\n---")?;
    front.lines().find_map(|line| {
        line.strip_prefix("name:")
            .map(|v| v.trim().trim_matches(['"', '\'']))
    })
}

/// The skill folders a command applies to. With `Scope::User`, only agents
/// that appear to be installed are included.
pub fn targets(scope: &Scope) -> Result<Vec<Target>> {
    let folder = |root: PathBuf| root.join(SKILL_NAME);
    match scope {
        Scope::Dir(dir) => Ok(vec![Target {
            agent: "custom".into(),
            path: folder(dir.clone()),
        }]),
        Scope::Project => {
            let cwd = std::env::current_dir()
                .map_err(|e| CliError::Other(format!("no current directory: {e}")))?;
            Ok(AGENTS
                .iter()
                .map(|a| Target {
                    agent: a.id.into(),
                    path: folder(join(&cwd, a.project_root)),
                })
                .collect())
        }
        Scope::User => {
            let home = std::env::home_dir()
                .ok_or_else(|| CliError::Other("could not find your home directory".into()))?;
            let found: Vec<Target> = AGENTS
                .iter()
                .filter(|a| home.join(a.detect).is_dir())
                .map(|a| Target {
                    agent: a.id.into(),
                    path: folder(join(&home, a.user_root)),
                })
                .collect();
            if found.is_empty() {
                return Err(CliError::NotFound(
                    "no supported agent found (looked for ~/.claude and ~/.codex); \
                     use --dir <path> or --print"
                        .into(),
                ));
            }
            Ok(found)
        }
    }
}

fn join(base: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(base.to_path_buf(), |p, part| p.join(part))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    NotInstalled,
    UpToDate,
    Outdated,
    /// SKILL.md differs from what the CLI wrote: someone edited it, or it was
    /// not installed by the CLI.
    Modified,
}

impl State {
    fn describe(self) -> &'static str {
        match self {
            State::NotInstalled => "not installed",
            State::UpToDate => "up to date",
            State::Outdated => "installed, a newer version is published",
            State::Modified => "installed, but changed since the CLI wrote it",
        }
    }
}

fn state(target: &Target, published: Option<&str>) -> State {
    let Ok(current) = fs::read(target.path.join("SKILL.md")) else {
        return State::NotInstalled;
    };
    let current = sha256(&current);
    let written = fs::read(target.path.join(MARKER))
        .ok()
        .and_then(|m| serde_json::from_slice::<Marker>(&m).ok())
        .map(|m| m.digest);
    if written.as_deref() != Some(current.as_str()) {
        return State::Modified;
    }
    match published {
        Some(p) if p != current => State::Outdated,
        _ => State::UpToDate,
    }
}

#[derive(Serialize)]
struct Outcome {
    agent: String,
    path: PathBuf,
    result: &'static str,
}

pub fn install(
    api: &Api,
    scope: &Scope,
    force: bool,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let skill = fetch(api)?;
    let mut outcomes = Vec::new();
    let mut skipped = false;
    for target in targets(scope)? {
        let result = match state(&target, Some(&skill.digest)) {
            State::UpToDate => "unchanged",
            State::Modified if !force => {
                skipped = true;
                "skipped: changed since installed (use --force to replace)"
            }
            before => {
                write_skill(&target.path, &skill)?;
                if before == State::NotInstalled {
                    "installed"
                } else {
                    "updated"
                }
            }
        };
        outcomes.push(Outcome {
            agent: target.agent,
            path: target.path.join("SKILL.md"),
            result,
        });
    }
    if json {
        let data =
            json!({ "skill": { "name": SKILL_NAME, "digest": skill.digest }, "targets": outcomes });
        writeln!(out, "{:#}", json!({ "status": "success", "data": data }))?;
    } else {
        for o in &outcomes {
            writeln!(
                out,
                "{}: {} ({})",
                agent_name(&o.agent),
                o.result,
                o.path.display()
            )?;
        }
    }
    if skipped {
        return Err(CliError::Other(
            "some copies were not updated because they were changed".into(),
        ));
    }
    Ok(())
}

fn write_skill(dir: &Path, skill: &Skill) -> Result<()> {
    let fail =
        |e: std::io::Error| CliError::Other(format!("could not write to {}: {e}", dir.display()));
    fs::create_dir_all(dir).map_err(fail)?;
    // Write to a temporary file first so a failure never leaves half a file.
    let tmp = dir.join(".SKILL.md.tmp");
    fs::write(&tmp, &skill.bytes).map_err(fail)?;
    fs::rename(&tmp, dir.join("SKILL.md")).map_err(fail)?;
    let marker = Marker {
        digest: skill.digest.clone(),
        installed_by: concat!("sochain-cli ", env!("CARGO_PKG_VERSION")).into(),
    };
    let marker = serde_json::to_vec_pretty(&marker).expect("marker serializes");
    fs::write(dir.join(MARKER), marker).map_err(fail)
}

pub fn status(api: &Api, scope: &Scope, json: bool, out: &mut dyn Write) -> Result<()> {
    // Status still works offline; it just cannot tell whether an update exists.
    let published = fetch(api).ok().map(|s| s.digest);
    let rows: Vec<_> = targets(scope)?
        .into_iter()
        .map(|t| {
            let st = state(&t, published.as_deref());
            (t, st)
        })
        .collect();
    if json {
        let targets: Vec<_> = rows
            .iter()
            .map(
                |(t, st)| json!({ "agent": t.agent, "path": t.path.join("SKILL.md"), "state": st }),
            )
            .collect();
        let data = json!({ "published_digest": published, "targets": targets });
        writeln!(out, "{:#}", json!({ "status": "success", "data": data }))?;
    } else {
        for (t, st) in &rows {
            writeln!(
                out,
                "{}: {} ({})",
                agent_name(&t.agent),
                st.describe(),
                t.path.join("SKILL.md").display()
            )?;
        }
        if published.is_none() {
            writeln!(out, "(could not reach sochain.com to check for updates)")?;
        }
    }
    Ok(())
}

pub fn uninstall(scope: &Scope, force: bool, json: bool, out: &mut dyn Write) -> Result<()> {
    let mut outcomes = Vec::new();
    let mut skipped = false;
    for target in targets(scope)? {
        let result = match state(&target, None) {
            State::NotInstalled => "not installed",
            State::Modified if !force => {
                skipped = true;
                "skipped: changed since installed (use --force to remove)"
            }
            _ => {
                let fail = |e: std::io::Error| {
                    CliError::Other(format!("could not remove {}: {e}", target.path.display()))
                };
                fs::remove_file(target.path.join("SKILL.md")).map_err(fail)?;
                let _ = fs::remove_file(target.path.join(MARKER));
                // Leave the folder if anything else is in it.
                let _ = fs::remove_dir(&target.path);
                "removed"
            }
        };
        outcomes.push(Outcome {
            agent: target.agent,
            path: target.path.join("SKILL.md"),
            result,
        });
    }
    if json {
        writeln!(
            out,
            "{:#}",
            json!({ "status": "success", "data": { "targets": outcomes } })
        )?;
    } else {
        for o in &outcomes {
            writeln!(
                out,
                "{}: {} ({})",
                agent_name(&o.agent),
                o.result,
                o.path.display()
            )?;
        }
    }
    if skipped {
        return Err(CliError::Other(
            "some copies were not removed because they were changed".into(),
        ));
    }
    Ok(())
}

fn agent_name(id: &str) -> &str {
    AGENTS
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.name)
        .unwrap_or("Folder")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_front_matter_name() {
        let md = "---\nname: sochain-blockchain-data\ndescription: x\n---\n# Body\n";
        assert_eq!(front_matter_name(md), Some("sochain-blockchain-data"));
        assert_eq!(
            front_matter_name("---\nname: \"other\"\n---\n"),
            Some("other")
        );
        assert_eq!(front_matter_name("# no front matter"), None);
    }

    #[test]
    fn hashes_like_the_index() {
        assert_eq!(
            sha256(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
