//! EXP-1248 — THE list item, ×4 (web `@exp/ui` SessionRow + PrRow over
//! ListRow, desktop `run_rows::run_row` + `pr_rows::pr_row` over `flat_row`,
//! iOS/Android SessionRow + PrRow), locked by
//! `packages/domain-contract/fixtures/list-item.json`.
//!
//! Anatomy: [tree guides][lead glyph at `BASE + INDENT·depth`][mono
//! identifier][title][caption, big only][trailing meta]. NO fold chevron
//! (children always show), NO trailing chevron, NO buttons: the lead box is
//! [`MARK`] wide on EVERY row, so marks and titles at one depth align
//! exactly and a child's connector elbow ends at its own lead.
//!
//! This module holds the geometry, the PR lead's states and the fixture
//! replay (EXP-1294: session rows carry no caption any more).

/// The base inset of a depth-0 lead.
pub const BASE: f32 = 12.;
/// One nesting level.
pub const INDENT: f32 = 14.;
/// The lead box (run mark / PR node box): one indent level square.
pub const MARK: f32 = 14.;
/// Lead → text gap.
pub const GAP: f32 = 8.;
/// A small (one-line) session row.
pub const SMALL: f32 = 32.;
/// A big (two-line) session row.
pub const BIG: f32 = 52.;
/// A PR row (md+ / desktop).
pub const PR_ROW: f32 = 36.;
/// A PR row on a phone.
pub const PR_ROW_PHONE: f32 = 40.;
/// The PR node's ring diameter.
pub const PR_NODE: f32 = 12.;
/// The stack rail's width.
pub const RAIL: f32 = 1.;

/// The lead's x offset for a row at `depth`.
pub fn lead_x(depth: usize) -> f32 {
    BASE + INDENT * depth as f32
}

/// The PR row's lead: an open PR, the current stack member, the base branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrNodeState {
    Open,
    Current,
    Base,
}

/// The ring's colour role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrNodeRing {
    Emerald,
    Muted,
}

impl PrNodeState {
    /// The fixture's wire word.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "current" => Some(Self::Current),
            "base" => Some(Self::Base),
            _ => None,
        }
    }

    /// Emerald for a PR, muted for the base branch.
    pub fn ring(self) -> PrNodeRing {
        match self {
            Self::Base => PrNodeRing::Muted,
            Self::Open | Self::Current => PrNodeRing::Emerald,
        }
    }

    /// Only the current member fills its centre.
    pub fn filled(self) -> bool {
        self == Self::Current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        geometry: Geometry,
        pr_nodes: Vec<PrNodeCase>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Geometry {
        base: f32,
        indent: f32,
        mark: f32,
        gap: f32,
        small: f32,
        big: f32,
        pr_row: f32,
        pr_row_phone: f32,
        pr_node: f32,
        rail: f32,
    }
    #[derive(Deserialize)]
    struct PrNodeCase {
        name: String,
        state: String,
        ring: String,
        filled: bool,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/list-item.json"
        ))
        .expect("list-item.json parses")
    }

    #[test]
    fn the_geometry_matches_the_fixture() {
        let geometry = fixture().geometry;
        assert_eq!(BASE, geometry.base);
        assert_eq!(INDENT, geometry.indent);
        assert_eq!(MARK, geometry.mark);
        assert_eq!(GAP, geometry.gap);
        assert_eq!(SMALL, geometry.small);
        assert_eq!(BIG, geometry.big);
        assert_eq!(PR_ROW, geometry.pr_row);
        assert_eq!(PR_ROW_PHONE, geometry.pr_row_phone);
        assert_eq!(PR_NODE, geometry.pr_node);
        assert_eq!(RAIL, geometry.rail);
        // The lead box is one indent level, so a child's elbow ends at it.
        assert_eq!(MARK, INDENT);
        assert_eq!(lead_x(2), 40.);
    }

    /// Web PrNode cases, one per fixture `prNodes[].name`.
    #[test]
    fn pr_node_states_match_the_fixture() {
        let cases = fixture().pr_nodes;
        assert!(!cases.is_empty());
        for case in cases {
            let state = PrNodeState::parse(&case.state)
                .unwrap_or_else(|| panic!("{}: unknown state", case.name));
            let ring = match state.ring() {
                PrNodeRing::Emerald => "emerald",
                PrNodeRing::Muted => "muted",
            };
            assert_eq!(ring, case.ring, "{}", case.name);
            assert_eq!(state.filled(), case.filled, "{}", case.name);
        }
    }
}
