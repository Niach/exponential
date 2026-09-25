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

use gpui::{IntoElement, ParentElement, Render, SharedString, Styled, Window};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    ActiveTheme as _,
};

use coding::session_retention::{self, CHOICES};

use crate::surface::{glass_group_rows, glass_picker_row, picker_value_label};

use super::{error_notice, section};

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
    save_error: Option<SharedString>,
}

impl SessionsPane {
    pub fn new(_cx: &mut gpui::Context<Self>) -> Self {
        Self {
            loaded_for: None,
            days: None,
            save_error: None,
        }
    }

    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) -> PathBuf {
        let data_dir = crate::coding_flow::coding_data_dir(cx);
        if self.loaded_for.as_ref() != Some(&data_dir) {
            self.days = session_retention::load_days(&data_dir);
            self.loaded_for = Some(data_dir.clone());
            self.save_error = None;
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
        self.save_error = None;
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
                    this.save_error = Some(format!("Couldn't save the setting: {err}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

impl Render for SessionsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);
        let current = self.days;
        let value_color = cx.theme().foreground.opacity(0.7);

        let row = glass_picker_row(
            "Keep session history",
            None,
            Button::new("sessions-retention-select")
                .ghost()
                .cursor_pointer()
                .h_auto()
                .px_0()
                .py_0()
                .text_color(value_color)
                .dropdown_caret(true)
                // EXP-697: NOT `.label()` — see notifications_prefs.
                .child(picker_value_label(choice_label(current)))
                .dropdown_menu({
                    let entity = cx.entity();
                    move |mut menu, _, _| {
                        for (days, label) in CHOICES {
                            let entity = entity.clone();
                            menu = menu.item(
                                PopupMenuItem::new(label)
                                    .checked(current == days)
                                    .on_click(move |_, _, cx| {
                                        entity.update(cx, |this, cx| this.set_days(days, cx));
                                    }),
                            );
                        }
                        menu
                    }
                })
                .into_any_element(),
            cx,
        );

        let mut card = section(cx)
            .child(crate::surface::glass_section_header("Sessions", None, cx))
            .child(glass_group_rows(vec![row]));
        if let Some(error) = &self.save_error {
            card = card.child(error_notice(error.clone(), cx));
        }
        card
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
