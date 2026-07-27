//! EXP-314 — per-team custom issue statuses: the desktop half of the
//! CROSS-PLATFORM status-resolution contract (web `lib/status-icons.ts` +
//! `lib/issue-statuses.ts`, iOS `IssueStatusResolution.swift`, Android
//! `IssueStatusResolution.kt`). No shared code exists — the four platforms
//! share LITERALS, locked by each one's unit tests (the `IssueSorting`
//! precedent).
//!
//! The contract, verbatim:
//!
//! 1. `teamStatuses(rows)` — order by category [`IssueStatusCategory::DISPLAY_ORDER`],
//!    then `sort_order` asc, then `created_at` asc, then `id`. A `started`
//!    row's clock position is its index among the `started` rows of that order.
//! 2. `resolve(issue)` — (a) the team row whose `id == issue.status_id`;
//!    (b) else the team row whose `builtin_key == issue.status`; (c) else a
//!    locally CONSTRUCTED default from the generated contract defaults for
//!    that anchor (unknown/missing anchor → the backlog default). Rendering
//!    must NEVER fail; constructed rows key as `builtin:<key>`.
//! 3. Glyph by category — see [`category_glyph`] / [`started_clock_glyph`].
//! 4. Colors — builtin rows (`builtin_key != None`) and constructed fallbacks
//!    render TODAY's platform token colors keyed on the builtin key, byte
//!    identical to pre-EXP-314 rendering (the synced hex is deliberately
//!    IGNORED for builtins: `theme.foreground`/`muted_foreground` are
//!    theme-reactive here, and a seeded near-white would vanish on a light
//!    theme). Only CUSTOM rows render their stored hex, through the same
//!    parse path the label dots use.

use crate::contract;
use crate::enums::IssueStatus;
use crate::options::{ColorToken, IconGlyph};
use crate::rows::{Issue, IssueStatusRow};

// ---------------------------------------------------------------------------
// Category
// ---------------------------------------------------------------------------

/// `issue_status_category` (contract `issueStatusCategory`), with the §5.5
/// tolerant-unknown fallback: a newer server's category must never drop a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IssueStatusCategory {
    Backlog,
    Unstarted,
    Started,
    Completed,
    Cancelled,
    Duplicate,
    /// Forward-compat: a category this build does not know. Renders like
    /// backlog and sorts last.
    Unknown,
}

impl IssueStatusCategory {
    /// Board display order — locked by test to
    /// `contract::ISSUE_STATUS_CATEGORY_DISPLAY_ORDER`.
    pub const DISPLAY_ORDER: [IssueStatusCategory; 6] = [
        IssueStatusCategory::Started,
        IssueStatusCategory::Unstarted,
        IssueStatusCategory::Backlog,
        IssueStatusCategory::Completed,
        IssueStatusCategory::Cancelled,
        IssueStatusCategory::Duplicate,
    ];

    /// Settings-pane section order — locked by test to
    /// `contract::ISSUE_STATUS_CATEGORY_SETTINGS_ORDER`.
    pub const SETTINGS_ORDER: [IssueStatusCategory; 6] = [
        IssueStatusCategory::Backlog,
        IssueStatusCategory::Unstarted,
        IssueStatusCategory::Started,
        IssueStatusCategory::Completed,
        IssueStatusCategory::Cancelled,
        IssueStatusCategory::Duplicate,
    ];

    pub fn from_wire(wire: &str) -> Self {
        match wire {
            contract::ISSUE_STATUS_CATEGORY_BACKLOG => IssueStatusCategory::Backlog,
            contract::ISSUE_STATUS_CATEGORY_UNSTARTED => IssueStatusCategory::Unstarted,
            contract::ISSUE_STATUS_CATEGORY_STARTED => IssueStatusCategory::Started,
            contract::ISSUE_STATUS_CATEGORY_COMPLETED => IssueStatusCategory::Completed,
            contract::ISSUE_STATUS_CATEGORY_CANCELLED => IssueStatusCategory::Cancelled,
            contract::ISSUE_STATUS_CATEGORY_DUPLICATE => IssueStatusCategory::Duplicate,
            _ => IssueStatusCategory::Unknown,
        }
    }

    pub fn as_wire(&self) -> Option<&'static str> {
        Some(match self {
            IssueStatusCategory::Backlog => contract::ISSUE_STATUS_CATEGORY_BACKLOG,
            IssueStatusCategory::Unstarted => contract::ISSUE_STATUS_CATEGORY_UNSTARTED,
            IssueStatusCategory::Started => contract::ISSUE_STATUS_CATEGORY_STARTED,
            IssueStatusCategory::Completed => contract::ISSUE_STATUS_CATEGORY_COMPLETED,
            IssueStatusCategory::Cancelled => contract::ISSUE_STATUS_CATEGORY_CANCELLED,
            IssueStatusCategory::Duplicate => contract::ISSUE_STATUS_CATEGORY_DUPLICATE,
            IssueStatusCategory::Unknown => return None,
        })
    }

    /// Human label — the settings pane's section headings.
    pub fn label(&self) -> &'static str {
        match self {
            IssueStatusCategory::Backlog => "Backlog",
            IssueStatusCategory::Unstarted => "Unstarted",
            IssueStatusCategory::Started => "Started",
            IssueStatusCategory::Completed => "Completed",
            IssueStatusCategory::Cancelled => "Cancelled",
            IssueStatusCategory::Duplicate => "Duplicate",
            IssueStatusCategory::Unknown => "Other",
        }
    }

    /// Position in [`Self::DISPLAY_ORDER`]; `Unknown` sorts after everything.
    pub fn display_rank(&self) -> usize {
        Self::DISPLAY_ORDER
            .iter()
            .position(|category| category == self)
            .unwrap_or(Self::DISPLAY_ORDER.len())
    }

    /// The enum ANCHOR a status of this category writes (server
    /// `CATEGORY_ANCHOR`). The anchor is what enum-only writers (swipes,
    /// toggles, the coding launcher's parking) and every legacy client see.
    pub fn anchor(&self) -> IssueStatus {
        match self {
            IssueStatusCategory::Backlog => IssueStatus::Backlog,
            IssueStatusCategory::Unstarted => IssueStatus::Todo,
            IssueStatusCategory::Started => IssueStatus::InProgress,
            IssueStatusCategory::Completed => IssueStatus::Done,
            IssueStatusCategory::Cancelled => IssueStatus::Cancelled,
            IssueStatusCategory::Duplicate => IssueStatus::Duplicate,
            IssueStatusCategory::Unknown => IssueStatus::Backlog,
        }
    }
}

// ---------------------------------------------------------------------------
// Tint + resolved status
// ---------------------------------------------------------------------------

/// How a status renders its color (F1): builtin/constructed rows resolve a
/// THEME token (byte identical to pre-EXP-314 rendering, and theme-reactive);
/// custom rows carry their stored `#rrggbb`.
#[derive(Clone, Debug, PartialEq)]
pub enum StatusTint {
    Token(ColorToken),
    Hex(String),
}

/// One status as the UI renders it — a synced row OR a constructed builtin
/// default. Deliberately owned (not `Copy`/`'static`): the vocabulary is
/// per-team data now.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedStatus {
    /// Grouping / filter key: the row id, or `builtin:<key>` for a
    /// constructed default (the statuses shape has not synced yet).
    pub group_key: String,
    /// `Some(id)` only for a real synced row — `None` means "write the enum
    /// anchor, not a `statusId`".
    pub row_id: Option<String>,
    pub name: String,
    pub category: IssueStatusCategory,
    pub tint: StatusTint,
    pub glyph: IconGlyph,
    /// `Some(wire)` for the 7 locked builtins (synced or constructed).
    pub builtin_key: Option<String>,
}

impl ResolvedStatus {
    /// The enum anchor this status writes: its own builtin key when it has
    /// one, else its category's anchor.
    pub fn anchor(&self) -> IssueStatus {
        match self.builtin_key.as_deref() {
            Some(wire) => IssueStatus::from_wire(wire),
            None => self.category.anchor(),
        }
    }

    /// Whether this is a constructed fallback (statuses shape not synced) —
    /// writes must degrade to the enum anchor.
    pub fn is_fallback(&self) -> bool {
        self.row_id.is_none()
    }
}

/// The `builtin:<key>` group key of a constructed default.
pub fn fallback_group_key(builtin_key: &str) -> String {
    format!("builtin:{builtin_key}")
}

// ---------------------------------------------------------------------------
// The 7 builtin defaults (from the generated contract)
// ---------------------------------------------------------------------------

/// One locally-constructed builtin default. Wire strings come STRAIGHT from
/// `contract.generated.rs` (the same parallel arrays the server seeds from);
/// only the color TOKEN is desktop-side, and it is exactly what
/// `ISSUE_STATUS_OPTIONS` rendered before EXP-314.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefaultStatus {
    pub key: &'static str,
    pub category: &'static str,
    pub name: &'static str,
    pub color: ColorToken,
    pub sort_order: i32,
}

const fn default_status(index: usize, color: ColorToken) -> DefaultStatus {
    DefaultStatus {
        key: contract::ISSUE_STATUS_DEFAULT_KEYS[index],
        category: contract::ISSUE_STATUS_DEFAULT_CATEGORIES[index],
        name: contract::ISSUE_STATUS_DEFAULT_NAMES[index],
        color,
        sort_order: contract::ISSUE_STATUS_DEFAULT_SORT_ORDERS[index],
    }
}

/// The 7 builtin statuses every team has, in contract order (backlog, todo,
/// in_progress, in_review, done, cancelled, duplicate).
pub const DEFAULT_STATUSES: [DefaultStatus; 7] = [
    default_status(0, ColorToken::MutedForeground), // backlog
    default_status(1, ColorToken::Foreground),      // todo
    default_status(2, ColorToken::Yellow),          // in_progress
    default_status(3, ColorToken::Green),           // in_review
    default_status(4, ColorToken::Blue),            // done
    default_status(5, ColorToken::MutedForeground), // cancelled
    default_status(6, ColorToken::MutedForeground), // duplicate
];

/// The color token of a builtin key — the ONE mapping builtin rows (synced
/// or constructed) render through. An unknown key degrades to muted.
pub fn builtin_color_token(builtin_key: &str) -> ColorToken {
    DEFAULT_STATUSES
        .iter()
        .find(|default| default.key == builtin_key)
        .map(|default| default.color)
        .unwrap_or(ColorToken::MutedForeground)
}

// ---------------------------------------------------------------------------
// Glyphs
// ---------------------------------------------------------------------------

const CLOCKS_2: [IconGlyph; 2] = [IconGlyph::Progress24, IconGlyph::Progress34];
const CLOCKS_3: [IconGlyph; 3] = [
    IconGlyph::Progress14,
    IconGlyph::Progress24,
    IconGlyph::Progress34,
];
const CLOCKS_4: [IconGlyph; 4] = [
    IconGlyph::Progress15,
    IconGlyph::Progress25,
    IconGlyph::Progress35,
    IconGlyph::Progress45,
];

/// The pie clock of the `index0`-th `started` status of a team that has
/// `count` of them. Cross-platform parity table (web `startedClockIcon`):
/// `count <= 2` → [2/4, 3/4] (the builtin In Progress / In Review pair keeps
/// today's half + ¾ look), `count == 3` → [1/4, 2/4, 3/4], `count >= 4` →
/// [1/5..4/5]. The index is CLAMPED into the table — the server caps started
/// at 4, but racing owners can transiently exceed it (F6).
pub fn started_clock_glyph(index0: usize, count: usize) -> IconGlyph {
    let table: &[IconGlyph] = if count <= 2 {
        &CLOCKS_2
    } else if count == 3 {
        &CLOCKS_3
    } else {
        &CLOCKS_4
    };
    table[index0.min(table.len() - 1)]
}

/// The glyph of a status: fixed per category, except `started`, which picks
/// its clock from its position among the team's started statuses.
pub fn category_glyph(
    category: IssueStatusCategory,
    started_index0: usize,
    started_count: usize,
) -> IconGlyph {
    match category {
        IssueStatusCategory::Backlog => IconGlyph::CircleDashed,
        IssueStatusCategory::Unstarted => IconGlyph::Circle,
        IssueStatusCategory::Started => started_clock_glyph(started_index0, started_count),
        IssueStatusCategory::Completed => IconGlyph::CircleCheck,
        IssueStatusCategory::Cancelled => IconGlyph::CircleX,
        IssueStatusCategory::Duplicate => IconGlyph::Copy,
        // Forward-compat: an unknown category renders like backlog, the same
        // fallback `get_issue_status_config` uses for an unknown status.
        IssueStatusCategory::Unknown => IconGlyph::CircleDashed,
    }
}

// ---------------------------------------------------------------------------
// Team ordering + resolution
// ---------------------------------------------------------------------------

/// Contract rule 1: category display order, then `sort_order` asc, then
/// `created_at` asc, then `id` (a total order — the group order must be
/// identical on every client and stable across re-hydrates).
pub fn sort_team_statuses(rows: &[IssueStatusRow]) -> Vec<IssueStatusRow> {
    let mut out: Vec<IssueStatusRow> = rows.to_vec();
    out.sort_by(|a, b| {
        IssueStatusCategory::from_wire(&a.category)
            .display_rank()
            .cmp(&IssueStatusCategory::from_wire(&b.category).display_rank())
            .then_with(|| {
                a.sort_order
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
            })
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// Resolve one row of an ALREADY-[`sort_team_statuses`]-ordered slice.
pub fn resolve_row(sorted: &[IssueStatusRow], index: usize) -> ResolvedStatus {
    let row = &sorted[index];
    let category = IssueStatusCategory::from_wire(&row.category);
    let (started_index0, started_count) = started_position(sorted, index);
    let tint = match row.builtin_key.as_deref() {
        Some(key) => StatusTint::Token(builtin_color_token(key)),
        None => StatusTint::Hex(row.color.clone().unwrap_or_default()),
    };
    ResolvedStatus {
        group_key: row.id.clone(),
        row_id: Some(row.id.clone()),
        name: row.name.clone(),
        category,
        tint,
        glyph: category_glyph(category, started_index0, started_count),
        builtin_key: row.builtin_key.clone(),
    }
}

/// `(index among started rows, started row count)` for the row at `index`.
fn started_position(sorted: &[IssueStatusRow], index: usize) -> (usize, usize) {
    let started: Vec<usize> = sorted
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            IssueStatusCategory::from_wire(&row.category) == IssueStatusCategory::Started
        })
        .map(|(ix, _)| ix)
        .collect();
    let index0 = started.iter().position(|ix| *ix == index).unwrap_or(0);
    (index0, started.len())
}

/// A constructed builtin default for an enum anchor — contract rule 2(c).
/// An unknown/absent anchor yields the BACKLOG default (web's
/// `getIssueStatusConfig` fallback, unchanged).
pub fn constructed_default(status: IssueStatus) -> ResolvedStatus {
    let wire = status.as_wire().unwrap_or(contract::ISSUE_STATUS_DEFAULT_KEYS[0]);
    let default = DEFAULT_STATUSES
        .iter()
        .find(|default| default.key == wire)
        .unwrap_or(&DEFAULT_STATUSES[0]);
    resolved_from_default(default)
}

fn resolved_from_default(default: &DefaultStatus) -> ResolvedStatus {
    let category = IssueStatusCategory::from_wire(default.category);
    // The constructed set has exactly two started defaults (in_progress,
    // in_review) — index 0 and 1 of a count of 2, i.e. today's 2/4 + 3/4.
    let started: Vec<&DefaultStatus> = DEFAULT_STATUSES
        .iter()
        .filter(|d| IssueStatusCategory::from_wire(d.category) == IssueStatusCategory::Started)
        .collect();
    let index0 = started
        .iter()
        .position(|d| d.key == default.key)
        .unwrap_or(0);
    ResolvedStatus {
        group_key: fallback_group_key(default.key),
        row_id: None,
        name: default.name.to_string(),
        category,
        tint: StatusTint::Token(default.color),
        glyph: category_glyph(category, index0, started.len()),
        builtin_key: Some(default.key.to_string()),
    }
}

/// The constructed default vocabulary, in board display order — what every
/// picker/list shows until the `issue_statuses` shape lands its first
/// snapshot. Byte-identical to the pre-EXP-314 `ISSUE_STATUS_OPTIONS` order.
pub fn default_resolved_statuses() -> Vec<ResolvedStatus> {
    let mut out: Vec<&DefaultStatus> = DEFAULT_STATUSES.iter().collect();
    out.sort_by(|a, b| {
        IssueStatusCategory::from_wire(a.category)
            .display_rank()
            .cmp(&IssueStatusCategory::from_wire(b.category).display_rank())
            .then_with(|| a.sort_order.cmp(&b.sort_order))
    });
    out.into_iter().map(resolved_from_default).collect()
}

/// The team's status vocabulary as the UI renders it: the sorted synced rows,
/// or the constructed defaults while the shape has not synced.
pub fn team_resolved_statuses(rows: &[IssueStatusRow]) -> Vec<ResolvedStatus> {
    let sorted = sort_team_statuses(rows);
    if sorted.is_empty() {
        return default_resolved_statuses();
    }
    (0..sorted.len())
        .map(|index| resolve_row(&sorted, index))
        .collect()
}

/// Contract rule 2 — resolve an issue's status against its team's rows.
/// `rows` need NOT be pre-sorted (they are sorted here); pass the same slice
/// used to build the group list so the clock positions agree.
pub fn resolve_status(issue: &Issue, rows: &[IssueStatusRow]) -> ResolvedStatus {
    let sorted = sort_team_statuses(rows);
    resolve_status_sorted(issue, &sorted)
}

/// [`resolve_status`] over an ALREADY-sorted slice (the hot path — the board
/// query sorts once per frame).
pub fn resolve_status_sorted(issue: &Issue, sorted: &[IssueStatusRow]) -> ResolvedStatus {
    if let Some(status_id) = issue.status_id.as_deref() {
        if let Some(index) = sorted.iter().position(|row| row.id == status_id) {
            return resolve_row(sorted, index);
        }
    }
    if let Some(wire) = issue.status.as_wire() {
        if let Some(index) = sorted
            .iter()
            .position(|row| row.builtin_key.as_deref() == Some(wire))
        {
            return resolve_row(sorted, index);
        }
    }
    constructed_default(issue.status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: &str, category: &str, name: &str, sort: f64, builtin: Option<&str>) -> IssueStatusRow {
        serde_json::from_value(json!({
            "id": id,
            "team_id": "t-1",
            "category": category,
            "name": name,
            "color": "#123456",
            "sort_order": sort,
            "builtin_key": builtin,
            "created_at": "2026-01-01 00:00:00+00",
        }))
        .unwrap()
    }

    fn builtin_rows() -> Vec<IssueStatusRow> {
        DEFAULT_STATUSES
            .iter()
            .enumerate()
            .map(|(ix, default)| {
                row(
                    &format!("row-{ix}"),
                    default.category,
                    default.name,
                    default.sort_order as f64,
                    Some(default.key),
                )
            })
            .collect()
    }

    fn issue(status: &str, status_id: Option<&str>) -> Issue {
        serde_json::from_value(json!({
            "id": "i-1",
            "board_id": "b-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "t",
            "status": status,
            "priority": "none",
            "status_id": status_id,
        }))
        .unwrap()
    }

    #[test]
    fn categories_are_locked_to_the_generated_contract() {
        let values: Vec<&str> = [
            IssueStatusCategory::Backlog,
            IssueStatusCategory::Unstarted,
            IssueStatusCategory::Started,
            IssueStatusCategory::Completed,
            IssueStatusCategory::Cancelled,
            IssueStatusCategory::Duplicate,
        ]
        .iter()
        .map(|c| c.as_wire().unwrap())
        .collect();
        assert_eq!(values, contract::ISSUE_STATUS_CATEGORY_VALUES);

        let display: Vec<&str> = IssueStatusCategory::DISPLAY_ORDER
            .iter()
            .map(|c| c.as_wire().unwrap())
            .collect();
        assert_eq!(display, contract::ISSUE_STATUS_CATEGORY_DISPLAY_ORDER);

        let settings: Vec<&str> = IssueStatusCategory::SETTINGS_ORDER
            .iter()
            .map(|c| c.as_wire().unwrap())
            .collect();
        assert_eq!(settings, contract::ISSUE_STATUS_CATEGORY_SETTINGS_ORDER);

        // Tolerant-unknown (§5.5): a newer server's category never drops a row.
        assert_eq!(IssueStatusCategory::from_wire("triaged"), IssueStatusCategory::Unknown);
        assert_eq!(IssueStatusCategory::Unknown.as_wire(), None);
        assert!(
            IssueStatusCategory::Unknown.display_rank()
                > IssueStatusCategory::Duplicate.display_rank()
        );
    }

    #[test]
    fn defaults_mirror_the_generated_contract_arrays() {
        let keys: Vec<&str> = DEFAULT_STATUSES.iter().map(|d| d.key).collect();
        assert_eq!(keys, contract::ISSUE_STATUS_DEFAULT_KEYS);
        let categories: Vec<&str> = DEFAULT_STATUSES.iter().map(|d| d.category).collect();
        assert_eq!(categories, contract::ISSUE_STATUS_DEFAULT_CATEGORIES);
        let names: Vec<&str> = DEFAULT_STATUSES.iter().map(|d| d.name).collect();
        assert_eq!(names, contract::ISSUE_STATUS_DEFAULT_NAMES);
        let sorts: Vec<i32> = DEFAULT_STATUSES.iter().map(|d| d.sort_order).collect();
        assert_eq!(sorts, contract::ISSUE_STATUS_DEFAULT_SORT_ORDERS);
        // The keys ARE the anchor enum values.
        assert_eq!(keys, contract::ISSUE_STATUS_VALUES);
    }

    #[test]
    fn builtin_tokens_are_todays_option_table_colors() {
        // F1 byte-lock: builtin rendering must not change with this feature.
        for option in &crate::options::ISSUE_STATUS_OPTIONS {
            let wire = option.value.as_wire().unwrap();
            assert_eq!(builtin_color_token(wire), option.color, "{wire}");
        }
        assert_eq!(
            builtin_color_token("something_new"),
            ColorToken::MutedForeground
        );
    }

    #[test]
    fn started_clock_table_is_the_cross_platform_literal_list() {
        // THE parity table (web `startedClockIcon`, iOS/Android mirrors).
        let names = |count: usize| -> Vec<&'static str> {
            (0..count)
                .map(|ix| started_clock_glyph(ix, count).file_name())
                .collect()
        };
        assert_eq!(names(1), vec!["progress-2-4"]);
        assert_eq!(names(2), vec!["progress-2-4", "progress-3-4"]);
        assert_eq!(
            names(3),
            vec!["progress-1-4", "progress-2-4", "progress-3-4"]
        );
        assert_eq!(
            names(4),
            vec!["progress-1-5", "progress-2-5", "progress-3-5", "progress-4-5"]
        );
        // F6: a transiently over-cap team clamps into the 4-clock table.
        assert_eq!(
            names(5),
            vec![
                "progress-1-5",
                "progress-2-5",
                "progress-3-5",
                "progress-4-5",
                "progress-4-5"
            ]
        );
        assert_eq!(started_clock_glyph(9, 2).file_name(), "progress-3-4");
        assert_eq!(contract::ISSUE_STATUS_STARTED_MAX, 4);
    }

    #[test]
    fn category_glyphs_are_the_fixed_table() {
        let glyph = |category| category_glyph(category, 0, 2).file_name();
        assert_eq!(glyph(IssueStatusCategory::Backlog), "circle-dashed");
        assert_eq!(glyph(IssueStatusCategory::Unstarted), "circle");
        assert_eq!(glyph(IssueStatusCategory::Started), "progress-2-4");
        assert_eq!(glyph(IssueStatusCategory::Completed), "circle-check");
        assert_eq!(glyph(IssueStatusCategory::Cancelled), "circle-x");
        assert_eq!(glyph(IssueStatusCategory::Duplicate), "copy");
        assert_eq!(glyph(IssueStatusCategory::Unknown), "circle-dashed");
    }

    #[test]
    fn constructed_defaults_reproduce_todays_vocabulary() {
        let resolved = default_resolved_statuses();
        let names: Vec<&str> = resolved.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["In Progress", "In Review", "Todo", "Backlog", "Done", "Cancelled", "Duplicate"]
        );
        let glyphs: Vec<&str> = resolved.iter().map(|r| r.glyph.file_name()).collect();
        assert_eq!(
            glyphs,
            vec![
                "progress-2-4",
                "progress-3-4",
                "circle",
                "circle-dashed",
                "circle-check",
                "circle-x",
                "copy"
            ]
        );
        for status in &resolved {
            assert!(status.is_fallback());
            assert!(status.group_key.starts_with("builtin:"));
            assert!(matches!(status.tint, StatusTint::Token(_)));
        }
        // The fallback vocabulary walks the enum display order.
        let anchors: Vec<&str> = resolved
            .iter()
            .map(|r| r.anchor().as_wire().unwrap())
            .collect();
        assert_eq!(anchors, contract::ISSUE_STATUS_DISPLAY_ORDER);
    }

    #[test]
    fn team_order_is_category_then_sort_then_created_then_id() {
        let rows = vec![
            row("z", "started", "QA", 2.0, None),
            row("a", "started", "Building", 1.0, Some("in_progress")),
            row("m", "backlog", "Backlog", 1.0, Some("backlog")),
            row("n", "completed", "Done", 1.0, Some("done")),
            row("o", "unstarted", "Todo", 1.0, Some("todo")),
        ];
        let names: Vec<String> = team_resolved_statuses(&rows)
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["Building", "QA", "Todo", "Backlog", "Done"]);

        // Ties break on created_at, then id.
        let mut a = row("b", "backlog", "A", 1.0, None);
        let mut b = row("a", "backlog", "B", 1.0, None);
        a.created_at = Some("2026-01-02 00:00:00+00".into());
        b.created_at = Some("2026-01-01 00:00:00+00".into());
        let sorted = sort_team_statuses(&[a.clone(), b.clone()]);
        assert_eq!(sorted[0].id, "a");
        a.created_at = b.created_at.clone();
        let sorted = sort_team_statuses(&[a, b]);
        assert_eq!(sorted[0].id, "a", "id breaks the final tie");
    }

    #[test]
    fn started_clocks_follow_position_within_the_team() {
        let rows = vec![
            row("s1", "started", "Building", 1.0, Some("in_progress")),
            row("s2", "started", "Review", 2.0, Some("in_review")),
            row("s3", "started", "QA", 3.0, None),
        ];
        let glyphs: Vec<&str> = team_resolved_statuses(&rows)
            .iter()
            .map(|r| r.glyph.file_name())
            .collect();
        assert_eq!(glyphs, vec!["progress-1-4", "progress-2-4", "progress-3-4"]);
    }

    #[test]
    fn resolve_prefers_status_id_then_anchor_then_constructed() {
        let rows = builtin_rows();
        let sorted = sort_team_statuses(&rows);
        let custom = row("custom-1", "started", "QA", 9.0, None);
        let mut with_custom = rows.clone();
        with_custom.push(custom);

        // (a) status_id hits a synced row.
        let resolved = resolve_status(&issue("todo", Some("custom-1")), &with_custom);
        assert_eq!(resolved.name, "QA");
        assert_eq!(resolved.row_id.as_deref(), Some("custom-1"));
        assert_eq!(resolved.group_key, "custom-1");
        assert!(!resolved.is_fallback());
        assert_eq!(resolved.anchor(), IssueStatus::InProgress);
        assert_eq!(resolved.tint, StatusTint::Hex("#123456".into()));

        // (b) an unknown/absent status_id falls back to the anchor's builtin.
        let resolved = resolve_status_sorted(&issue("done", None), &sorted);
        assert_eq!(resolved.name, "Done");
        assert_eq!(resolved.builtin_key.as_deref(), Some("done"));
        assert_eq!(resolved.tint, StatusTint::Token(ColorToken::Blue));
        let resolved = resolve_status_sorted(&issue("done", Some("gone")), &sorted);
        assert_eq!(resolved.builtin_key.as_deref(), Some("done"));

        // (c) no rows at all → the constructed default.
        let resolved = resolve_status(&issue("in_review", None), &[]);
        assert_eq!(resolved.group_key, "builtin:in_review");
        assert_eq!(resolved.glyph.file_name(), "progress-3-4");
        assert!(resolved.is_fallback());

        // Unknown anchor → the backlog default, never a panic.
        let resolved = resolve_status(&issue("triaged", None), &[]);
        assert_eq!(resolved.group_key, "builtin:backlog");
        let resolved = resolve_status_sorted(&issue("triaged", None), &sorted);
        assert_eq!(resolved.group_key, "builtin:backlog");
    }
}
