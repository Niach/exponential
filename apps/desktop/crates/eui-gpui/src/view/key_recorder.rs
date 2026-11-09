//! #EXP-1629 (interactions-round1.json keyRecorder): the KeyRecorder's
//! capture half. The macro's `field` (a pressable Box) presses like any
//! Box; while its `recording` recipe prop is true the field holds the
//! focus and EVERY key goes to `record` ahead of the window's keybindings
//! (an app-wide keystroke interceptor, like the IDE's recorder), modifier
//! changes and mouse buttons 3-5 over it too; losing the focus fires
//! `blur`. The core keeps the guards and the HOST owns the grammar.

use gpui::{App, Context, FocusHandle, KeyUpEvent, Keystroke, Modifiers, MouseButton, NavigationDirection, Subscription, Window};
use serde_json::{json, Value};

use super::SurfaceView;

/// The field recording now: its id, focus handle and the listeners that
/// live while it records.
pub(crate) struct KeyRecording {
    id: String,
    focus: FocusHandle,
    /// The last key down without its up: a down of the same key again is
    /// the platform's key repeat (the interceptor sees no `is_held`).
    last_down: Option<String>,
    _intercept: Subscription,
    _blur: Subscription,
}

/// The canonical key name of a gpui key (`a`-`z`, `0`-`9`, `f1`-`f24`, the
/// named keys, the US punctuation), and whether gpui reported its shifted
/// character (`!` for shift-1: the physical key is `1`). Anything else
/// (a modifier alone, media keys, a non-US character) is `None`.
pub fn canonical_key(gpui_key: &str) -> Option<(String, bool)> {
    const SHIFTED: [(&str, &str); 21] = [
        ("!", "1"), ("@", "2"), ("#", "3"), ("$", "4"), ("%", "5"), ("^", "6"), ("&", "7"), ("*", "8"), ("(", "9"), (")", "0"),
        ("_", "-"), ("+", "="), ("{", "["), ("}", "]"), ("|", "\\"), (":", ";"), ("\"", "'"), ("<", ","), (">", "."), ("?", "/"), ("~", "`"),
    ];
    const NAMED: [&str; 15] = ["space", "enter", "tab", "escape", "backspace", "delete", "insert", "home", "end", "pageup", "pagedown", "up", "down", "left", "right"];
    const PUNCT: [&str; 11] = ["-", "=", "[", "]", "\\", ";", "'", ",", ".", "/", "`"];
    if let Some((_, k)) = SHIFTED.iter().find(|(s, _)| *s == gpui_key) {
        return Some((k.to_string(), true));
    }
    let shifted_letter = gpui_key.len() == 1 && gpui_key.chars().all(|c| c.is_ascii_uppercase());
    let key = gpui_key.to_ascii_lowercase();
    let key = match key.as_str() {
        "return" => "enter".to_string(),
        "esc" => "escape".to_string(),
        "page_up" => "pageup".to_string(),
        "page_down" => "pagedown".to_string(),
        " " => "space".to_string(),
        _ => key,
    };
    let single = key.len() == 1 && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let function = key.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()).is_some_and(|n| (1..=24).contains(&n)) && !key.starts_with("f0");
    (single || function || NAMED.contains(&key.as_str()) || PUNCT.contains(&key.as_str())).then_some((key, shifted_letter))
}

/// The held flags of a payload (meta = Cmd / Super / Win = gpui `platform`).
fn flags(m: &Modifiers) -> serde_json::Map<String, Value> {
    let mut p = serde_json::Map::new();
    p.insert("ctrl".into(), json!(m.control));
    p.insert("alt".into(), json!(m.alt));
    p.insert("shift".into(), json!(m.shift));
    p.insert("meta".into(), json!(m.platform));
    p
}

/// The fixture's button number of a gpui mouse button (1 left, 2 right,
/// 3 middle, 4 back, 5 forward).
pub(crate) fn button_number(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 3,
        MouseButton::Navigate(NavigationDirection::Back) => 4,
        MouseButton::Navigate(NavigationDirection::Forward) => 5,
    }
}

impl SurfaceView {
    /// `Some(recording)` when node `index` is a KeyRecorder's `field`.
    pub(crate) fn key_recorder_field(&self, index: u32) -> Option<bool> {
        let n = self.cache.node(index)?;
        if n.removed || n.owner_component.as_deref() != Some("KeyRecorder") || n.part.as_deref() != Some("field") {
            return None;
        }
        let q = self.surface.layout_node(index)?.part_query.as_ref().filter(|q| q.component == "KeyRecorder" && q.part == "field")?;
        Some(q.props.get("recording").and_then(Value::as_bool) == Some(true))
    }

    /// After a pass: the field recording now (whoever set `recording`)
    /// takes the focus and the capture; none recording drops it.
    pub(crate) fn sync_key_recorder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.cache.nodes.iter().filter(|n| !n.removed).map(|n| n.index).find(|i| self.key_recorder_field(*i) == Some(true)).and_then(|i| self.cache.node(i)).map(|n| n.id.clone());
        if now.as_deref() == self.recording.as_ref().map(|r| r.id.as_str()) {
            return;
        }
        self.recording = None;
        let Some(id) = now else { return };
        let Some(focus) = self.focus_handles.get(&id).cloned() else { return };
        self.focus_id(&id, window, cx);
        let entity = cx.entity().downgrade();
        let intercept = cx.intercept_keystrokes(move |event, window, cx: &mut App| {
            let Some(entity) = entity.upgrade() else { return };
            let focused = entity.read(cx).recording.as_ref().is_some_and(|r| r.focus.is_focused(window));
            if !focused {
                return;
            }
            // Nothing else sees it: no keybinding, no Tab, no Escape.
            cx.stop_propagation();
            let keystroke = event.keystroke.clone();
            entity.update(cx, |this, cx| this.record_key_down(&keystroke, cx));
        });
        let blur = cx.on_blur(&focus, window, |this, _window, cx| this.key_recorder_blurred(cx));
        self.recording = Some(KeyRecording { id, focus, last_down: None, _intercept: intercept, _blur: blur });
    }

    fn recording_index(&self) -> Option<u32> {
        let id = &self.recording.as_ref()?.id;
        self.cache.index_of(id).filter(|i| self.key_recorder_field(*i) == Some(true))
    }

    fn record(&mut self, payload: serde_json::Map<String, Value>, cx: &mut Context<Self>) {
        if let Some(i) = self.recording_index() {
            self.fire(i, "record", Some(Value::Object(payload)), cx);
        }
    }

    /// A key down while recording (a repeat sends nothing).
    pub(crate) fn record_key_down(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let Some((key, shifted)) = canonical_key(&keystroke.key) else { return };
        let Some(r) = self.recording.as_mut() else { return };
        if r.last_down.as_deref() == Some(key.as_str()) {
            return;
        }
        r.last_down = Some(key.clone());
        let mut m = keystroke.modifiers;
        m.shift |= shifted;
        let mut p = flags(&m);
        p.insert("type".into(), json!("down"));
        p.insert("key".into(), json!(key));
        self.record(p, cx);
    }

    /// A key up on the recording field (`true` = taken).
    pub(crate) fn record_key_up(&mut self, ev: &KeyUpEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(r) = self.recording.as_mut().filter(|r| r.focus.is_focused(window)) else { return false };
        if let Some((key, shifted)) = canonical_key(&ev.keystroke.key) {
            if r.last_down.as_deref() == Some(key.as_str()) {
                r.last_down = None;
            }
            let mut m = ev.keystroke.modifiers;
            m.shift |= shifted;
            let mut p = flags(&m);
            p.insert("type".into(), json!("up"));
            p.insert("key".into(), json!(key));
            self.record(p, cx);
        }
        true
    }

    /// The held modifiers changed while the field records (`true` = taken).
    pub(crate) fn record_modifiers(&mut self, modifiers: &Modifiers, cx: &mut Context<Self>) -> bool {
        let Some(r) = self.recording.as_mut() else { return false };
        // macOS sends no key up for a key pressed with Cmd: a modifier
        // change ends any repeat.
        r.last_down = None;
        let mut p = flags(modifiers);
        p.insert("type".into(), json!("modifiers"));
        self.record(p, cx);
        true
    }

    /// A mouse button down over the field: 3-5 go to the host while it
    /// records (`true` = taken); 1 and 2 send nothing.
    pub(crate) fn record_mouse(&mut self, button: MouseButton, modifiers: &Modifiers, cx: &mut Context<Self>) -> bool {
        let n = button_number(button);
        if self.recording.is_none() || !(3..=5).contains(&n) {
            return false;
        }
        let mut p = flags(modifiers);
        p.insert("type".into(), json!("mouse"));
        p.insert("button".into(), json!(n));
        self.record(p, cx);
        true
    }

    /// The field lost the focus while recording: `blur` (the core writes
    /// recording false and fires `stop`).
    fn key_recorder_blurred(&mut self, cx: &mut Context<Self>) {
        let Some(i) = self.recording_index() else { return };
        self.recording = None;
        self.fire(i, "blur", None, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_key;

    #[test]
    fn canonical_names() {
        for (k, want) in [("k", Some(("k", false))), ("K", Some(("k", true))), ("!", Some(("1", true))), ("f13", Some(("f13", false))), ("f25", None), ("f0", None), ("space", Some(("space", false))), ("return", Some(("enter", false))), ("?", Some(("/", true))), ("`", Some(("`", false))), ("shift", None), ("platform", None), ("é", None)] {
            assert_eq!(canonical_key(k), want.map(|(a, b)| (a.to_string(), b)), "{k}");
        }
    }
}
