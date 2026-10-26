//! FEED-79 (EXP-1259 D10) — the IDE's ONE failed-action surface, the twin of
//! web `lib/action-error.ts` `failedAction` + `@exp/ui` `toastActionError` /
//! `FieldError`, per `feedback-states.json` `errors`:
//!
//! - a failed ACTION (save, delete, start, connect, merge, send, invite) is
//!   an error toast titled `Couldn't <verb> <thing>`, the server's message as
//!   its description, Retry when re-running the same call is safe
//!   ([`failed_action`], [`failed_action_with_retry`]);
//! - INVALID INPUT stays under its field, 13px in the rose foreground
//!   ([`field_error`]): auth forms, the invite email, URL fields, byte caps,
//!   required fields, names already taken.
//!
//! Nothing else draws an error inline. Automatic work (a load on open or on
//! focus, a re-list after a write, polling, prefetch) NEVER toasts: a
//! section whose LOAD failed shows its empty state with a Retry
//! ([`load_failed_state`]); only that Retry PRESS failing again toasts
//! ([`failed_load`] with `attended`).

use gpui::{
    div, px, App, ClickEvent, Div, ElementId, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{ActiveTheme as _, Icon};

use crate::toast::{self, Toast};

/// `errors.titlePattern`.
const TITLE_PATTERN: &str = "Couldn't {verb} {thing}";
/// `errors.inline.size`: the field error's text size (px).
pub(crate) const FIELD_ERROR_PX: f32 = 13.;

/// `Couldn't <verb> <thing>`, the failed action's toast title.
pub(crate) fn action_error_title(verb: &str, thing: &str) -> String {
    TITLE_PATTERN.replace("{verb}", verb).replace("{thing}", thing)
}

/// The toast's description for `err`: the server's message when it reads like
/// a sentence, offline as offline (`ApiError::user_message`), never a JSON or
/// list dump (web `actionErrorDescription`).
pub(crate) fn action_error_description(err: &api::ApiError) -> Option<String> {
    description_of(&err.user_message())
}

/// [`action_error_description`] for a message a site already holds.
pub(crate) fn description_of(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() || trimmed.starts_with('[') || trimmed.starts_with('{') {
        return None;
    }
    Some(trimmed.to_string())
}

/// The failed action's toast itself, for a site that shows it in a window
/// of its own choosing (or adds a different action than Retry).
pub(crate) fn failed_toast(verb: &str, thing: &str, description: Option<String>) -> Toast {
    let toast = Toast::error(action_error_title(verb, thing));
    match description.as_deref().and_then(description_of) {
        Some(description) => toast.description(description),
        None => toast,
    }
}

/// Toast a failed action on the active window: `Couldn't <verb> <thing>` +
/// `description` (the server's message), no Retry.
pub(crate) fn failed_action(verb: &str, thing: &str, description: Option<String>, cx: &mut App) {
    toast::show_in_active_window(failed_toast(verb, thing, description), cx);
}

/// [`failed_action`] for an [`api::ApiError`].
pub(crate) fn failed_action_err(verb: &str, thing: &str, err: &api::ApiError, cx: &mut App) {
    failed_action(verb, thing, action_error_description(err), cx);
}

/// [`failed_action`] with a Retry that re-runs `retry` — only when running
/// the same call again is safe.
pub(crate) fn failed_action_with_retry(
    verb: &str,
    thing: &str,
    description: Option<String>,
    retry: impl Fn(&mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    failed_action_with(verb, thing, description, crate::feedback::RETRY, retry, cx);
}

/// [`failed_action_with_retry`] under another action label (the check
/// gate's `Merge anyway`, `errors.mergeRefusal.force`).
pub(crate) fn failed_action_with(
    verb: &str,
    thing: &str,
    description: Option<String>,
    label: &'static str,
    action: impl Fn(&mut Window, &mut App) + 'static,
    cx: &mut App,
) {
    let toast = failed_toast(verb, thing, description).action(label, action);
    toast::show_in_active_window(toast, cx);
}

/// A LOAD failed: quiet unless `attended` (the person pressed the empty
/// state's Retry), then `Couldn't load <thing>` + the reason. The empty
/// state's own Retry stays the way back, so the toast carries none.
pub(crate) fn failed_load(thing: &str, attended: bool, description: Option<String>, cx: &mut App) {
    if attended {
        failed_action("load", thing, description, cx);
    }
}

/// A section whose LOAD failed: its list empty state (`No <noun>`, the
/// `empty-state.json` copy) with a Retry under it that re-runs the load.
pub(crate) fn load_failed_state(
    id: impl Into<ElementId>,
    icon: Icon,
    title: impl Into<SharedString>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    crate::controls::empty_state_list(icon, title, cx).child(
        crate::controls::text_button(
            id,
            crate::feedback::RETRY,
            crate::controls::TextButtonVariant::Link,
            cx,
        )
        .on_click(on_retry),
    )
}

/// A name the server says is already taken (CONFLICT): INVALID INPUT, so
/// it stays under its field rather than toasting.
pub(crate) fn is_name_conflict(err: &api::ApiError) -> bool {
    matches!(err, api::ApiError::Http { status: 409, .. })
}

/// INVALID INPUT: the line under its field, 13px in the rose foreground.
pub(crate) fn field_error(message: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_size(px(FIELD_ERROR_PX))
        .line_height(px(20.))
        .text_color(cx.theme().danger)
        .child(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/feedback-states.json");

    #[test]
    fn titles_and_numbers_are_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).expect("feedback-states.json parses");
        let errors = &fixture["errors"];
        assert_eq!(errors["titlePattern"].as_str(), Some(TITLE_PATTERN));
        assert_eq!(errors["retry"].as_str(), Some(crate::feedback::RETRY));
        assert_eq!(errors["inline"]["size"].as_f64(), Some(FIELD_ERROR_PX as f64));
        let mut actions = 0;
        for case in errors["cases"].as_array().expect("cases") {
            match case["kind"].as_str() {
                Some("action") => {}
                // Automatic work fails quietly: no toast, no inline line
                // (`trunk_sync`'s `attended` gate, the quiet loads on open).
                Some("background") => {
                    assert_eq!(case["render"], "quiet");
                    continue;
                }
                _ => {
                    assert_eq!(case["render"], "inline");
                    continue;
                }
            }
            actions += 1;
            assert_eq!(case["render"], "toast");
            let title = action_error_title(
                case["verb"].as_str().unwrap(),
                case["thing"].as_str().unwrap(),
            );
            assert_eq!(Some(title.as_str()), case["title"].as_str());
        }
        assert!(actions >= 4);
    }

    #[test]
    fn a_description_is_a_sentence_or_nothing() {
        assert_eq!(description_of("  Name taken  ").as_deref(), Some("Name taken"));
        assert_eq!(description_of("   "), None);
        assert_eq!(description_of("[{\"code\":\"x\"}]"), None);
        assert_eq!(description_of("{\"message\":\"x\"}"), None);
        let offline = api::ApiError::Transport { message: "dns".into(), offline: true };
        assert_eq!(action_error_description(&offline), Some(offline.user_message()));
        let http = api::ApiError::Http { status: 409, message: "A board with this slug exists".into() };
        assert_eq!(
            action_error_description(&http).as_deref(),
            Some("A board with this slug exists")
        );
    }
}
