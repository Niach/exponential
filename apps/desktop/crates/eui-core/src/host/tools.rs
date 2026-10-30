//! VAPP-161: a package's TOOLS (docs/package-format.md "Tools"): the MCP
//! servers and scripts its functions call (`tools.<id>.<name>` for an MCP
//! tool, `tools.<id>` for a script), each behind the consent card the host
//! MUST show before the first call. Mirrors
//! `packages/exponential-ui/src/host/tools.ts` line by line;
//! `fixtures/host-router.json` (validation) and `fixtures/host-package.json`
//! `consent` + `toolCalls` replay it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::package::PackageIssue;
use super::signature::canonical_json;
use super::{is_js_space, js_trim};

/// `^[a-z][a-z0-9_-]{0,31}$`: a tool's id (`package.tools.idPattern`).
pub const TOOL_ID_PATTERN: &str = "^[a-z][a-z0-9_-]{0,31}$";
pub const TOOL_KINDS: &[&str] = &["mcp", "script"];
pub const TOOL_TRANSPORTS: &[&str] = &["http", "stdio", "script"];
/// The host function names a tool call starts with.
pub const TOOL_FUNCTION_PREFIX: &str = "tools.";

fn is_id_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
}

/// [`TOOL_ID_PATTERN`].
pub fn is_tool_id(id: &str) -> bool {
    id.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && id.len() <= 32 && id.chars().all(is_id_char)
}

/// `^[A-Za-z_][A-Za-z0-9_]*$`: an env variable name.
fn is_env_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_string_list(v: &Value) -> bool {
    v.as_array().is_some_and(|l| l.iter().all(Value::is_string))
}

/// `new URL(s)` parses with an `http:` / `https:` protocol and a host.
fn is_http_url(s: &str) -> bool {
    url::Url::parse(s).is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some_and(|h| !h.is_empty()))
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

/// Every problem with a package's `tools`, ids in sorted order.
pub fn validate_tools(tools: &Value) -> Vec<PackageIssue> {
    let Some(tools) = tools.as_object() else { return vec![issue("/tools", "tools is an object of tools")] };
    let mut issues = Vec::new();
    let mut ids: Vec<&String> = tools.keys().collect();
    ids.sort_by(|a, b| super::utf16_cmp(a, b));
    for id in ids {
        let at = format!("/tools/{id}");
        if !is_tool_id(id) {
            issues.push(issue(&at, "a tool id is lowercase letters, digits, - and _"));
        }
        let Some(tool) = tools[id].as_object() else {
            issues.push(issue(&at, "a tool is {kind, title, url | command}"));
            continue;
        };
        let kind = tool.get("kind").and_then(Value::as_str);
        if !kind.is_some_and(|k| TOOL_KINDS.contains(&k)) {
            issues.push(issue(format!("{at}/kind"), "kind is mcp or script"));
        }
        if tool.get("title").and_then(Value::as_str).is_none_or(|t| js_trim(t).is_empty()) {
            issues.push(issue(format!("{at}/title"), "title is a required string"));
        }
        if tool.get("description").is_some_and(|d| !d.is_string()) {
            issues.push(issue(format!("{at}/description"), "description is a string"));
        }
        // `!== undefined`: a present `null` counts as set.
        let url = tool.get("url");
        let command = tool.get("command");
        let (has_url, has_command) = (url.is_some(), command.is_some());
        if kind == Some("mcp") && has_url == has_command {
            issues.push(issue(&at, "an mcp tool has a url or a command"));
        }
        if kind == Some("script") && has_url {
            issues.push(issue(format!("{at}/url"), "a script runs a command, not a url"));
        }
        if kind == Some("script") && !has_command {
            issues.push(issue(format!("{at}/command"), "command is a non-empty string"));
        }
        if url.is_some_and(|u| !u.as_str().is_some_and(is_http_url)) {
            issues.push(issue(format!("{at}/url"), "url is an absolute http(s) url"));
        }
        if command.is_some_and(|c| c.as_str().is_none_or(|c| js_trim(c).is_empty())) {
            issues.push(issue(format!("{at}/command"), "command is a non-empty string"));
        }
        if tool.get("args").is_some_and(|a| !is_string_list(a) || has_url) {
            issues.push(issue(format!("{at}/args"), "args is a list of strings beside a command"));
        }
        if tool.get("env").is_some_and(|e| !is_string_list(e) || e.as_array().into_iter().flatten().any(|n| !n.as_str().is_some_and(is_env_name)) || has_url) {
            issues.push(issue(format!("{at}/env"), "env is a list of variable names beside a command"));
        }
    }
    issues
}

/// One row of the consent card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolConsentRow {
    pub id: String,
    /// `mcp` | `script`.
    pub kind: String,
    /// `http` | `stdio` | `script`.
    pub transport: String,
    pub title: String,
    pub description: Option<String>,
    /// The url, or the command line exactly as it runs.
    pub target: String,
    pub env: Vec<String>,
}

/// An arg as the consent card shows it: as is, or JSON-quoted when it is
/// empty or holds a space, a quote or a backslash.
fn shell_arg(arg: &str) -> String {
    if arg.is_empty() || arg.chars().any(|c| is_js_space(c) || matches!(c, '"' | '\'' | '\\')) {
        canonical_json(&Value::String(arg.to_string()))
    } else {
        arg.to_string()
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

/// The consent card's rows for a (valid) package's tools, by id.
pub fn tool_consent(pkg: &Value) -> Vec<ToolConsentRow> {
    let Some(tools) = pkg.get("tools").and_then(Value::as_object) else { return Vec::new() };
    let mut ids: Vec<&String> = tools.keys().collect();
    ids.sort_by(|a, b| super::utf16_cmp(a, b));
    ids.into_iter()
        .map(|id| {
            let tool = &tools[id];
            let kind = tool.get("kind").and_then(Value::as_str).unwrap_or("").to_string();
            let transport = if kind == "script" {
                "script"
            } else if tool.get("url").is_some() {
                "http"
            } else {
                "stdio"
            };
            // `tool.url ?? command line`: a null url is the command line.
            let target = match tool.get("url").filter(|u| !u.is_null()) {
                Some(url) => url.as_str().unwrap_or("").to_string(),
                None => std::iter::once(tool.get("command").and_then(Value::as_str).unwrap_or("").to_string()).chain(strings(tool.get("args"))).map(|a| shell_arg(&a)).collect::<Vec<_>>().join(" "),
            };
            ToolConsentRow {
                id: id.clone(),
                kind,
                transport: transport.to_string(),
                title: tool.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
                description: tool.get("description").and_then(Value::as_str).map(str::to_string),
                target,
                env: strings(tool.get("env")),
            }
        })
        .collect()
}

/// The key a remembered grant is stored under: ANY change to the tool's
/// definition is a new key (the card asks again).
pub fn tool_consent_key(package_id: &str, tool_id: &str, tool: &Value) -> String {
    format!("{package_id}/{tool_id}:{}", canonical_json(tool))
}

/// The tool a host function name calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub tool_id: String,
    /// The MCP server's tool; `None` = a script.
    pub tool: Option<String>,
}

/// `tools.<id>` (a script) or `tools.<id>.<name>` (an MCP server's tool);
/// `None` = not a tool call (`^tools\.([a-z][a-z0-9_-]{0,31})(?:\.(.+))?$`,
/// `.` matching no line terminator).
pub fn tool_call(name: &str) -> Option<ToolCall> {
    let rest = name.strip_prefix(TOOL_FUNCTION_PREFIX)?;
    let end = rest.find(|c: char| !is_id_char(c)).unwrap_or(rest.len());
    let (id, tail) = rest.split_at(end);
    if !is_tool_id(id) {
        return None;
    }
    let tool = match tail {
        "" => None,
        t => {
            let t = t.strip_prefix('.').filter(|t| !t.is_empty() && !t.chars().any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')))?;
            Some(t.to_string())
        }
    };
    Some(ToolCall { tool_id: id.to_string(), tool })
}
