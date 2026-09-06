//! EXP-746 — claude's private wire: the argv and the stream-json decoder.
//! Owned by lane E2 (the argv is also spike checkpoint #1).
//!
//! The argv is a flag-for-flag port of what the claude SDK spawns, in the
//! SAME order, so a diff against the vendored `sdk.mjs` stays readable.

use std::path::Path;

/// Whether `-p` leads the argv.
///
/// SPIKE CHECKPOINT #1 (S1): the vendored SDK never passes it, but the
/// installed CLI's own `--help` says `--input-format`/`--output-format`/
/// `--include-partial-messages`/`--permission-prompt-tool`/
/// `--replay-user-messages` "only work with --print". The binary's help is the
/// stronger authority and the loud failure (a rejected flag) beats the silent
/// one (no `can_use_tool` ever arrives), so it defaults to `true` until the
/// spike measures which spelling keeps stdin open ACROSS two prompts and still
/// emits `can_use_tool`. Flipping it is this one const plus two argv vectors.
pub const CLAUDE_PRINT_MODE: bool = true;

/// Where the MCP config comes from. `File` is the zero-risk fallback (the
/// same `.exp-mcp.json` the PTY path writes); `Inline` keeps the `expu_` key
/// off disk and is spike checkpoint #2.
#[derive(Clone, Copy, Debug)]
pub enum McpConfig<'a> {
    File(&'a Path),
    Inline(&'a str),
}

/// Everything that varies in a claude ACP spawn.
pub struct ClaudeArgs<'a> {
    pub print_mode: bool,
    pub model: Option<&'a str>,
    /// `ultracode` wins over a plain effort level.
    pub effort: Option<&'a str>,
    /// Control-protocol spelling (`default|acceptEdits|plan|auto|
    /// bypassPermissions`). NOTE for E2: the ARGV spelling of `default` is
    /// `manual` — commander rejects `--permission-mode default`.
    pub permission_mode: Option<&'a str>,
    pub allow_dangerous: bool,
    pub session_id: Option<&'a str>,
    pub resume: Option<&'a str>,
    pub fork_session: bool,
    pub mcp_config: Option<McpConfig<'a>>,
    pub strict_mcp_config: bool,
    /// The `--settings` file: `{}` on this path, kept ONLY because the
    /// reaper selects escaped claude processes by that path appearing in the
    /// process command line (EXP-300).
    pub settings: Option<&'a Path>,
    pub add_dirs: &'a [std::path::PathBuf],
    pub disallowed_tools: &'a [&'a str],
}

/// The full argv after the program name.
// EXP-746 E2: fill
#[allow(unused_variables)]
pub fn claude_argv(args: &ClaudeArgs<'_>) -> Vec<String> {
    todo!("EXP-746 E2: emit the SDK's flag order, `=`-form for --session-id/--resume")
}
