//! EXP-1029 contract — the IDE styleguide's section skeleton.
//!
//! The IDE had no styleguide surface of its own: the web page
//! (`apps/styleguide`) documents every platform's symbol per control. This
//! module gives the IDE the SAME four ordered sections, read from the ONE
//! index the web page reads (`apps/styleguide/src/sections/sections.json`,
//! embedded at compile time), and one placeholder entry file per registered
//! id (`entries/`), so a leaf fills its file on both platforms and never
//! edits an index. EXP-1039 gave it its screen ([`screen`], a debug-only
//! `Screen::Styleguide` behind `EXP_DEV_STYLEGUIDE=1`) and the IDE's own
//! starting set of live entries ([`native`], filed under the same four
//! sections); the drift test below keeps the two indexes one.
#![allow(dead_code)]

pub(crate) mod entries;
pub(crate) mod native;
pub(crate) mod screen;

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

    #[test]
    fn a_placeholder_renders_its_owner() {
        // Every entry is a placeholder until its leaf lands; the demo is a
        // `Div`, so the text is checked on the constants it is built from.
        for entry in entries::ENTRIES {
            assert!(entry.owner.starts_with("EXP-"), "{}", entry.id);
        }
    }
}
