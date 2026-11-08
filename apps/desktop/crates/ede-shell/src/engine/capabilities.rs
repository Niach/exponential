//! eDE's own capabilities (#VAPP-273, capabilities.json): `windows`
//! (`windows.read`: `list`), `focus` (`focus.window`: `window`) and `launch`
//! (`launch.app`: `app`), each an `ask` scope an app declares in its
//! manifest. They reach the compositor over ede-protocol with ONE grant
//! minted for the apps (`ui`: a focus needs it, the list needs none), never
//! the shell's root token, and launch runs the launcher's desktop entries.
//! The Engine's bridge validates the arguments and checks the answers
//! (`nativeOutputSchema`); these providers answer the native shape.

use std::sync::{Arc, Mutex};

use ede_protocol::{Capability as EdeCapability, Client, SessionId, Token, WindowId, WindowInfo};
use eengine::capability::Capability;
use eengine::providers::Failure;
use eengine::tools::{mcp_error, mcp_result, tool_def};
use serde_json::{json, Value};
use uicore_data::apps;

/// How a capability reaches the compositor: a fresh connection per call
/// (the calls run on worker threads, a `Client` is one blocking socket).
pub trait Connect: Send + Sync {
    fn client(&self) -> Result<Client, String>;
}

/// The compositor of `$WAYLAND_DISPLAY` with a grant minted once for the
/// apps (`ui`), minted from the root token on the first call; a root
/// connection that cannot mint is used as it is.
#[derive(Default)]
pub struct Minted {
    token: Mutex<Option<Token>>,
}

impl Connect for Minted {
    fn client(&self) -> Result<Client, String> {
        if let Some(token) = self.token.lock().unwrap_or_else(|p| p.into_inner()).clone() {
            return Client::connect_with_token(token).map_err(|e| e.to_string());
        }
        let mut root = Client::connect_env().map_err(|e| e.to_string())?;
        if !root.hello.capabilities.contains(&EdeCapability::System) {
            return Ok(root);
        }
        let session = SessionId::new("exponential-apps").map_err(|e| e.to_string())?;
        let grant = root.mint(session, vec![EdeCapability::Ui], None).map_err(|e| e.to_string())?;
        *self.token.lock().unwrap_or_else(|p| p.into_inner()) = Some(grant.token.clone());
        Client::connect_with_token(grant.token).map_err(|e| e.to_string())
    }
}

fn unreachable(e: String) -> Failure {
    Failure::new("unreachable", e)
}

fn tools_of(capability: &str) -> Vec<Value> {
    eengine::tools::TOOLS.iter().filter(|t| t.name.starts_with(&format!("{capability}_"))).map(|t| t.tool_object(t.tool())).collect()
}

/// A compositor window as `windows_list` answers it.
pub fn window_json(w: &WindowInfo) -> Value {
    json!({
        "id": w.id.to_string(),
        "title": w.title.clone().unwrap_or_default(),
        "app": w.app_id.clone().unwrap_or_default(),
        "workspace": w.workspace,
        "focused": w.focused,
        "minimized": w.minimized,
    })
}

/// `windows`: the open windows, protected ones left out.
pub struct Windows(pub Arc<dyn Connect>);

impl Capability for Windows {
    fn name(&self) -> &str {
        "windows"
    }

    fn tools(&self) -> Vec<Value> {
        tools_of("windows")
    }

    fn call(&self, tool: &str, _arguments: &Value) -> Result<Value, Failure> {
        if tool != "list" {
            return Ok(mcp_error("unavailable"));
        }
        let mut client = self.0.client().map_err(unreachable)?;
        let windows = client.windows().map_err(|e| unreachable(e.to_string()))?;
        Ok(mcp_result(json!({"windows": windows.iter().filter(|w| !w.protected).map(window_json).collect::<Vec<_>>()})))
    }
}

/// `focus`: bring a window to the front (`focused` = false when it is gone).
pub struct Focus(pub Arc<dyn Connect>);

impl Capability for Focus {
    fn name(&self) -> &str {
        "focus"
    }

    fn tools(&self) -> Vec<Value> {
        tools_of("focus")
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        if tool != "window" {
            return Ok(mcp_error("unavailable"));
        }
        let Some(id) = arguments.get("window").and_then(Value::as_str).and_then(|s| WindowId::new(s).ok()) else {
            return Ok(mcp_result(json!({"focused": false})));
        };
        let mut client = self.0.client().map_err(unreachable)?;
        let focused = match client.focus(id) {
            Ok(()) => true,
            Err(ede_protocol::Error::Failed(_)) => false,
            Err(e) => return Err(unreachable(e.to_string())),
        };
        Ok(mcp_result(json!({"focused": focused})))
    }
}

/// `launch`: start an installed application by its desktop entry id.
pub struct Launch {
    pub apps: Arc<Vec<apps::App>>,
    /// Runs the entry's command (`system::launch`; a test records it).
    pub run: Arc<dyn Fn(&str) + Send + Sync>,
}

impl Capability for Launch {
    fn name(&self) -> &str {
        "launch"
    }

    fn tools(&self) -> Vec<Value> {
        tools_of("launch")
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        if tool != "app" {
            return Ok(mcp_error("unavailable"));
        }
        let wanted = arguments.get("app").and_then(Value::as_str).unwrap_or("").trim_end_matches(".desktop");
        let found = self.apps.iter().find(|a| a.id.trim_end_matches(".desktop").eq_ignore_ascii_case(wanted)).filter(|a| !a.exec.is_empty());
        match found {
            Some(app) => {
                (self.run)(&app.exec);
                Ok(mcp_result(json!({"launched": true})))
            }
            None => Ok(mcp_result(json!({"launched": false}))),
        }
    }
}

/// eDE's three capabilities over `connect`, launching from `apps`.
pub fn ede_capabilities(connect: Arc<dyn Connect>, apps: Arc<Vec<apps::App>>) -> Vec<Arc<dyn Capability>> {
    vec![
        Arc::new(Windows(connect.clone())),
        Arc::new(Focus(connect)),
        Arc::new(Launch { apps, run: Arc::new(|command| crate::system::launch(command)) }),
    ]
}

/// The v1 tool objects these providers answer exist in the catalog.
pub fn catalog_tools() -> Vec<&'static eengine::tools::ToolDef> {
    [("windows", "list"), ("focus", "window"), ("launch", "app")].iter().filter_map(|(c, t)| tool_def(c, t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    use ede_protocol::{read_frame, write_frame, Call, Hello, Message, Request};

    /// A compositor over a socket pair: answers hello, the window list and a focus.
    struct Fake {
        windows: Vec<WindowInfo>,
        focused: Arc<Mutex<Vec<String>>>,
    }

    impl Connect for Fake {
        fn client(&self) -> Result<Client, String> {
            let (mine, theirs) = UnixStream::pair().map_err(|e| e.to_string())?;
            let windows = self.windows.clone();
            let focused = self.focused.clone();
            std::thread::spawn(move || {
                let mut theirs = theirs;
                while let Ok(frame) = read_frame(&mut theirs) {
                    let call: Call = serde_json::from_slice(&frame).unwrap();
                    let result = match call.request {
                        Request::Hello { .. } => serde_json::to_value(Hello { version: 1, capabilities: vec![EdeCapability::Ui], session_id: SessionId::new("apps").unwrap() }).unwrap(),
                        Request::ListWindows => serde_json::to_value(&windows).unwrap(),
                        Request::FocusWindow { window } => {
                            if windows.iter().any(|w| w.id == window) {
                                focused.lock().unwrap().push(window.to_string());
                                Value::Null
                            } else {
                                let error = Message::Error { id: call.id, error: ede_protocol::Failure::new(ede_protocol::ErrorCode::NotFound, "no such window") };
                                write_frame(&mut theirs, &serde_json::to_vec(&error).unwrap()).unwrap();
                                continue;
                            }
                        }
                        _ => Value::Null,
                    };
                    let reply = Message::Reply { id: call.id, result };
                    write_frame(&mut theirs, &serde_json::to_vec(&reply).unwrap()).unwrap();
                }
            });
            Client::over(mine, Token::new("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef").unwrap()).map_err(|e| e.to_string())
        }
    }

    fn info(id: &str, app: &str, title: Option<&str>, protected: bool) -> WindowInfo {
        WindowInfo { id: WindowId::new(id).unwrap(), title: title.map(String::from), app_id: Some(app.into()), x: 0, y: 0, width: 10, height: 10, focused: id == "w1", fullscreen: false, protected, x11: false, workspace: 0, minimized: false }
    }

    fn fake() -> (Arc<Fake>, Arc<Mutex<Vec<String>>>) {
        let focused = Arc::new(Mutex::new(Vec::new()));
        (Arc::new(Fake { windows: vec![info("w1", "firefox", Some("Firefox"), false), info("w2", "org.keepassxc.KeePassXC", None, true)], focused: focused.clone() }), focused)
    }

    #[test]
    fn the_catalog_defines_the_three_tools() {
        assert_eq!(catalog_tools().len(), 3);
        assert_eq!(Windows(fake().0).tools().len(), 1);
    }

    #[test]
    fn windows_list_leaves_protected_windows_out() {
        let (connect, _) = fake();
        let out = Windows(connect).call("list", &json!({})).unwrap();
        let def = tool_def("windows", "list").unwrap();
        let structured = out["structuredContent"].clone();
        assert!(eengine::tools::validate_schema(def.native_output_schema.as_ref().unwrap(), &structured).is_ok());
        assert_eq!(structured["windows"].as_array().unwrap().len(), 1);
        assert_eq!(structured["windows"][0], json!({"id": "w1", "title": "Firefox", "app": "firefox", "workspace": 0, "focused": true, "minimized": false}));
    }

    #[test]
    fn focus_names_the_window_and_a_gone_one_is_not_an_error() {
        let (connect, focused) = fake();
        let focus = Focus(connect);
        assert_eq!(focus.call("window", &json!({"window": "w1"})).unwrap()["structuredContent"], json!({"focused": true}));
        assert_eq!(focused.lock().unwrap().as_slice(), ["w1"]);
        assert_eq!(focus.call("window", &json!({"window": "w9"})).unwrap()["structuredContent"], json!({"focused": false}));
        assert_eq!(focus.call("window", &json!({})).unwrap()["structuredContent"], json!({"focused": false}));
        assert_eq!(focus.call("nope", &json!({})).unwrap()["isError"], true);
    }

    #[test]
    fn launch_runs_an_installed_entry_only() {
        let ran = Arc::new(Mutex::new(Vec::new()));
        let run = {
            let ran = ran.clone();
            Arc::new(move |c: &str| ran.lock().unwrap().push(c.to_string())) as Arc<dyn Fn(&str) + Send + Sync>
        };
        let apps = Arc::new(vec![apps::App { id: "org.gnome.Calculator.desktop".into(), name: "Calculator".into(), comment: String::new(), exec: "gnome-calculator".into(), exec_url: None, icon: None, keywords: String::new(), categories: vec![] }]);
        let launch = Launch { apps, run };
        assert_eq!(launch.call("app", &json!({"app": "org.gnome.Calculator"})).unwrap()["structuredContent"], json!({"launched": true}));
        assert_eq!(launch.call("app", &json!({"app": "org.gnome.Calculator.desktop"})).unwrap()["structuredContent"], json!({"launched": true}));
        assert_eq!(launch.call("app", &json!({"app": "nope"})).unwrap()["structuredContent"], json!({"launched": false}));
        assert_eq!(ran.lock().unwrap().len(), 2);
    }
}
