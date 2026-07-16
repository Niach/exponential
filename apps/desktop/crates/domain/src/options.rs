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
    /// web `Timer` — status in_progress.
    Timer,
    /// web `GitPullRequest` — status in_review.
    GitPullRequest,
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
pub const ISSUE_STATUS_OPTIONS: [IssueOption<IssueStatus>; 7] = [
    opt(
        IssueStatus::Backlog,
        "Backlog",
        IconGlyph::CircleDashed,
        ColorToken::MutedForeground,
    ),
    opt(
        IssueStatus::Todo,
        "Todo",
        IconGlyph::Circle,
        ColorToken::Foreground,
    ),
    opt(
        IssueStatus::InProgress,
        "In Progress",
        IconGlyph::Timer,
        ColorToken::Yellow,
    ),
    opt(
        IssueStatus::InReview,
        "In Review",
        IconGlyph::GitPullRequest,
        ColorToken::Green,
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

/// Web `issuePriorityOptions` — same order, labels, glyphs, colors.
pub const ISSUE_PRIORITY_OPTIONS: [IssueOption<IssuePriority>; 5] = [
    opt(
        IssuePriority::None,
        "No priority",
        IconGlyph::Minus,
        ColorToken::MutedForeground,
    ),
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
];

/// Web `getIssueStatusConfig` — find-or-first-fallback (unknown/forward-compat
/// values render as the first option, exactly like web).
pub fn get_issue_status_config(status: IssueStatus) -> &'static IssueOption<IssueStatus> {
    ISSUE_STATUS_OPTIONS
        .iter()
        .find(|option| option.value == status)
        .unwrap_or(&ISSUE_STATUS_OPTIONS[0])
}

/// Web `getIssuePriorityConfig` — find-or-first-fallback.
pub fn get_issue_priority_config(priority: IssuePriority) -> &'static IssueOption<IssuePriority> {
    ISSUE_PRIORITY_OPTIONS
        .iter()
        .find(|option| option.value == priority)
        .unwrap_or(&ISSUE_PRIORITY_OPTIONS[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_options_mirror_web_table() {
        // Order + labels are the web table verbatim.
        let values: Vec<_> = ISSUE_STATUS_OPTIONS.iter().map(|o| o.value).collect();
        assert_eq!(
            values,
            vec![
                IssueStatus::Backlog,
                IssueStatus::Todo,
                IssueStatus::InProgress,
                IssueStatus::InReview,
                IssueStatus::Done,
                IssueStatus::Cancelled,
                IssueStatus::Duplicate,
            ]
        );
        let labels: Vec<_> = ISSUE_STATUS_OPTIONS.iter().map(|o| o.label).collect();
        assert_eq!(
            labels,
            vec!["Backlog", "Todo", "In Progress", "In Review", "Done", "Cancelled", "Duplicate"]
        );
        // Labels agree with the enum's own label() (single source of display
        // truth across the two P2/P3 surfaces).
        for option in &ISSUE_STATUS_OPTIONS {
            assert_eq!(option.label, option.value.label());
        }
    }

    #[test]
    fn priority_options_mirror_web_table() {
        let values: Vec<_> = ISSUE_PRIORITY_OPTIONS.iter().map(|o| o.value).collect();
        assert_eq!(
            values,
            vec![
                IssuePriority::None,
                IssuePriority::Urgent,
                IssuePriority::High,
                IssuePriority::Medium,
                IssuePriority::Low,
            ]
        );
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
    fn config_lookups_fall_back_to_first_option() {
        // web getOptionConfig: options.find(...) ?? fallback(first).
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
