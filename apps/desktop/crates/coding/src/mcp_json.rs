//! `.mcp.json` for the spawned `claude` (masterplan-v3 §7.1 step 4):
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
//! coding session** (the worktree lives under the user's own repos root).
//! It authenticates the spawned `claude` as the real signed-in user against
//! `/api/mcp`, exposing the `exponential_*` MCP tools.

use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const MCP_JSON_FILE: &str = ".mcp.json";

// Struct (not `serde_json::json!`) so key order is declaration order —
// serde_json's default Map would alphabetize and break byte-stable output.
#[derive(Serialize)]
struct McpFile<'a> {
    #[serde(rename = "mcpServers")]
    mcp_servers: McpServers<'a>,
}

#[derive(Serialize)]
struct McpServers<'a> {
    exponential: McpServer<'a>,
}

#[derive(Serialize)]
struct McpServer<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    url: String,
    headers: Headers<'a>,
}

#[derive(Serialize)]
struct Headers<'a> {
    #[serde(rename = "Authorization")]
    authorization: String,
    #[serde(skip)]
    _marker: std::marker::PhantomData<&'a ()>,
}

/// Render the exact file content (2-space pretty JSON + trailing newline).
/// `base_url` tolerates a trailing slash.
pub fn render_mcp_json(base_url: &str, personal_key: &str) -> String {
    let origin = base_url.trim_end_matches('/');
    let file = McpFile {
        mcp_servers: McpServers {
            exponential: McpServer {
                kind: "http",
                url: format!("{origin}/api/mcp"),
                headers: Headers {
                    authorization: format!("Bearer {personal_key}"),
                    _marker: std::marker::PhantomData,
                },
            },
        },
    };
    let mut rendered = serde_json::to_string_pretty(&file).expect("mcp json serialize");
    rendered.push('\n');
    rendered
}

/// Write `.mcp.json` into the worktree root, `0600` on unix (it carries the
/// raw personal key; `claude` runs as the same user). Overwrites every launch
/// so a regenerated key (§7.2) is picked up on the next session.
pub fn write_mcp_json(
    worktree: &Path,
    base_url: &str,
    personal_key: &str,
) -> io::Result<PathBuf> {
    let path = worktree.join(MCP_JSON_FILE);
    let content = render_mcp_json(base_url, personal_key);
    write_private(&path, &content)?;
    Ok(path)
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
            render_mcp_json("https://app.exponential.at", "expu_rawkey123"),
            EXPECTED
        );
    }

    #[test]
    fn trailing_slash_on_base_url_is_normalized() {
        assert_eq!(
            render_mcp_json("https://app.exponential.at/", "expu_rawkey123"),
            EXPECTED
        );
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

        let path = write_mcp_json(&dir, "http://localhost:8321", "expu_first").unwrap();
        assert_eq!(path, dir.join(".mcp.json"));
        assert!(fs::read_to_string(&path).unwrap().contains("Bearer expu_first"));

        // Relaunch after a §7.2 Regenerate: the fresh key replaces the old.
        write_mcp_json(&dir, "http://localhost:8321", "expu_second").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Bearer expu_second"));
        assert!(!content.contains("expu_first"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "mcp.json must be private");
        }

        let _ = fs::remove_dir_all(&dir);
    }
}
