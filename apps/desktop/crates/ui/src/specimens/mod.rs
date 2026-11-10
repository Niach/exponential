//! EXP-1259 / #EXP-1779 — the dev-only SPECIMEN screens behind the desktop
//! captures. Each entry (`entries/`, one file per id) draws one component or
//! view in the states the app never shows together, full-window, for the
//! desktop rows of `packages/view-catalog/views.json` (`capture.desktop` =
//! `specimen:<id>`, the pictures in `shots/`). This is not a styleguide: the
//! component catalogue is the genUI page (exponential.at/sdk/genui).
//!
//! Reachable from no menu: only `EXP_DEV_SCREEN=specimen:<id>`
//! ([`crate::navigation::Screen::Specimen`]) draws an entry. The registry
//! ([`entries::ENTRIES`], [`entries::find`]) is the one index; an entry no
//! capture uses is deleted, not kept.
#![allow(dead_code)]

pub(crate) mod entries;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id is unique and resolves through [`entries::find`].
    #[test]
    fn every_entry_is_found_by_its_id() {
        let mut ids: Vec<&str> = entries::ENTRIES.iter().map(|entry| entry.id).collect();
        for id in &ids {
            assert_eq!(entries::find(id).map(|entry| entry.id), Some(*id));
        }
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), entries::ENTRIES.len(), "duplicate ids");
        assert!(entries::find("nope").is_none());
    }

    /// EXP-1063: an entry is handed the window and the app, so it can draw
    /// the REAL component. Every entry paints in one window, twice (the
    /// second frame reuses the entities `use_keyed_state` kept) — with NO
    /// `sync::Store` installed here, so every store read on those paths is
    /// `try_global`.
    #[gpui::test]
    async fn every_entry_paints_with_the_window_and_the_app(cx: &mut gpui::TestAppContext) {
        use gpui::{div, IntoElement, ParentElement as _, Render, Window};

        struct Gallery;

        impl Render for Gallery {
            fn render(
                &mut self,
                window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Gallery").entered();
                let mut gallery = div();
                for entry in entries::ENTRIES {
                    gallery = gallery.child((entry.render)(window, cx));
                }
                gallery
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_, _| Gallery);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}
