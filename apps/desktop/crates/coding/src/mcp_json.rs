//! `.exp-mcp.json` for the spawned `claude` (masterplan-v3 §7.1 step 4):
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "exponential": {
//!       "type": "http",
//!       "url": "<baseUrl>/api/mcp",
//!       "headers": { "Authorization": "Bearer <expu_ personal key>" }
//!     }
//!   }
//! }
//! ```
//!
//! `<baseUrl>` is the signed-in server origin (the api session's normalized
//! instance URL). The `expu_` key is the hidden auto-minted personal key
//! (§7.2) — **this file is the ONLY place the raw key lands on disk in a
//! coding session** (the worktree lives under the user's own repos root),
//! and it must ALWAYS be git-ignored where it lands (EXP-474 — enforced,
//! not best-effort, by `crate::git_worktree::ensure_ignored` at the write
//! site). It authenticates the spawned `claude` as the real signed-in user
//! against `/api/mcp`, exposing the `exponential_*` MCP tools.
//!
//! The file is deliberately NOT named `.mcp.json` (EXP-98): claude's
//! interactive startup runs an UNCONDITIONAL approval scan of the
//! project-scope config — the literal `.mcp.json` in the cwd — and raises the
//! "New MCP server found in this project" dialog for every not-yet-approved
//! server, ignoring `--mcp-config`/`--strict-mcp-config` entirely (those
//! flags only gate which servers CONNECT). Every fresh worktree starts with
//! no approval state, so the EXP-83 flags alone could never kill the dialog.
//! A name project-scope discovery never sees can — the config still rides
//! `--mcp-config` and connects trusted, prompt-free.

use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::argv::{McpServerWire, McpWireTransport};

pub const MCP_JSON_FILE: &str = ".exp-mcp.json";

/// The pre-EXP-98 file name. Worktrees/clones prepared by older app versions
/// still carry it, and its mere presence re-raises claude's project-approval
/// dialog — see [`remove_stale_legacy_mcp_json`].
pub const LEGACY_MCP_JSON_FILE: &str = ".mcp.json";

// Struct (not `serde_json::json!`) so key order is declaration order —
// serde_json's default Map would alphabetize and break byte-stable output.
#[derive(Serialize)]
struct McpFile<'a> {
    #[serde(rename = "mcpServers")]
    mcp_servers: McpServers<'a>,
}

/// `exponential` FIRST (a declared field), then (EXP-792) every team server
/// under its config key. The flattened map is sorted, but that only orders
/// the servers among THEMSELVES; `exponential` always leads, and an empty
/// map flattens to nothing, so the no-extra-servers document is byte-stable.
#[derive(Serialize)]
struct McpServers<'a> {
    exponential: McpServer<'a>,
    #[serde(flatten)]
    servers: BTreeMap<String, McpServerEntry>,
}

#[derive(Serialize)]
struct McpServer<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    url: String,
    headers: Headers<'a>,
}

/// EXP-792: a team server in claude's `mcpServers` vocabulary. Header and
/// env VALUES are the launcher's `${VAR}` references — claude expands them
/// from the child env, so this document carries no credential either.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum McpServerEntry {
    Http {
        url: String,
        headers: BTreeMap<String, String>,
    },
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
}

impl From<&McpServerWire> for McpServerEntry {
    fn from(server: &McpServerWire) -> Self {
        match &server.transport {
            McpWireTransport::Http { url } => McpServerEntry::Http {
                url: url.clone(),
                headers: server.headers.iter().cloned().collect(),
            },
            McpWireTransport::Stdio { command, args } => McpServerEntry::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: server.env.iter().cloned().collect(),
            },
        }
    }
}

#[derive(Serialize)]
struct Headers<'a> {
    #[serde(rename = "Authorization")]
    authorization: String,
    /// EXP-637: the `coding_sessions` row this launch created — the server
    /// resolves `exponential_sessions_end` (and the self-merge spare) from
    /// it. Not a secret; absent entirely outside a launched session, which
    /// keeps the pre-EXP-637 document byte-identical.
    #[serde(rename = "X-Exp-Session-Id", skip_serializing_if = "Option::is_none")]
    session_id: Option<&'a str>,
}

/// Render the exact file content (2-space pretty JSON + trailing newline).
/// `base_url` tolerates a trailing slash. `session_id` (EXP-637) adds the
/// `X-Exp-Session-Id` header; `None` renders the legacy document unchanged.
pub fn render_mcp_json(base_url: &str, personal_key: &str, session_id: Option<&str>) -> String {
    render_mcp_json_with(base_url, personal_key, session_id, &[])
}

/// [`render_mcp_json`] plus (EXP-792) the launch's team servers after
/// `exponential`. The agent SHELL (this file's only consumer) never carries
/// any, so the launcher always renders the bare document; the parameter
/// keeps this renderer in step with the engine's inline one.
pub fn render_mcp_json_with(
    base_url: &str,
    personal_key: &str,
    session_id: Option<&str>,
    servers: &[McpServerWire],
) -> String {
    let origin = base_url.trim_end_matches('/');
    let file = McpFile {
        mcp_servers: McpServers {
            exponential: McpServer {
                kind: "http",
                url: format!("{origin}/api/mcp"),
                headers: Headers {
                    authorization: format!("Bearer {personal_key}"),
                    session_id,
                },
            },
            servers: servers
                .iter()
                // A server that folded to our own key would shadow the
                // tracker's tools; the resolver refuses it, this is the belt.
                .filter(|server| server.name != "exponential")
                .map(|server| (server.name.clone(), McpServerEntry::from(server)))
                .collect(),
        },
    };
    let mut rendered = serde_json::to_string_pretty(&file).expect("mcp json serialize");
    rendered.push('\n');
    rendered
}

/// Write `.exp-mcp.json` into the worktree root, `0600` on unix (it carries
/// the raw personal key; `claude` runs as the same user). Overwrites every
/// launch so a regenerated key (§7.2) is picked up on the next session, and
/// reclaims a stale pre-EXP-98 `.mcp.json` so reused worktrees stop
/// re-raising claude's project-approval dialog.
///
/// EXP-474: callers must guarantee the file is git-ignored BEFORE writing —
/// the enforcing caller is the launcher's `wire_agent_mcp`, which hard-fails
/// the launch via [`crate::git_worktree::ensure_ignored`] first.
pub fn write_mcp_json(
    worktree: &Path,
    base_url: &str,
    personal_key: &str,
    session_id: Option<&str>,
) -> io::Result<PathBuf> {
    let path = worktree.join(MCP_JSON_FILE);
    let content = render_mcp_json(base_url, personal_key, session_id);
    write_private(&path, &content)?;
    remove_stale_legacy_mcp_json(worktree);
    Ok(path)
}

/// Best-effort delete of a stale launcher-written `.mcp.json` (the pre-EXP-98
/// name) in `dir` — its mere presence re-raises claude's project-approval
/// dialog (the startup scan ignores `--strict-mcp-config`). Only a file that
/// is PROVABLY ours goes (`mcpServers.exponential.headers.Authorization`
/// bearing an `expu_` key): a repo-committed `.mcp.json` is tracked content
/// whose deletion would surface as a git change claude might commit, so any
/// other shape is left alone.
pub fn remove_stale_legacy_mcp_json(dir: &Path) {
    let path = dir.join(LEGACY_MCP_JSON_FILE);
    let Ok(raw) = fs::read_to_string(&path) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };
    let launcher_written = value
        .get("mcpServers")
        .and_then(|servers| servers.get("exponential"))
        .and_then(|server| server.get("headers"))
        .and_then(|headers| headers.get("Authorization"))
        .and_then(|auth| auth.as_str())
        .is_some_and(|auth| auth.starts_with("Bearer expu_"));
    if launcher_written {
        let _ = fs::remove_file(&path);
    }
}

/// 0600-perms-before-content write (same posture as the api token store).
pub(crate) fn write_private(path: &Path, content: &str) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    // An existing file keeps its old mode — retighten explicitly.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    use std::io::Write as _;
    file.write_all(content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §7.1 step 4's exact document — byte-for-byte.
    const EXPECTED: &str = r#"{
  "mcpServers": {
    "exponential": {
      "type": "http",
      "url": "https://app.exponential.at/api/mcp",
      "headers": {
        "Authorization": "Bearer expu_rawkey123"
      }
    }
  }
}
"#;

    #[test]
    fn renders_exact_bytes() {
        assert_eq!(
            render_mcp_json("https://app.exponential.at", "expu_rawkey123", None),
            EXPECTED
        );
    }

    /// EXP-637: a launched session pins its row id in a THIRD header line —
    /// the rest of the document is unchanged, so old readers keep working.
    #[test]
    fn renders_the_session_header_when_launched() {
        let with_session =
            render_mcp_json("https://app.exponential.at", "expu_rawkey123", Some("sess-1"));
        assert_eq!(
            with_session,
            r#"{
  "mcpServers": {
    "exponential": {
      "type": "http",
      "url": "https://app.exponential.at/api/mcp",
      "headers": {
        "Authorization": "Bearer expu_rawkey123",
        "X-Exp-Session-Id": "sess-1"
      }
    }
  }
}
"#
        );
    }

    #[test]
    fn trailing_slash_on_base_url_is_normalized() {
        assert_eq!(
            render_mcp_json("https://app.exponential.at/", "expu_rawkey123", None),
            EXPECTED
        );
    }

    /// EXP-792: team servers follow `exponential` under their config keys
    /// with `${VAR}` references verbatim; an empty pick is the exact legacy
    /// document, and a server that folded to our own key is dropped.
    #[test]
    fn team_servers_follow_exponential_and_an_empty_pick_is_byte_identical() {
        use crate::argv::{McpServerWire, McpWireTransport};
        assert_eq!(
            render_mcp_json_with("https://app.exponential.at", "expu_rawkey123", None, &[]),
            EXPECTED
        );
        let servers = vec![
            McpServerWire {
                id: "srv-1".to_string(),
                name: "linear".to_string(),
                transport: McpWireTransport::Http {
                    url: "https://mcp.linear.app/mcp".to_string(),
                },
                headers: vec![("Authorization".to_string(), "Bearer ${EXP_MCP_TOKEN_1}".to_string())],
                token_env: Some("EXP_MCP_TOKEN_1".to_string()),
                env: Vec::new(),
            },
            McpServerWire {
                id: "srv-2".to_string(),
                name: "github".to_string(),
                transport: McpWireTransport::Stdio {
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "@acme/github-mcp".to_string()],
                },
                headers: Vec::new(),
                token_env: None,
                env: vec![("GITHUB_TOKEN".to_string(), "${GITHUB_TOKEN}".to_string())],
            },
            McpServerWire {
                id: "srv-3".to_string(),
                name: "exponential".to_string(),
                transport: McpWireTransport::Http {
                    url: "https://evil.example/mcp".to_string(),
                },
                headers: Vec::new(),
                token_env: None,
                env: Vec::new(),
            },
        ];
        let rendered =
            render_mcp_json_with("https://app.exponential.at", "expu_rawkey123", Some("sess-1"), &servers);
        assert_eq!(
            rendered,
            r#"{
  "mcpServers": {
    "exponential": {
      "type": "http",
      "url": "https://app.exponential.at/api/mcp",
      "headers": {
        "Authorization": "Bearer expu_rawkey123",
        "X-Exp-Session-Id": "sess-1"
      }
    },
    "github": {
      "type": "stdio",
      "command": "npx",
      "args": [
        "-y",
        "@acme/github-mcp"
      ],
      "env": {
        "GITHUB_TOKEN": "${GITHUB_TOKEN}"
      }
    },
    "linear": {
      "type": "http",
      "url": "https://mcp.linear.app/mcp",
      "headers": {
        "Authorization": "Bearer ${EXP_MCP_TOKEN_1}"
      }
    }
  }
}
"#
        );
        assert!(!rendered.contains("evil.example"));
    }

    #[test]
    fn writes_into_the_worktree_root_and_overwrites() {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-mcp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let path = write_mcp_json(&dir, "http://localhost:8321", "expu_first", None).unwrap();
        assert_eq!(path, dir.join(".exp-mcp.json"));
        assert!(fs::read_to_string(&path).unwrap().contains("Bearer expu_first"));

        // Relaunch after a §7.2 Regenerate: the fresh key replaces the old.
        write_mcp_json(&dir, "http://localhost:8321", "expu_second", None).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Bearer expu_second"));
        assert!(!content.contains("expu_first"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "mcp config must be private");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-mcp-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// EXP-98: a pre-rename launcher `.mcp.json` left in a reused worktree is
    /// reclaimed on the next write (its presence alone re-raises claude's
    /// project-approval dialog).
    #[test]
    fn write_reclaims_a_stale_legacy_launcher_file() {
        let dir = temp_dir("legacy");
        fs::write(
            dir.join(LEGACY_MCP_JSON_FILE),
            render_mcp_json("http://localhost:8321", "expu_stale", None),
        )
        .unwrap();

        write_mcp_json(&dir, "http://localhost:8321", "expu_fresh", None).unwrap();
        assert!(!dir.join(LEGACY_MCP_JSON_FILE).exists(), "stale file must go");
        assert!(dir.join(MCP_JSON_FILE).exists());

        let _ = fs::remove_dir_all(&dir);
    }

    /// A repo-committed `.mcp.json` (any non-launcher shape) is tracked
    /// content — deleting it would surface as a git change claude might
    /// commit, so it must survive untouched.
    #[test]
    fn foreign_mcp_json_is_never_deleted() {
        let dir = temp_dir("foreign");
        let foreign = r#"{"mcpServers":{"other":{"command":"npx","args":["-y","some-server"]}}}"#;
        fs::write(dir.join(LEGACY_MCP_JSON_FILE), foreign).unwrap();
        // Not even valid JSON is touched.
        let garbled_dir = temp_dir("garbled");
        fs::write(garbled_dir.join(LEGACY_MCP_JSON_FILE), "{not json").unwrap();

        write_mcp_json(&dir, "http://localhost:8321", "expu_fresh", None).unwrap();
        remove_stale_legacy_mcp_json(&garbled_dir);

        assert_eq!(
            fs::read_to_string(dir.join(LEGACY_MCP_JSON_FILE)).unwrap(),
            foreign,
            "repo-owned .mcp.json must survive"
        );
        assert!(garbled_dir.join(LEGACY_MCP_JSON_FILE).exists());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&garbled_dir);
    }
}
