//! Settings → Sessions (EXP-886): how long THIS machine keeps the session
//! history of finished runs — the device-only transcripts and resume
//! records. Desktop-only, in the "This device" group, never gated.
//!
//! The value is `sessionRetentionDays` in the shared `settings.json`
//! ([`coding::session_retention`]); the CLI daemon on the same data dir
//! honours it at its own boot. Unlimited (the default) keeps everything.
//! A change writes on pick (no Save button) and, when it names a window,
//! prunes right away on the background executor instead of waiting for the
//! next start.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{div, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window};
use serde_json::{json, Value};

use coding::session_retention::{self, CHOICES};
use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use super::doc::{mount, path, IdeToasts, Ink};

/// The menu label for a stored window. A value the pane does not offer (a
/// hand-edited file) still reads truthfully.
fn choice_label(days: Option<u32>) -> SharedString {
    CHOICES
        .iter()
        .find(|(choice, _)| *choice == days)
        .map(|(_, label)| SharedString::from(*label))
        .unwrap_or_else(|| match days {
            Some(days) => format!("{days} days").into(),
            None => "Unlimited".into(),
        })
}

pub struct SessionsPane {
    /// The data dir `days` was read from — re-read when the signed-in
    /// account (and with it the data dir) changes.
    loaded_for: Option<PathBuf>,
    days: Option<u32>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// A menu item's value for a window (`"unlimited"` or the day count).
fn choice_value(days: Option<u32>) -> String {
    days.map_or_else(|| "unlimited".to_string(), |days| days.to_string())
}

impl SessionsPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // A pick leaves the binding for the pane: it writes the file and
        // prunes off the UI thread, reverting and toasting on failure.
        let pane = cx.weak_entity();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            binding.on_action("days", move |_, event, cx| {
                let picked = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str)?;
                let days = CHOICES.iter().map(|(days, _)| *days).find(|days| choice_value(*days) == picked)?;
                let pane = pane.clone();
                // Deferred: the binding is mid-update and this writes it.
                cx.defer(move |cx| {
                    let _ = pane.update(cx, |pane, cx| pane.set_days(days, cx));
                });
                None
            });
            binding
        });
        let view = mount("settings-sessions", document(&Ink::of(cx)), &binding, window, cx);
        Self { loaded_for: None, days: None, binding, view }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(window, cx)
    }

    /// The pane showing `days` (the pictures pin it: no settings file).
    #[cfg(test)]
    pub(crate) fn show(&mut self, days: Option<u32>, cx: &mut gpui::Context<Self>) {
        self.loaded_for = Some(crate::coding_flow::coding_data_dir(cx));
        self.days = days;
        cx.notify();
    }

    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) -> PathBuf {
        let data_dir = crate::coding_flow::coding_data_dir(cx);
        if self.loaded_for.as_ref() != Some(&data_dir) {
            self.days = session_retention::load_days(&data_dir);
            self.loaded_for = Some(data_dir.clone());
        }
        data_dir
    }

    fn set_days(&mut self, days: Option<u32>, cx: &mut gpui::Context<Self>) {
        let data_dir = self.ensure_loaded(cx);
        if self.days == days {
            return;
        }
        let previous = self.days;
        self.days = days;
        cx.notify();
        // File I/O (and the prune, which can walk many journals) stays off
        // the UI thread.
        let write = cx.background_executor().spawn(async move {
            session_retention::save_days(&data_dir, days).map_err(|err| err.to_string())?;
            if days.is_some() {
                let (journals, records) = steer::prune_session_history(&data_dir);
                log::info!(
                    "session history: window now {days:?} days — pruned {journals} journal(s), \
                     {records} record(s)"
                );
            }
            Ok::<(), String>(())
        });
        cx.spawn(async move |this, cx| {
            if let Err(err) = write.await {
                let _ = this.update(cx, |this, cx| {
                    this.days = previous;
                    // FEED-79: the error toast; Retry writes the pick again.
                    let pane = cx.entity().downgrade();
                    crate::action_error::failed_action_with_retry(
                        "save",
                        "the session history setting",
                        Some(err),
                        move |_, cx| {
                            let _ = pane.update(cx, |this, cx| this.set_days(days, cx));
                        },
                        cx,
                    );
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

/// The pane's tree: the band over one picker row.
fn document(ink: &Ink) -> Value {
    let items: Vec<Value> = CHOICES
        .iter()
        .map(|(days, label)| {
            let value = choice_value(*days);
            json!({"label": label, "value": value, "kind": "checkbox", "checked": super::doc::eq(path("/pane/value"), json!(value))})
        })
        .collect();
    let row = ink.picker_row("retention", false, json!(true), "Keep session history", None, path("/pane/label"), "days", items);
    // `settings::section` (`gap_2`).
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [ink.band("band", "Sessions", None), ink.group("group", vec![row])]})
}

impl Render for SessionsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SessionsPane").entered();
        self.ensure_loaded(cx);
        let projection = json!({"label": choice_label(self.days).to_string(), "value": choice_value(self.days)});
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cover_every_choice_and_an_unoffered_value() {
        assert_eq!(choice_label(None), "Unlimited");
        assert_eq!(choice_label(Some(365)), "1 year");
        assert_eq!(choice_label(Some(90)), "90 days");
        assert_eq!(choice_label(Some(30)), "30 days");
        assert_eq!(choice_label(Some(7)), "7 days");
    }
}
