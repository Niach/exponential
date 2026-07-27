//! Status/priority option tables — mirror of `apps/web/src/lib/domain.ts`
//! (masterplan-v3 §4.7). Do not re-derive: value order, labels, glyphs and
//! color roles are copied from the web tables verbatim.
//!
//! `domain` is gpui-free (§3.1 dependency rule), so the table carries
//! *presentation data*, not gpui types: [`IconGlyph`] names the Lucide SVG
//! (one-to-one with the `domain.ts` import names — the SVG files ship in
//! `apps/desktop/assets/icons/` and the `ui` crate's `ExpIcon` enum is
//! generated from them), and [`ColorToken`] names the theme token the color
//! resolves from (web Tailwind classes → the generated design-token accents).
//! The `ui` crate maps both to `gpui_component::Icon` / `gpui::Hsla`.

use crate::enums::{IssuePriority, IssueStatus};

/// A Lucide glyph, named after its SVG file (and the `lucide-react` import in
/// `domain.ts`). [`IconGlyph::file_name`] is the contract with
/// `apps/desktop/assets/icons/{file_name}.svg` — the `ui` crate's mapper is an
/// exhaustive match, so a new glyph here fails compilation there until the
/// SVG + mapping land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconGlyph {
    /// web `CircleDashed` — status backlog.
    CircleDashed,
    /// web `Circle` — status todo.
    Circle,
    /// web `Timer` — the pre-EXP-314 in_progress glyph. Kept: the registry
    /// still ships `timer.svg` and the variant costs nothing.
    Timer,
    /// web `GitPullRequest` — PR affordances (and the pre-EXP-314 in_review
    /// status glyph).
    GitPullRequest,
    /// EXP-314 pie clock 1/4 — a `started` status at position 0 of 3.
    Progress14,
    /// EXP-314 pie clock 2/4 — a `started` status at position 0 of ≤2.
    Progress24,
    /// EXP-314 pie clock 3/4 — a `started` status at position 1 of ≤2.
    Progress34,
    /// EXP-314 pie clock 1/5 — a `started` status at position 0 of ≥4.
    Progress15,
    /// EXP-314 pie clock 2/5 — a `started` status at position 1 of ≥4.
    Progress25,
    /// EXP-314 pie clock 3/5 — a `started` status at position 2 of ≥4.
    Progress35,
    /// EXP-314 pie clock 4/5 — a `started` status at position 3 of ≥4.
    Progress45,
    /// web `CircleCheck` — status done.
    CircleCheck,
    /// web `CircleX` — status cancelled.
    CircleX,
    /// web `Copy` — status duplicate.
    Copy,
    /// web `Minus` — priority none.
    Minus,
    /// web `AlertTriangle` — priority urgent (the Lucide file is
    /// `triangle-alert.svg`; `AlertTriangle` is the legacy react alias).
    TriangleAlert,
    /// web `SignalHigh` — priority high.
    SignalHigh,
    /// web `SignalMedium` — priority medium.
    SignalMedium,
    /// web `SignalLow` — priority low.
    SignalLow,
}

impl IconGlyph {
    /// The SVG file name (without `.svg`) in `apps/desktop/assets/icons/`.
    pub fn file_name(&self) -> &'static str {
        match self {
            IconGlyph::CircleDashed => "circle-dashed",
            IconGlyph::Circle => "circle",
            IconGlyph::Timer => "timer",
            IconGlyph::GitPullRequest => "git-pull-request",
            IconGlyph::Progress14 => "progress-1-4",
            IconGlyph::Progress24 => "progress-2-4",
            IconGlyph::Progress34 => "progress-3-4",
            IconGlyph::Progress15 => "progress-1-5",
            IconGlyph::Progress25 => "progress-2-5",
            IconGlyph::Progress35 => "progress-3-5",
            IconGlyph::Progress45 => "progress-4-5",
            IconGlyph::CircleCheck => "circle-check",
            IconGlyph::CircleX => "circle-x",
            IconGlyph::Copy => "copy",
            IconGlyph::Minus => "minus",
            IconGlyph::TriangleAlert => "triangle-alert",
            IconGlyph::SignalHigh => "signal-high",
            IconGlyph::SignalMedium => "signal-medium",
            IconGlyph::SignalLow => "signal-low",
        }
    }
}

/// The color role an option renders with — web Tailwind classes mapped onto
/// theme/design tokens (§4.3: status/priority accents are token-locked, never
/// loose hex in Rust).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorToken {
    /// web `text-muted-foreground`.
    MutedForeground,
    /// web `text-foreground`.
    Foreground,
    /// web `text-yellow-500` → generated accent `YELLOW`.
    Yellow,
    /// web `text-green-500` → generated accent `GREEN`.
    Green,
    /// web `text-red-500` → generated accent `RED`.
    Red,
    /// web `text-orange-500` → generated accent `ORANGE`.
    Orange,
    /// web `text-blue-500` → generated accent `BLUE`.
    Blue,
}

/// Web `IssueOption<TValue>` — one row of an option table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IssueOption<V: 'static> {
    pub value: V,
    pub label: &'static str,
    pub icon: IconGlyph,
    pub color: ColorToken,
}

const fn opt<V>(value: V, label: &'static str, icon: IconGlyph, color: ColorToken) -> IssueOption<V> {
    IssueOption {
        value,
        label,
        icon,
        color,
    }
}

/// Web `issueStatusOptions` — same order, labels, glyphs, colors.
///
/// REV2-85: the table is ordered by the contract `displayOrder`
/// (`contract::ISSUE_STATUS_DISPLAY_ORDER`, locked by test).
///
/// EXP-314: this static table is no longer the picker vocabulary — per-team
/// [`crate::statuses`] rows are. It survives as the **anchor-enum fallback**
/// vocabulary the cross-team surfaces (search, tab chips, My Issues) render
/// from, and as the source of the builtin color tokens. The in_progress /
/// in_review glyphs moved to the pie clocks in step with the shared registry's
/// `status-in-progress` / `status-in-review` concepts.
pub const ISSUE_STATUS_OPTIONS: [IssueOption<IssueStatus>; 7] = [
    opt(
        IssueStatus::InProgress,
        "In Progress",
        IconGlyph::Progress24,
        ColorToken::Yellow,
    ),
    opt(
        IssueStatus::InReview,
        "In Review",
        IconGlyph::Progress34,
        ColorToken::Green,
    ),
    opt(
        IssueStatus::Todo,
        "Todo",
        IconGlyph::Circle,
        ColorToken::Foreground,
    ),
    opt(
        IssueStatus::Backlog,
        "Backlog",
        IconGlyph::CircleDashed,
        ColorToken::MutedForeground,
    ),
    opt(
        IssueStatus::Done,
        "Done",
        IconGlyph::CircleCheck,
        ColorToken::Blue,
    ),
    opt(
        IssueStatus::Cancelled,
        "Cancelled",
        IconGlyph::CircleX,
        ColorToken::MutedForeground,
    ),
    opt(
        IssueStatus::Duplicate,
        "Duplicate",
        IconGlyph::Copy,
        ColorToken::MutedForeground,
    ),
];

/// Web `issuePriorityOptions` — same labels, glyphs, colors, and the same
/// contract `displayOrder` as the status table (REV2-85).
pub const ISSUE_PRIORITY_OPTIONS: [IssueOption<IssuePriority>; 5] = [
    opt(
        IssuePriority::Urgent,
        "Urgent",
        IconGlyph::TriangleAlert,
        ColorToken::Red,
    ),
    opt(
        IssuePriority::High,
        "High",
        IconGlyph::SignalHigh,
        ColorToken::Orange,
    ),
    opt(
        IssuePriority::Medium,
        "Medium",
        IconGlyph::SignalMedium,
        ColorToken::Yellow,
    ),
    opt(
        IssuePriority::Low,
        "Low",
        IconGlyph::SignalLow,
        ColorToken::Blue,
    ),
    opt(
        IssuePriority::None,
        "No priority",
        IconGlyph::Minus,
        ColorToken::MutedForeground,
    ),
];

/// Web `getIssueStatusConfig` — find-or-fallback. Unknown/forward-compat
/// values render as `backlog` (web parity: the START of the lifecycle, which
/// is no longer the first row of the display-ordered table).
pub fn get_issue_status_config(status: IssueStatus) -> &'static IssueOption<IssueStatus> {
    ISSUE_STATUS_OPTIONS
        .iter()
        .find(|option| option.value == status)
        .unwrap_or_else(|| {
            ISSUE_STATUS_OPTIONS
                .iter()
                .find(|option| option.value == IssueStatus::Backlog)
                .expect("backlog is in the status table")
        })
}

/// Web `getIssuePriorityConfig` — find-or-fallback (unknown → no priority).
pub fn get_issue_priority_config(priority: IssuePriority) -> &'static IssueOption<IssuePriority> {
    ISSUE_PRIORITY_OPTIONS
        .iter()
        .find(|option| option.value == priority)
        .unwrap_or_else(|| {
            ISSUE_PRIORITY_OPTIONS
                .iter()
                .find(|option| option.value == IssuePriority::None)
                .expect("none is in the priority table")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_options_mirror_web_table() {
        // REV2-85: the option table IS the contract display order — the one
        // picker vocabulary shared with web/iOS/Android.
        let order: Vec<&str> = ISSUE_STATUS_OPTIONS
            .iter()
            .map(|o| o.value.as_wire().unwrap())
            .collect();
        assert_eq!(order, crate::contract::ISSUE_STATUS_DISPLAY_ORDER);
        let labels: Vec<_> = ISSUE_STATUS_OPTIONS.iter().map(|o| o.label).collect();
        assert_eq!(
            labels,
            vec!["In Progress", "In Review", "Todo", "Backlog", "Done", "Cancelled", "Duplicate"]
        );
        // Labels agree with the enum's own label() (single source of display
        // truth across the two P2/P3 surfaces).
        for option in &ISSUE_STATUS_OPTIONS {
            assert_eq!(option.label, option.value.label());
        }
    }

    #[test]
    fn priority_options_mirror_web_table() {
        let order: Vec<&str> = ISSUE_PRIORITY_OPTIONS
            .iter()
            .map(|o| o.value.as_wire().unwrap())
            .collect();
        assert_eq!(order, crate::contract::ISSUE_PRIORITY_DISPLAY_ORDER);
        for option in &ISSUE_PRIORITY_OPTIONS {
            assert_eq!(option.label, option.value.label());
        }
        // Spot-check the web color mapping (domain.ts).
        assert_eq!(
            get_issue_priority_config(IssuePriority::Urgent).color,
            ColorToken::Red
        );
        assert_eq!(
            get_issue_priority_config(IssuePriority::High).color,
            ColorToken::Orange
        );
        assert_eq!(
            get_issue_priority_config(IssuePriority::Medium).color,
            ColorToken::Yellow
        );
        assert_eq!(
            get_issue_priority_config(IssuePriority::Low).color,
            ColorToken::Blue
        );
    }

    #[test]
    fn config_lookups_fall_back_to_backlog_and_no_priority() {
        // web getOptionConfig: options.find(...) ?? backlog / no-priority.
        assert_eq!(
            get_issue_status_config(IssueStatus::Unknown).value,
            IssueStatus::Backlog
        );
        assert_eq!(
            get_issue_priority_config(IssuePriority::Unknown).value,
            IssuePriority::None
        );
        // Known values resolve to themselves.
        assert_eq!(
            get_issue_status_config(IssueStatus::Done).value,
            IssueStatus::Done
        );
    }

    #[test]
    fn glyph_file_names_line_up_with_lucide() {
        // §4.7: "The SVG file names must line up with domain.ts's glyph names
        // so the mapping is one-to-one."
        assert_eq!(IconGlyph::CircleDashed.file_name(), "circle-dashed");
        assert_eq!(IconGlyph::TriangleAlert.file_name(), "triangle-alert");
        assert_eq!(IconGlyph::SignalMedium.file_name(), "signal-medium");
        // Every glyph used by the tables names a distinct file.
        let mut names: Vec<_> = ISSUE_STATUS_OPTIONS
            .iter()
            .map(|o| o.icon.file_name())
            .chain(ISSUE_PRIORITY_OPTIONS.iter().map(|o| o.icon.file_name()))
            .collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate glyph file names");
    }
}
