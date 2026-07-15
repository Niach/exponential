//! Semantic git-flow lanes for the Source Control screen: default branch →
//! `exp/batch-*` batch-run branches → issue branches → everything else,
//! joined to the synced issue rows purely BY BRANCH NAME (no commit graph).
//! [`build_lanes`] is pure and unit-tested; the screen renders the result as
//! simple indented flex rows with a PR-state tone, ↑/↓ badges, and a
//! "worktree" tag.
//!
//! Local-branch-only by design (known-accepted): work coded on another
//! device shows no lane until its branch exists here.

use coding::branch_name;
use coding::scm::BranchInfo;

/// What a lane semantically is (drives indent + tone + click behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneKind {
    /// The repo's default branch — always the first lane.
    Default,
    /// An `exp/batch-*` batch-run branch (multi-issue coding session; PR
    /// tone joins from any synced issue linked to the branch — the batch's
    /// issues share ONE combined PR).
    Batch,
    /// A branch joined to a synced issue (server `issue.branch` first, then
    /// the computed `<prefix><IDENTIFIER>` fallback).
    Issue,
    /// Any other local branch (muted).
    Other,
}

/// PR-state tone for the lane dot: open = green, merged = blue, closed =
/// red, none = muted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrTone {
    Open,
    Merged,
    Closed,
    None,
}

/// One rendered lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane {
    /// The local git branch name (the click target for view-branch lanes).
    pub branch: String,
    /// Display label (`IDENT · title` / the branch itself).
    pub label: String,
    pub kind: LaneKind,
    /// Indent level: 0 = default, 1 = everything else.
    pub indent: u8,
    pub pr: PrTone,
    /// vs `origin/<branch>` — filled by the caller for VISIBLE lanes only
    /// (bounded git cost); `None` = unknown/no upstream.
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    /// Checked out in the trunk clone right now.
    pub current: bool,
    /// Lives in a session worktree.
    pub worktree: bool,
    /// The joined issue's id — issue lanes navigate to the issue on click.
    pub issue_id: Option<String>,
}

/// What leads a lane row (EXP-67): the default lane carries nothing, PR
/// states keep their tones (merged upgraded from a blue dot to a green
/// check), and PR-less lanes with local work — a dirty worktree or commits
/// ahead of origin — get the yellow in-progress badge; only truly idle
/// lanes stay a muted dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneIndicator {
    /// The default branch — no indicator at all.
    None,
    /// PR merged — green check (the done-status glyph).
    MergedCheck,
    /// PR open — green dot.
    OpenDot,
    /// PR closed — red dot.
    ClosedDot,
    /// No PR, but local work exists (dirty worktree or ahead of origin) —
    /// yellow in-progress badge (the in-progress-status glyph).
    Progress,
    /// Stale/idle — muted dot.
    IdleDot,
}

/// Pure indicator pick for one lane. `ahead` is the background ↑ count when
/// known; `dirty` is the lane worktree's (or, for the current lane, the
/// trunk's) uncommitted-changes probe. PR state is authoritative: a merged
/// lane keeps its check even if the worktree is dirty again.
pub fn lane_indicator(
    kind: LaneKind,
    pr: PrTone,
    ahead: Option<u32>,
    dirty: bool,
) -> LaneIndicator {
    if matches!(kind, LaneKind::Default) {
        return LaneIndicator::None;
    }
    match pr {
        PrTone::Merged => LaneIndicator::MergedCheck,
        PrTone::Open => LaneIndicator::OpenDot,
        PrTone::Closed => LaneIndicator::ClosedDot,
        PrTone::None => {
            if dirty || ahead.is_some_and(|ahead| ahead > 0) {
                LaneIndicator::Progress
            } else {
                LaneIndicator::IdleDot
            }
        }
    }
}

/// The built lane list (EXP-67: uncapped — the sidebar scrolls the full list
/// instead of collapsing it behind a "+N more" toggle).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlowModel {
    pub lanes: Vec<Lane>,
}

/// The synced issue fields the join needs (kept `coding`/gpui-free so the
/// builder stays pure and testable).
#[derive(Debug, Clone)]
pub struct IssueLite {
    pub id: String,
    pub identifier: String,
    pub title: String,
    /// Server truth (`issues.branch`) — wins over the computed name.
    pub branch: Option<String>,
    pub pr_state: Option<String>,
}

/// Max label length before the issue title is ellipsized.
const MAX_LABEL: usize = 56;

/// Build the lane model from local branches + synced rows. Ordering: default
/// lane (always), batch branches, issue branches, other branches. Joins are
/// by branch name only: issues via `issue.branch` then the computed
/// `<prefix><IDENTIFIER>`; batch lanes take their PR tone from any issue
/// whose server branch IS the batch branch (they share the combined PR).
pub fn build_lanes(
    branches: &[BranchInfo],
    default_branch: &str,
    branch_prefix: &str,
    issues: &[IssueLite],
) -> FlowModel {
    let default_info = branches.iter().find(|b| b.name == default_branch);
    let default_lane = Lane {
        branch: default_branch.to_string(),
        label: default_branch.to_string(),
        kind: LaneKind::Default,
        indent: 0,
        pr: PrTone::None,
        ahead: None,
        behind: None,
        current: default_info.is_some_and(|b| b.current),
        worktree: false,
        issue_id: None,
    };

    // Non-default local branches, in git's order (current first, then
    // most-recently-committed).
    let rest: Vec<&BranchInfo> = branches
        .iter()
        .filter(|b| b.name != default_branch)
        .collect();

    // Batch lanes: every local `exp/batch-*` branch. Every issue of a batch
    // run shares its combined PR, so any linked issue supplies the PR tone.
    let mut batch_lanes: Vec<Lane> = Vec::new();
    for info in rest.iter().filter(|b| b.name.starts_with("exp/batch-")) {
        let matched = issues
            .iter()
            .find(|issue| issue.branch.as_deref() == Some(info.name.as_str()));
        batch_lanes.push(Lane {
            branch: info.name.clone(),
            label: info.name.clone(),
            kind: LaneKind::Batch,
            indent: 1,
            pr: pr_tone(matched.and_then(|issue| issue.pr_state.as_deref())),
            ahead: None,
            behind: None,
            current: info.current,
            worktree: info.worktree,
            issue_id: None,
        });
    }

    // Issue + other lanes from the remaining branches.
    let mut issue_lanes: Vec<Lane> = Vec::new();
    let mut others: Vec<Lane> = Vec::new();
    for info in rest.iter().filter(|b| !b.name.starts_with("exp/batch-")) {
        let issue = issues.iter().find(|issue| {
            issue
                .branch
                .as_deref()
                .filter(|branch| !branch.is_empty())
                .map_or_else(
                    || branch_name(branch_prefix, &issue.identifier) == info.name,
                    |branch| branch == info.name,
                )
        });
        let Some(issue) = issue else {
            others.push(Lane {
                branch: info.name.clone(),
                label: info.name.clone(),
                kind: LaneKind::Other,
                indent: 1,
                pr: PrTone::None,
                ahead: None,
                behind: None,
                current: info.current,
                worktree: info.worktree,
                issue_id: None,
            });
            continue;
        };
        issue_lanes.push(Lane {
            branch: info.name.clone(),
            label: truncate_label(&format!("{} · {}", issue.identifier, issue.title)),
            kind: LaneKind::Issue,
            indent: 1,
            pr: pr_tone(issue.pr_state.as_deref()),
            ahead: None,
            behind: None,
            current: info.current,
            worktree: info.worktree,
            issue_id: Some(issue.id.clone()),
        });
    }

    // Flatten: default · batch branches · issue branches · other branches.
    let mut flat: Vec<Lane> = Vec::new();
    flat.append(&mut batch_lanes);
    flat.append(&mut issue_lanes);
    flat.append(&mut others);

    let mut lanes = Vec::with_capacity(flat.len() + 1);
    lanes.push(default_lane);
    lanes.extend(flat);
    FlowModel { lanes }
}

/// Tree-connector geometry for one lane row — drives the graph gutter the
/// Source Control flow section draws (continuous vertical rails + ├/└
/// elbows, so the lanes read as an actual branch graph instead of an
/// indented list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneConnector {
    /// Pass-through vertical rails for gutter columns `0..indent-1` (an
    /// ancestor's subtree continues below this row).
    pub rails: Vec<bool>,
    /// The lane's own elbow in column `indent-1`: `Some(true)` = └ (last
    /// sibling), `Some(false)` = ├. `None` for the root lane (indent 0).
    pub elbow: Option<bool>,
}

/// Compute the connector gutter for the (possibly truncated) lane list.
/// Pure companion of [`build_lanes`] — same flat ordering in, one connector
/// per lane out.
pub fn connectors(lanes: &[Lane]) -> Vec<LaneConnector> {
    lanes
        .iter()
        .enumerate()
        .map(|(ix, lane)| {
            let indent = usize::from(lane.indent);
            if indent == 0 {
                return LaneConnector {
                    rails: Vec::new(),
                    elbow: None,
                };
            }
            let rails = (0..indent - 1)
                .map(|depth| has_following_sibling(lanes, ix, depth as u8))
                .collect();
            let last = !has_following_sibling(lanes, ix, (indent - 1) as u8);
            LaneConnector {
                rails,
                elbow: Some(last),
            }
        })
        .collect()
}

/// Whether the gutter column at `depth` continues below row `ix`: a later
/// lane sits at `depth + 1` before that column's parent subtree closes (a
/// lane at `indent <= depth`).
fn has_following_sibling(lanes: &[Lane], ix: usize, depth: u8) -> bool {
    for lane in &lanes[ix + 1..] {
        if lane.indent <= depth {
            return false;
        }
        if lane.indent == depth + 1 {
            return true;
        }
    }
    false
}

/// Map a synced `pr_state` value onto the lane tone.
fn pr_tone(pr_state: Option<&str>) -> PrTone {
    match pr_state {
        Some("open") => PrTone::Open,
        Some("merged") => PrTone::Merged,
        Some("closed") => PrTone::Closed,
        _ => PrTone::None,
    }
}

/// Ellipsize a lane label at a char boundary.
fn truncate_label(label: &str) -> String {
    if label.chars().count() <= MAX_LABEL {
        return label.to_string();
    }
    let cut: String = label.chars().take(MAX_LABEL - 1).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, current: bool, worktree: bool) -> BranchInfo {
        BranchInfo { name: name.to_string(), current, worktree }
    }

    fn issue(
        id: &str,
        identifier: &str,
        branch: Option<&str>,
        pr_state: Option<&str>,
    ) -> IssueLite {
        IssueLite {
            id: id.to_string(),
            identifier: identifier.to_string(),
            title: format!("Title of {identifier}"),
            branch: branch.map(str::to_string),
            pr_state: pr_state.map(str::to_string),
        }
    }

    #[test]
    fn default_lane_is_always_first_even_without_a_local_branch_row() {
        let model = build_lanes(&[], "main", "exp/", &[]);
        assert_eq!(model.lanes.len(), 1);
        assert_eq!(model.lanes[0].kind, LaneKind::Default);
        assert_eq!(model.lanes[0].branch, "main");
        assert!(!model.lanes[0].current);
    }

    #[test]
    fn batch_lane_takes_its_pr_tone_from_a_linked_issue() {
        let branches = [
            branch("main", true, false),
            branch("exp/batch-a1b2c3d4", false, true),
            // Issue joined via server branch truth.
            branch("exp/EXP-7", false, true),
            // Plain issue branch (computed-name fallback).
            branch("exp/EXP-9", false, false),
        ];
        let issues = [
            // Two batch issues share the combined PR on the batch branch;
            // they render NO issue lanes of their own (no own branch).
            issue("i1", "EXP-1", Some("exp/batch-a1b2c3d4"), Some("open")),
            issue("i2", "EXP-2", Some("exp/batch-a1b2c3d4"), Some("open")),
            issue("i7", "EXP-7", Some("exp/EXP-7"), Some("merged")),
            issue("i9", "EXP-9", None, None),
        ];

        let model = build_lanes(&branches, "main", "exp/", &issues);
        let kinds: Vec<(LaneKind, u8)> =
            model.lanes.iter().map(|lane| (lane.kind, lane.indent)).collect();
        assert_eq!(
            kinds,
            vec![
                (LaneKind::Default, 0),
                (LaneKind::Batch, 1),
                (LaneKind::Issue, 1),
                (LaneKind::Issue, 1),
            ]
        );
        assert!(model.lanes[0].current);
        // Batch lane: the branch itself as label, the shared PR's tone, the
        // worktree tag, no issue click target.
        assert_eq!(model.lanes[1].label, "exp/batch-a1b2c3d4");
        assert_eq!(model.lanes[1].pr, PrTone::Open);
        assert!(model.lanes[1].worktree);
        assert_eq!(model.lanes[1].issue_id, None);
        // Issue lane: identifier · title label, tone, click target.
        assert_eq!(model.lanes[2].label, "EXP-7 · Title of EXP-7");
        assert_eq!(model.lanes[2].pr, PrTone::Merged);
        assert_eq!(model.lanes[2].issue_id.as_deref(), Some("i7"));
        // Computed-name fallback joined EXP-9 without server branch truth.
        assert_eq!(model.lanes[3].issue_id.as_deref(), Some("i9"));
    }

    #[test]
    fn unlinked_batch_branch_gets_a_toneless_batch_lane() {
        let branches = [branch("main", true, false), branch("exp/batch-deadbeef", false, false)];
        let model = build_lanes(&branches, "main", "exp/", &[]);
        assert_eq!(model.lanes[1].kind, LaneKind::Batch);
        assert_eq!(model.lanes[1].label, "exp/batch-deadbeef");
        assert_eq!(model.lanes[1].pr, PrTone::None);
    }

    #[test]
    fn server_branch_truth_wins_over_the_computed_name() {
        // The issue's server branch is nonstandard; the computed name for a
        // DIFFERENT issue must not steal the lane.
        let branches = [branch("main", true, false), branch("feat/custom", false, false)];
        let issues = [
            issue("i1", "EXP-1", Some("feat/custom"), Some("closed")),
            issue("i2", "EXP-2", None, None),
        ];
        let model = build_lanes(&branches, "main", "exp/", &issues);
        assert_eq!(model.lanes[1].issue_id.as_deref(), Some("i1"));
        assert_eq!(model.lanes[1].pr, PrTone::Closed);
    }

    #[test]
    fn unjoined_branches_are_muted_other_lanes() {
        let branches = [branch("main", false, false), branch("spike/wild-idea", true, false)];
        let model = build_lanes(&branches, "main", "exp/", &[]);
        assert_eq!(model.lanes[1].kind, LaneKind::Other);
        assert!(model.lanes[1].current);
        assert_eq!(model.lanes[1].indent, 1);
    }

    #[test]
    fn every_lane_is_kept_no_cap() {
        // EXP-67: the sidebar scrolls the full list — no "+N more" cap.
        let names: Vec<String> = (1..=8).map(|n| format!("branch-{n}")).collect();
        let mut branches = vec![branch("main", true, false)];
        branches.extend(names.iter().map(|name| branch(name, false, false)));

        let model = build_lanes(&branches, "main", "exp/", &[]);
        assert_eq!(model.lanes.len(), 1 + 8);
    }

    #[test]
    fn connectors_draw_rails_and_elbows_like_a_tree() {
        // Hand-built lanes with a nested subtree — connectors() is generic
        // over indent levels even though build_lanes emits only 0/1 today.
        fn lane(indent: u8) -> Lane {
            Lane {
                branch: String::new(),
                label: String::new(),
                kind: LaneKind::Other,
                indent,
                pr: PrTone::None,
                ahead: None,
                behind: None,
                current: false,
                worktree: false,
                issue_id: None,
            }
        }
        // root(0) · parent(1) · child(2) · child(2) · sibling(1)
        let lanes = [lane(0), lane(1), lane(2), lane(2), lane(1)];
        let connectors = connectors(&lanes);
        // Root: no gutter at all.
        assert_eq!(connectors[0], LaneConnector { rails: vec![], elbow: None });
        // Parent: ├ (the sibling follows at root level).
        assert_eq!(connectors[1], LaneConnector { rails: vec![], elbow: Some(false) });
        // First child: rail through column 0 (sibling below), ├ in column 1.
        assert_eq!(
            connectors[2],
            LaneConnector { rails: vec![true], elbow: Some(false) }
        );
        // Last child: └, rail still passing through.
        assert_eq!(
            connectors[3],
            LaneConnector { rails: vec![true], elbow: Some(true) }
        );
        // Sibling: last root-level lane → └.
        assert_eq!(connectors[4], LaneConnector { rails: vec![], elbow: Some(true) });
    }

    #[test]
    fn lane_indicator_maps_states_like_exp_67_asks() {
        use LaneIndicator::*;
        // Master/default: nothing — even when dirty or ahead.
        assert_eq!(lane_indicator(LaneKind::Default, PrTone::None, Some(3), true), None);
        // Merged: green check, authoritative over local noise.
        assert_eq!(lane_indicator(LaneKind::Issue, PrTone::Merged, Some(2), true), MergedCheck);
        // Open / closed PRs keep their dots.
        assert_eq!(lane_indicator(LaneKind::Issue, PrTone::Open, Option::None, false), OpenDot);
        assert_eq!(lane_indicator(LaneKind::Issue, PrTone::Closed, Option::None, false), ClosedDot);
        // No PR + local work → yellow progress (dirty OR ahead).
        assert_eq!(lane_indicator(LaneKind::Issue, PrTone::None, Option::None, true), Progress);
        assert_eq!(lane_indicator(LaneKind::Other, PrTone::None, Some(1), false), Progress);
        assert_eq!(lane_indicator(LaneKind::Batch, PrTone::None, Some(0), true), Progress);
        // Stale: muted dot (unknown counts stay idle, not progress).
        assert_eq!(lane_indicator(LaneKind::Issue, PrTone::None, Option::None, false), IdleDot);
        assert_eq!(lane_indicator(LaneKind::Other, PrTone::None, Some(0), false), IdleDot);
    }

    #[test]
    fn long_issue_titles_are_ellipsized() {
        let long = "x".repeat(200);
        let branches = [branch("main", true, false), branch("exp/EXP-1", false, false)];
        let issues = [IssueLite {
            id: "i1".to_string(),
            identifier: "EXP-1".to_string(),
            title: long,
            branch: None,
            pr_state: None,
        }];
        let model = build_lanes(&branches, "main", "exp/", &issues);
        assert!(model.lanes[1].label.chars().count() <= MAX_LABEL);
        assert!(model.lanes[1].label.ends_with('…'));
    }
}
