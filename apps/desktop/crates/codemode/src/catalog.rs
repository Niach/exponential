//! EXP-1236: what a run's scripts can call. Loaded lazily on the first
//! `exec`/`describe` of a run (one `initialize` + `tools/list` per upstream,
//! in parallel), cached for the run; a server that did not answer is listed
//! as unreachable and tried again next time.

use std::time::Duration;

use serde_json::{json, Value};

use crate::client::McpClient;
use crate::Upstream;

/// One callable tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSpec {
    /// The config key (`exponential`, `computer`, a team server's key).
    pub server: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub tools: Vec<ToolSpec>,
    /// Servers that did not answer `tools/list`: (name, why).
    pub unreachable: Vec<(String, String)>,
    /// Stdio team servers: the agent calls them directly, scripts cannot.
    pub direct_only: Vec<String>,
}

/// How long one upstream may take to list its tools.
const LIST_TIMEOUT: Duration = Duration::from_secs(20);
/// A description's first line, cut for the `ALL_TOOLS` listing.
const SUMMARY_MAX: usize = 110;

impl Catalog {
    /// Ask every upstream for its tools, in parallel.
    pub fn load(upstreams: &[Upstream], direct_only: Vec<String>, http: &reqwest::blocking::Client) -> Catalog {
        let handles: Vec<_> = upstreams
            .iter()
            .cloned()
            .map(|upstream| {
                let http = http.clone();
                std::thread::spawn(move || {
                    let client = McpClient::new(http, upstream.clone());
                    let listed = client.initialize(LIST_TIMEOUT).and_then(|()| client.list_tools(LIST_TIMEOUT));
                    (upstream.name, listed)
                })
            })
            .collect();
        let mut catalog = Catalog { direct_only, ..Catalog::default() };
        for handle in handles {
            let Ok((name, listed)) = handle.join() else {
                continue;
            };
            match listed {
                Ok(tools) => {
                    for tool in tools {
                        let tool_name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
                        if tool_name.is_empty() {
                            continue;
                        }
                        catalog.tools.push(ToolSpec {
                            server: name.clone(),
                            name: sanitize(tool_name),
                            description: tool
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            input_schema: tool.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object" })),
                        });
                    }
                }
                Err(err) => catalog.unreachable.push((name, err.to_string())),
            }
        }
        catalog.tools.sort_by(|a, b| (&a.server, &a.name).cmp(&(&b.server, &b.name)));
        catalog
    }

    /// The `tools.mcp__<server>__<tool>` spelling.
    pub fn js_name(server: &str, tool: &str) -> String {
        format!("mcp__{server}__{tool}")
    }

    /// The listing a script reads as `ALL_TOOLS`: one line per tool with a
    /// one-line summary, then what is unreachable and what is direct-only.
    pub fn all_tools_text(&self) -> String {
        let mut out = String::new();
        for tool in &self.tools {
            out.push_str(&Self::js_name(&tool.server, &tool.name));
            let summary = summary(&tool.description);
            if !summary.is_empty() {
                out.push_str(": ");
                out.push_str(&summary);
            }
            out.push('\n');
        }
        for (name, why) in &self.unreachable {
            out.push_str(&format!("unreachable: {name} ({why})\n"));
        }
        for name in &self.direct_only {
            out.push_str(&format!("direct-call only (stdio): {name}\n"));
        }
        out
    }

    /// A tool by any spelling: `mcp__s__t`, `s.t`, or a bare `t` when only
    /// one server has it.
    pub fn resolve_name(&self, text: &str) -> Option<&ToolSpec> {
        let text = text.trim();
        if let Some(rest) = text.strip_prefix("mcp__") {
            return self.tools.iter().find(|tool| {
                rest.strip_prefix(tool.server.as_str())
                    .and_then(|after| after.strip_prefix("__"))
                    .is_some_and(|name| name == tool.name)
            });
        }
        if let Some((server, name)) = text.split_once('.') {
            return self.tools.iter().find(|tool| tool.server == server && tool.name == name);
        }
        let mut matches = self.tools.iter().filter(|tool| tool.name == text);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    /// `describe`'s answer: schemas for `names`, or every tool's summary
    /// when `names` is empty.
    pub fn describe(&self, names: &[String]) -> Value {
        if names.is_empty() {
            return json!({
                "tools": self.tools.iter().map(|tool| json!({
                    "name": Self::js_name(&tool.server, &tool.name),
                    "summary": summary(&tool.description),
                })).collect::<Vec<_>>(),
                "unreachable": self.unreachable.iter().map(|(name, why)| json!({ "server": name, "reason": why })).collect::<Vec<_>>(),
                "directOnly": self.direct_only,
            });
        }
        let mut tools = Vec::new();
        let mut unknown = Vec::new();
        for name in names {
            match self.resolve_name(name) {
                Some(tool) => tools.push(json!({
                    "name": Self::js_name(&tool.server, &tool.name),
                    "server": tool.server,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                })),
                None => unknown.push(name.clone()),
            }
        }
        json!({
            "tools": tools,
            "unknown": unknown,
            "unreachable": self.unreachable.iter().map(|(name, why)| json!({ "server": name, "reason": why })).collect::<Vec<_>>(),
            "directOnly": self.direct_only,
        })
    }
}

/// A tool name as a JS identifier piece: ASCII alphanumerics and `_`.
fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// The first sentence or line of a description, cut at [`SUMMARY_MAX`].
fn summary(description: &str) -> String {
    let first_line = description.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
    let sentence_end = first_line
        .char_indices()
        .find(|(i, c)| *c == '.' && (i + 1 == first_line.len() || first_line[i + 1..].starts_with([' ', '\n'])))
        .map(|(i, _)| i + 1)
        .unwrap_or(first_line.len());
    let sentence = &first_line[..sentence_end];
    if sentence.chars().count() <= SUMMARY_MAX {
        return sentence.to_string();
    }
    let cut: String = sentence.chars().take(SUMMARY_MAX - 1).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog {
            tools: vec![
                ToolSpec {
                    server: "computer".into(),
                    name: "click".into(),
                    description: "Click an element. Second sentence that is long.".into(),
                    input_schema: json!({ "type": "object", "properties": { "x": { "type": "number" } } }),
                },
                ToolSpec {
                    server: "exponential".into(),
                    name: "issues_list".into(),
                    description: "x".repeat(200),
                    input_schema: json!({ "type": "object" }),
                },
                ToolSpec {
                    server: "linear".into(),
                    name: "click".into(),
                    description: String::new(),
                    input_schema: json!({ "type": "object" }),
                },
            ],
            unreachable: vec![("sentry".into(), "401".into())],
            direct_only: vec!["playwright".into()],
        }
    }

    #[test]
    fn all_tools_lists_one_line_per_tool_with_summaries_and_the_other_lists() {
        let text = catalog().all_tools_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "mcp__computer__click: Click an element.");
        assert!(lines[1].starts_with("mcp__exponential__issues_list: xxx"));
        assert!(lines[1].ends_with('…'));
        assert!(lines[1].chars().count() < "mcp__exponential__issues_list: ".len() + SUMMARY_MAX + 1);
        assert_eq!(lines[2], "mcp__linear__click");
        assert_eq!(lines[3], "unreachable: sentry (401)");
        assert_eq!(lines[4], "direct-call only (stdio): playwright");
    }

    #[test]
    fn names_resolve_in_three_spellings_and_bare_names_must_be_unambiguous() {
        let catalog = catalog();
        assert_eq!(catalog.resolve_name("mcp__computer__click").unwrap().server, "computer");
        assert_eq!(catalog.resolve_name("linear.click").unwrap().server, "linear");
        assert_eq!(catalog.resolve_name("issues_list").unwrap().server, "exponential");
        assert!(catalog.resolve_name("click").is_none(), "two servers have click");
        assert!(catalog.resolve_name("mcp__computer__nope").is_none());
    }

    #[test]
    fn describe_answers_schemas_unknowns_and_the_summary_list() {
        let catalog = catalog();
        let answer = catalog.describe(&["computer.click".into(), "ghost".into()]);
        assert_eq!(answer["tools"][0]["name"], json!("mcp__computer__click"));
        assert_eq!(answer["tools"][0]["inputSchema"]["properties"]["x"]["type"], json!("number"));
        assert_eq!(answer["unknown"], json!(["ghost"]));
        assert_eq!(answer["directOnly"], json!(["playwright"]));
        let all = catalog.describe(&[]);
        assert_eq!(all["tools"].as_array().unwrap().len(), 3);
        assert_eq!(all["tools"][0]["summary"], json!("Click an element."));
        assert_eq!(all["unreachable"][0]["server"], json!("sentry"));
    }

    #[test]
    fn tool_names_are_sanitized_to_identifier_pieces() {
        assert_eq!(sanitize("browser-click"), "browser_click");
        assert_eq!(sanitize("ok_name9"), "ok_name9");
    }
}
