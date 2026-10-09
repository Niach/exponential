//! EXP-1029 contract — the IDE styleguide's section skeleton.
//!
//! The styleguide page (`apps/styleguide`) documents every platform's symbol
//! per control. This module mirrors the SAME four ordered sections, read from
//! the ONE
//! index the web page reads (`apps/styleguide/src/sections/sections.json`,
//! embedded at compile time), and one entry file per registered id
//! (`entries/`), so a leaf fills its file on both platforms and never edits
//! an index. EXP-1030 closed that sweep: every entry draws its demo now, and
//! `toast` is the one placeholder left (EXP-1031 owns it). The drift test
//! below keeps the two indexes one.
//!
//! There is NO styleguide surface inside this app and there must not be one:
//! the styleguide is its own deployed application (`apps/styleguide`,
//! styleguide.exponential.at) and several AI actions point at it. This module
//! is the section DATA that index defines, so the IDE's entry ids and owners
//! cannot drift from the page that draws them — it renders nothing and is
//! reachable from no menu.
#![allow(dead_code)]

pub(crate) mod entries;

use serde::Deserialize;

/// The shared section index, byte for byte the web's.
pub(crate) const SECTION_INDEX_JSON: &str =
    include_str!("../../../../../styleguide/src/sections/sections.json");

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Section {
    pub id: String,
    /// 1..4 — the page order.
    pub order: u8,
    /// The numbered heading, `1 Style`.
    pub title: String,
    pub blurb: String,
    /// Entry ids in page order.
    pub entries: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SectionIndex {
    sections: Vec<Section>,
    owners: std::collections::BTreeMap<String, String>,
}

fn index() -> SectionIndex {
    serde_json::from_str(SECTION_INDEX_JSON).expect("sections.json parses")
}

/// The four sections, in page order.
pub(crate) fn sections() -> Vec<Section> {
    let mut sections = index().sections;
    sections.sort_by_key(|section| section.order);
    sections
}

/// The issue that fills `entry_id`.
pub(crate) fn owner_of(entry_id: &str) -> Option<String> {
    index().owners.get(entry_id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_sections_are_numbered_in_order_with_the_agreed_titles() {
        let sections = sections();
        assert_eq!(
            sections.iter().map(|s| s.order).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(
            sections.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
            vec!["1 Style", "2 General components", "3 Special components", "4 Views"]
        );
    }

    #[test]
    fn every_registered_id_has_an_entry_file_and_an_owner_in_index_order() {
        let ids: Vec<&str> = entries::ENTRIES.iter().map(|entry| entry.id).collect();
        let registered: Vec<String> = sections().into_iter().flat_map(|s| s.entries).collect();
        assert_eq!(ids, registered.iter().map(String::as_str).collect::<Vec<_>>());
        for entry in entries::ENTRIES {
            assert_eq!(owner_of(entry.id).as_deref(), Some(entry.owner), "{}", entry.id);
        }
    }

    /// EXP-1030/EXP-1031: every entry is FILLED — each draws its own demo
    /// (`toast` was the last placeholder) and names a real owner.
    #[test]
    fn every_entry_is_filled() {
        const PLACEHOLDERS: [&str; 0] = [];
        // An owner is an issue identifier, `<BOARD>-<number>` (EXP-1031,
        // SLOP-7): uppercase letters, a dash, digits.
        let is_identifier = |owner: &str| {
            owner.split_once('-').is_some_and(|(board, number)| {
                !board.is_empty()
                    && board.chars().all(|c| c.is_ascii_uppercase())
                    && !number.is_empty()
                    && number.chars().all(|c| c.is_ascii_digit())
            })
        };
        for entry in entries::ENTRIES {
            assert!(is_identifier(entry.owner), "{}: {}", entry.id, entry.owner);
        }
        let placeholders: Vec<&str> = entries::ENTRIES
            .iter()
            .filter(|entry| entries::is_placeholder(entry.id))
            .map(|entry| entry.id)
            .collect();
        assert_eq!(placeholders, PLACEHOLDERS, "no entry is a placeholder");
        assert_eq!(owner_of("toast").as_deref(), Some("EXP-1031"));
    }

    /// EXP-1063: an entry is handed the window and the app, so it can draw
    /// the REAL component. Every entry paints in one window, twice (the
    /// second frame reuses the entities `use_keyed_state` kept), and the
    /// one that used to only describe its own — `device-settings` — now
    /// paints the product's own elements. EXP-1092
    /// did the same for `issue-context-menu` (a real `PopupMenu`),
    /// `pr-graph-badge` (`pr_graph::badge`) and `session-row` (the `run_rows`
    /// renderers over `domain::session_tree`) — with NO `sync::Store`
    /// installed here, so every store read on those paths is `try_global`.
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
