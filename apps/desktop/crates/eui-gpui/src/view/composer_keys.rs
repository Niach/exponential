//! #EDE-65 (interactions-round1.json composerKeys): a Composer's
//! `menuKeys`. While it is true, Up, Down, Enter, Tab and Escape with no
//! modifier held fire `key` {key} instead of reaching the field, so a host
//! drives the suggestion menu above it (eDE's `/` menu, the @ / # / :
//! typeahead). gpui dispatches keybindings before key listeners and the
//! textarea binds all five, so the capture is a keystroke interceptor (the
//! KeyRecorder's), installed only while some Composer has menuKeys on. A
//! held modifier (Shift+Enter's newline) and an IME composition pass by.

use gpui::{Context, EntityInputHandler as _, Keystroke, Modifiers, Window};
use serde_json::{json, Value};

use super::input::{FieldInput, FieldKind};
use super::SurfaceView;

/// The `key` payload name of a gpui key with `modifiers` held, or `None`
/// when the field keeps it.
pub fn composer_menu_key(key: &str, modifiers: &Modifiers) -> Option<&'static str> {
    if modifiers.control || modifiers.alt || modifiers.shift || modifiers.platform || modifiers.function {
        return None;
    }
    match key {
        "up" => Some("up"),
        "down" => Some("down"),
        "enter" => Some("enter"),
        "tab" => Some("tab"),
        "escape" => Some("escape"),
        _ => None,
    }
}

impl SurfaceView {
    fn menu_keys_on(&self, id: &str) -> bool {
        self.cache.index_of(id).and_then(|i| self.cache.node(i)).is_some_and(|n| !n.removed && n.component == "Composer" && n.props.get("menuKeys").and_then(Value::as_bool) == Some(true))
    }

    /// After a pass: the interceptor lives while a Composer has menuKeys on.
    pub(crate) fn sync_composer_keys(&mut self, cx: &mut Context<Self>) {
        let on = self.fields.iter().any(|(id, f)| f.kind == FieldKind::Composer && self.menu_keys_on(id));
        if on == self.composer_keys.is_some() {
            return;
        }
        self.composer_keys = on.then(|| {
            let entity = cx.entity().downgrade();
            cx.intercept_keystrokes(move |event, window, cx| {
                let Some(entity) = entity.upgrade() else { return };
                let keystroke = event.keystroke.clone();
                if entity.update(cx, |this, cx| this.composer_keystroke(&keystroke, window, cx)) {
                    cx.stop_propagation();
                }
            })
        });
    }

    /// A keystroke anywhere in the window: taken (`true`) when it is a menu
    /// key on the focused Composer with menuKeys on and no IME composition.
    fn composer_keystroke(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.fields.iter().find(|(_, f)| f.kind == FieldKind::Composer && f.focus_handle(cx).is_focused(window)).map(|(id, _)| id.clone()) else { return false };
        let composing = match self.fields.get(&id).map(|f| &f.input) {
            Some(FieldInput::Multi(s)) => s.update(cx, |s, cx| s.marked_text_range(window, cx)).is_some(),
            Some(FieldInput::Line(s)) => s.update(cx, |s, cx| s.marked_text_range(window, cx)).is_some(),
            None => false,
        };
        self.composer_key(&id, &keystroke.key, &keystroke.modifiers, composing, window, cx)
    }

    /// A key down in Composer `id` (`composing` = an IME composition is in
    /// progress): with menuKeys on, a menu key flushes the draft and fires
    /// `key` {key}; `true` = taken. Public for automation and tests (it
    /// never focuses the field).
    pub fn composer_key(&mut self, id: &str, key: &str, modifiers: &Modifiers, composing: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if composing || !self.menu_keys_on(id) {
            return false;
        }
        let Some(name) = composer_menu_key(key, modifiers) else { return false };
        self.flush_field(id, None, window, cx);
        if let Some(i) = self.cache.index_of(id) {
            self.fire(i, "key", Some(json!({"key": name})), cx);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::composer_menu_key;
    use gpui::Modifiers;

    #[test]
    fn menu_keys_only_without_modifiers() {
        let none = Modifiers::default();
        for k in ["up", "down", "enter", "tab", "escape"] {
            assert_eq!(composer_menu_key(k, &none), Some(k));
        }
        for k in ["left", "right", "home", "a", "backspace", "space"] {
            assert_eq!(composer_menu_key(k, &none), None, "{k}");
        }
        for m in [Modifiers::shift(), Modifiers::control(), Modifiers::alt(), Modifiers::command()] {
            assert_eq!(composer_menu_key("enter", &m), None);
            assert_eq!(composer_menu_key("tab", &m), None);
        }
    }
}
