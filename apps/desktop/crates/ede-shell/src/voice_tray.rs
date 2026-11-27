//! #EDE-267: the taskbar's Voice status icon, fed by the holder. Voice and
//! calls live in the login's holder (`exponential-app`): it long-polls the
//! eDE tray's fold (packages/etalk voice.json `edeTray`, `ui::ede_voice`)
//! over the session bus ([`api::bus::Request::Voice`], its own board, so
//! the call's level and timer never re-send the start menu's feeds). The
//! shell shows it as `/voice`: the icon while this machine is the member's
//! voice device (`show`), its popup (voice-tray.express: the call's wave,
//! level, state, lines, Mute, Open chat, End; Call the Assistant while no
//! call runs) and its applet menu (the tray's own rows). Every press goes
//! back to the holder as `voice-act` ([`api::bus::voice_act`]); Open chat
//! is the Assistant's one window (the start menu's `page`).

use std::{sync::mpsc, time::Duration};

use api::bus::Client;
use serde_json::{json, Value};

/// The popup document (taskbar `voice_tray`).
pub const POPUP: &str = "voice-tray";

const RETRY: Duration = Duration::from_secs(2);
const RETRY_UNSUPPORTED: Duration = Duration::from_secs(60);

/// `/voice` while no holder answers: no icon.
pub fn hidden() -> Value {
    json!({"show": false, "menu": [], "lines": [], "call": false, "canCall": false})
}

/// Follows the holder's voice board on a thread of its own; every change
/// is sent on `tx` ([`hidden`] while no holder answers).
pub fn follow(tx: mpsc::Sender<Value>) {
    let spawned = std::thread::Builder::new().name("ede-shell-voice".into()).spawn(move || {
        let path = api::bus::socket_path(&api::default_data_dir());
        let mut last = Value::Null;
        loop {
            let pause = match Client::connect(&path, api::bus::VOICE_TIMEOUT) {
                Ok(mut client) => {
                    let mut after = None;
                    loop {
                        match client.voice(after) {
                            Ok(Some((generation, voice))) => {
                                after = Some(generation);
                                let now = if voice.is_object() { voice } else { hidden() };
                                if now != last {
                                    last = now.clone();
                                    if tx.send(now).is_err() {
                                        return;
                                    }
                                }
                            }
                            Ok(None) => break RETRY_UNSUPPORTED,
                            Err(_) => break RETRY,
                        }
                    }
                }
                Err(_) => RETRY,
            };
            if last != hidden() {
                last = hidden();
                if tx.send(last.clone()).is_err() {
                    return;
                }
            }
            std::thread::sleep(pause);
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("holder voice: {e}");
    }
}

/// A press in the Voice tray (`ede_voice {action, item?}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    /// To the holder: `call`, `mute`, `end`, or `menu` with the row's id.
    Act { action: String, item: Option<String> },
    /// The Assistant's window.
    Chat,
}

pub fn press(args: &Value) -> Option<Press> {
    let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    // The applet menu's pick (window_menu::MENU_PICKS): its row's id as `item`.
    let action = text("action").unwrap_or_else(|| "menu".into());
    match action.as_str() {
        "chat" => Some(Press::Chat),
        "call" | "mute" | "end" => Some(Press::Act { action, item: None }),
        "menu" => Some(Press::Act { action, item: Some(text("item")?) }),
        _ => None,
    }
}

/// Sends `action` to the holder off the main loop.
pub fn act(action: String, item: Option<String>) {
    std::thread::spawn(move || {
        let path = api::bus::socket_path(&api::default_data_dir());
        match api::bus::voice_act(&path, &action, item.as_deref()) {
            Ok(Ok(())) => {}
            Ok(Err(why)) => tracing::warn!(%action, "voice: {why}"),
            Err(e) => tracing::warn!(%action, "voice: {e}"),
        }
    });
}

/// `EDE_SHELL_DEMO_VOICE=idle|call` (the goldens): a sample tray.
pub fn demo() -> Option<Value> {
    let mode = std::env::var("EDE_SHELL_DEMO_VOICE").ok()?;
    let menu = json!([
        {"kind": "item", "value": "endCall", "label": "End call"},
        {"kind": "separator"},
        {"kind": "checkbox", "value": "wakeWord", "label": "Wake word · Hey Assistant", "checked": true},
        {"kind": "checkbox", "value": "mute", "label": "Mute microphone", "checked": false},
        {"kind": "separator"},
        {"kind": "item", "value": "settings", "label": "Voice settings…"},
    ]);
    let call = mode == "call";
    Some(json!({
        "show": true, "dot": "none", "agent": "Assistant", "canCall": !call, "callLabel": "Call Assistant", "call": call,
        "activity": if call { "speaking" } else { "idle" }, "ringing": false, "level": if call { 0.7 } else { 0.0 },
        "state": if call { "02:14" } else { "" }, "muted": false, "mute": {"label": "Mute", "icon": "call-mute"}, "endLabel": "End call",
        "lines": if call { json!([
            {"id": "l1", "side": "out", "text": "Which of my sessions need me?"},
            {"id": "l2", "side": "in", "text": "One: Fix CI on #3590 wants to run the migration on the disposable database."},
            {"id": "l3", "side": "out", "text": "Allow it."},
        ]) } else { json!([]) },
        "menu": menu,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presses_parse() {
        assert_eq!(press(&json!({"action": "chat"})), Some(Press::Chat));
        assert_eq!(press(&json!({"action": "mute"})), Some(Press::Act { action: "mute".into(), item: None }));
        assert_eq!(press(&json!({"action": "menu", "item": "wakeWord"})), Some(Press::Act { action: "menu".into(), item: Some("wakeWord".into()) }));
        assert_eq!(press(&json!({"action": "menu"})), None, "a menu pick names its row");
        assert_eq!(press(&json!({"item": "mute"})), Some(Press::Act { action: "menu".into(), item: Some("mute".into()) }), "the applet menu's pick: the tray's Mute microphone row, not the call's Mute");
        assert_eq!(press(&json!({"action": "nonsense"})), None);
        assert_eq!(press(&json!({})), None);
    }

    #[test]
    fn the_demo_is_the_folds_shape() {
        std::env::set_var("EDE_SHELL_DEMO_VOICE", "call");
        let v = demo().unwrap();
        for key in ["show", "dot", "agent", "canCall", "callLabel", "call", "activity", "ringing", "level", "state", "muted", "mute", "endLabel", "lines", "menu"] {
            assert!(v.get(key).is_some(), "{key}");
        }
        std::env::remove_var("EDE_SHELL_DEMO_VOICE");
    }
}
