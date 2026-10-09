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
//! The big session row's caption is [`crate::session_row`]; this module
//! holds the geometry, the PR lead's states and the fixture replay.

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
    use crate::session_row::{
        list_elapsed, session_row_caption, SessionRowCaptionInput, SessionRowState,
    };
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        geometry: Geometry,
        elapsed: Vec<Elapsed>,
        captions: Vec<Caption>,
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
    struct Elapsed {
        name: String,
        ms: i64,
        expected: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Caption {
        name: String,
        ended: bool,
        paused: bool,
        state: String,
        device: Option<String>,
        started_at: Option<String>,
        updated_at: Option<String>,
        ended_at: Option<String>,
        blocked_label: Option<String>,
        now: String,
        expected: Expected,
    }
    #[derive(Deserialize)]
    struct Expected {
        text: String,
        tone: String,
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

    /// `YYYY-MM-DDTHH:MM:SSZ` → epoch ms; anything else is unparsable (the
    /// UI parses real columns with chrono; the fixture only needs this form).
    fn iso_ms(value: &str) -> Option<i64> {
        let bytes = value.as_bytes();
        if bytes.len() != 20 || bytes[10] != b'T' || bytes[19] != b'Z' {
            return None;
        }
        let num = |range: std::ops::Range<usize>| value.get(range)?.parse::<i64>().ok();
        let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
        let (hh, mm, ss) = (num(11..13)?, num(14..16)?, num(17..19)?);
        // Howard Hinnant's days-from-civil.
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        Some(((days * 24 + hh) * 60 + mm) * 60_000 + ss * 1_000)
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

    /// Web `describe('listElapsed')`, one case per fixture `name`.
    #[test]
    fn list_elapsed_matches_the_fixture() {
        let cases = fixture().elapsed;
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(list_elapsed(case.ms), case.expected, "{}", case.name);
        }
    }

    /// Web `describe('sessionRowCaption')`, one case per fixture `name`.
    #[test]
    fn session_row_caption_matches_the_fixture() {
        let cases = fixture().captions;
        assert!(!cases.is_empty());
        for case in cases {
            let state = SessionRowState::parse(&case.state)
                .unwrap_or_else(|| panic!("{}: unknown state", case.name));
            let (text, tone) = session_row_caption(SessionRowCaptionInput {
                ended: case.ended,
                paused: case.paused,
                state,
                device: case.device.as_deref(),
                started_ms: case.started_at.as_deref().and_then(iso_ms),
                updated_ms: case.updated_at.as_deref().and_then(iso_ms),
                ended_ms: case.ended_at.as_deref().and_then(iso_ms),
                blocked_label: case.blocked_label.as_deref(),
                now_ms: iso_ms(&case.now).expect("now parses"),
            });
            assert_eq!(text, case.expected.text, "{}", case.name);
            assert_eq!(tone.as_str(), case.expected.tone, "{}", case.name);
        }
    }

    /// Web `paints every live state in session-display.json's statusTone`.
    #[test]
    fn paints_every_live_state_in_session_display_json_s_status_tone() {
        let display: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-display.json"
        ))
        .expect("session-display.json parses");
        let cases = display["cases"].as_array().expect("cases");
        assert!(!cases.is_empty());
        for case in cases {
            if case["status"].as_str() == Some("ended") {
                continue;
            }
            let name = case["name"].as_str().unwrap_or_default();
            let state = SessionRowState::parse(case["state"].as_str().unwrap_or_default())
                .unwrap_or_else(|| panic!("{name}: unknown state"));
            let (_, tone) = session_row_caption(SessionRowCaptionInput {
                ended: false,
                paused: false,
                state,
                device: Some("mint"),
                started_ms: None,
                updated_ms: None,
                ended_ms: None,
                blocked_label: None,
                now_ms: 0,
            });
            assert_eq!(tone.as_str(), case["statusTone"].as_str().unwrap(), "{name}");
        }
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
