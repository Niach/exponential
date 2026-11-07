//! `shell_edit` (#VAPP-273): the host-only `shell.edit` capability
//! (capabilities.json: `hostOnly`, so no app can declare it and the Engine's
//! broker refuses it for every app) as the one tool the desktop's OWN agent
//! (the shell's chat, #EDE-58) gets on its hub next to the noun subset. It
//! writes one shell document: genUI's validator first (a refused document
//! writes nothing and answers its issues), then the person's consent in
//! the sealed dialog (`exponential-sealed consent`, Tier B: once per run),
//! then `exponential-ui set <name> -`, which commits it like any edit, so
//! the supervisor's frame watchdog, `undo` and Version history apply.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use emcp::server::{Caller, ToolProvider};
use serde_json::{json, Value};

/// Asks the person: `(title, body) -> allowed`.
pub type Ask = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;
/// Writes a validated document: `(ui dir, name, document) -> Err(why)`.
pub type Write_ = Arc<dyn Fn(&Path, &str, &Value) -> Result<(), String> + Send + Sync>;

pub struct ShellEdit {
    dir: PathBuf,
    /// The runs the person allowed (the caller's session id).
    granted: Mutex<HashSet<String>>,
    ask: Ask,
    write: Write_,
}

/// A sibling binary of this one (the package installs them together), else the bare name on PATH.
fn sibling(name: &str) -> PathBuf {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(name))).filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from(name))
}

/// `exponential-sealed consent`: `allow` exits 0.
fn ask_sealed(title: &str, body: &str) -> bool {
    match Command::new(sibling("exponential-sealed")).args(["consent", "--title", title, "--body", body, "--allow", "Allow", "--deny", "Don't allow"]).stdout(Stdio::null()).status() {
        Ok(status) => status.success(),
        Err(e) => {
            tracing::warn!("exponential-sealed consent: {e}");
            false
        }
    }
}

/// `exponential-ui --dir <dir> set <name> -` with the document on stdin.
fn write_cli(dir: &Path, name: &str, document: &Value) -> Result<(), String> {
    let mut child = Command::new(sibling("exponential-ui")).arg("--dir").arg(dir).args(["set", name, "-"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| format!("exponential-ui: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(document.to_string().as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

impl ShellEdit {
    /// The real one: the sealed consent dialog and the `exponential-ui` CLI.
    pub fn new(dir: PathBuf) -> Self {
        Self::with(dir, Arc::new(ask_sealed), Arc::new(write_cli))
    }

    pub fn with(dir: PathBuf, ask: Ask, write: Write_) -> Self {
        ShellEdit { dir, granted: Mutex::new(HashSet::new()), ask, write }
    }

    /// The line the agent's brief carries.
    pub fn note() -> String {
        "shell_edit changes one of this desktop's shell documents (the sidebar, the taskbar, the start menu, an applet: genUI JSON like apps/ede/ui); the person is asked before the first write of a run, and a refused document comes back with its issues.".into()
    }

    fn answer(applied: bool, issues: Vec<String>) -> Value {
        let structured = json!({"applied": applied, "issues": issues});
        json!({"content": [{"type": "text", "text": structured.to_string()}], "structuredContent": structured})
    }

    fn edit(&self, caller: &Caller, arguments: &Value) -> Value {
        let name = arguments.get("name").and_then(Value::as_str).unwrap_or("");
        if !ede_ui::valid_name(name) {
            return Self::answer(false, vec![format!("name: {name:?} is not a document name (lowercase letters, digits, - and _)")]);
        }
        let Some(document) = arguments.get("document").filter(|d| d.is_object()) else {
            return Self::answer(false, vec!["document: expected the document object {surface, root | components, data}".into()]);
        };
        if let Err(issues) = ede_ui::validate_document(name, document) {
            return Self::answer(false, issues.iter().map(|i| i.to_string()).collect());
        }
        let session = caller.session_id.as_str().to_string();
        let granted = self.granted.lock().unwrap_or_else(|p| p.into_inner()).contains(&session);
        if !granted {
            let body = format!("The Assistant ({}) wants to write the shell document “{name}”. It can be undone from Version history.", caller.label);
            if !(self.ask)("Change your desktop?", &body) {
                return Self::answer(false, vec!["consent: the person did not allow it".into()]);
            }
            self.granted.lock().unwrap_or_else(|p| p.into_inner()).insert(session);
        }
        match (self.write)(&self.dir, name, document) {
            Ok(()) => Self::answer(true, Vec::new()),
            Err(e) => Self::answer(false, vec![e]),
        }
    }
}

impl ToolProvider for ShellEdit {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(vec![json!({
            "name": "shell_edit",
            "description": "Write one shell document of this desktop (`<name>.json` in its ui directory: sidebar, taskbar, start-menu, an applet-*, settings, …) after validation. `applied` = true and it is on screen (committed; Version history undoes it); else `issues` says what was refused and nothing was written. The person is asked once per run.",
            "inputSchema": {"type": "object", "properties": {"name": {"type": "string", "minLength": 1, "maxLength": 64}, "document": {"type": "object"}}, "required": ["name", "document"], "additionalProperties": false}
        })])
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        match name {
            "shell_edit" => self.edit(caller, &arguments),
            _ => json!({"content": [{"type": "text", "text": format!("Unknown tool: {name}")}], "isError": true}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ede_protocol::SessionId;

    fn provider(allow: bool) -> (ShellEdit, Arc<Mutex<Vec<String>>>, Arc<Mutex<Vec<(String, Value)>>>) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let written = Arc::new(Mutex::new(Vec::new()));
        let ask: Ask = {
            let asked = asked.clone();
            Arc::new(move |title: &str, body: &str| {
                asked.lock().unwrap().push(format!("{title} {body}"));
                allow
            })
        };
        let write: Write_ = {
            let written = written.clone();
            Arc::new(move |_dir: &Path, name: &str, doc: &Value| {
                written.lock().unwrap().push((name.to_string(), doc.clone()));
                Ok(())
            })
        };
        (ShellEdit::with(PathBuf::from("/tmp/ui"), ask, write), asked, written)
    }

    fn caller(id: &str) -> Caller {
        Caller { session_id: SessionId::new(id).unwrap(), label: "claude".into() }
    }

    fn document() -> Value {
        json!({"surface": {"layer": "top", "anchor": ["top", "left", "right"], "size": {"height": 40}, "functions": ["ede_launch"]}, "root": {"id": "root", "component": "Box", "style": {"backgroundColor": "$color.popover"}, "children": [{"id": "go", "component": "Button", "props": {"label": "Files"}, "on": {"press": {"functionCall": {"@call": "ede_launch", "args": {"app": "nemo"}}}}}]}})
    }

    #[test]
    fn a_valid_document_asks_once_per_run_then_writes() {
        let (p, asked, written) = provider(true);
        let out = p.call(&caller("run-1"), "shell_edit", json!({"name": "panel", "document": document()}));
        assert_eq!(out["structuredContent"], json!({"applied": true, "issues": []}));
        let out = p.call(&caller("run-1"), "shell_edit", json!({"name": "panel", "document": document()}));
        assert_eq!(out["structuredContent"]["applied"], json!(true));
        assert_eq!(asked.lock().unwrap().len(), 1, "one ask per run");
        assert!(asked.lock().unwrap()[0].contains("“panel”"));
        assert_eq!(written.lock().unwrap().len(), 2);
        p.call(&caller("run-2"), "shell_edit", json!({"name": "panel", "document": document()}));
        assert_eq!(asked.lock().unwrap().len(), 2, "another run asks again");
    }

    #[test]
    fn a_refused_document_or_name_writes_nothing_and_asks_nobody() {
        let (p, asked, written) = provider(true);
        let mut broken = document();
        broken["root"]["children"][0]["on"]["press"]["functionCall"]["@call"] = json!("ede_logout");
        let out = p.call(&caller("run-1"), "shell_edit", json!({"name": "panel", "document": broken}));
        assert_eq!(out["structuredContent"]["applied"], json!(false));
        assert!(out["structuredContent"]["issues"][0].as_str().unwrap().contains("ede_logout"), "{out}");
        let out = p.call(&caller("run-1"), "shell_edit", json!({"name": "Bad Name", "document": document()}));
        assert!(out["structuredContent"]["issues"][0].as_str().unwrap().starts_with("name:"));
        let out = p.call(&caller("run-1"), "shell_edit", json!({"name": "panel"}));
        assert!(out["structuredContent"]["issues"][0].as_str().unwrap().starts_with("document:"));
        assert!(asked.lock().unwrap().is_empty() && written.lock().unwrap().is_empty());
        assert_eq!(p.call(&caller("run-1"), "nope", json!({}))["isError"], json!(true));
    }

    #[test]
    fn a_denied_consent_writes_nothing() {
        let (p, asked, written) = provider(false);
        let out = p.call(&caller("run-1"), "shell_edit", json!({"name": "panel", "document": document()}));
        assert_eq!(out["structuredContent"]["applied"], json!(false));
        assert!(out["structuredContent"]["issues"][0].as_str().unwrap().starts_with("consent:"));
        assert_eq!((asked.lock().unwrap().len(), written.lock().unwrap().len()), (1, 0));
    }

    #[test]
    fn the_tool_is_the_catalogs_host_only_scope() {
        let scope = eengine::broker::scope_def("shell.edit").expect("shell.edit in the catalog");
        assert!(scope.1.host_only);
        assert_eq!(scope.1.tools, vec!["edit".to_string()]);
        let (p, _, _) = provider(true);
        assert_eq!(p.tools().unwrap()[0]["name"], json!("shell_edit"));
    }
}
