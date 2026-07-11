//! Semantic git-flow lanes for the Source Control screen: default branch →
//! `exp/rel-*` release branches → issue branches → everything else, joined to
//! the synced issue/release rows purely BY BRANCH NAME (no commit graph).
//! [`build_lanes`] is pure and unit-tested; the screen renders the result as
//! simple indented flex rows with a PR-state tone, ↑/↓ badges, and a
//! "worktree" tag.
//!
//! Local-branch-only by design (known-accepted): a release coded on another
//! device shows no lane until its branch exists here.

use coding::scm::BranchInfo;
use coding::{branch_name, release_branch_name, release_slug};

/// What a lane semantically is (drives indent + tone + click behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneKind {
    /// The repo's default branch — always the first lane.
    Default,
    /// An `exp/rel-*` integration branch (matched to a synced release when
    /// the slugged name joins, generic otherwise).
    Release,
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
    /// Display label (release name / `IDENT · title` / the branch itself).
    pub label: String,
    pub kind: LaneKind,
    /// Indent level: 0 = default, 1 = release/orphan/other, 2 = an issue
    /// under its release lane.
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

/// The built strip: capped lanes + how many non-default lanes were hidden.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlowModel {
    pub lanes: Vec<Lane>,
    pub hidden: usize,
}

/// Non-default lanes shown while collapsed; "+N more" beyond.
pub const MAX_LANES: usize = 5;

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
    pub release_id: Option<String>,
}

/// The synced release fields the join needs.
#[derive(Debug, Clone)]
pub struct ReleaseLite {
    pub id: String,
    pub name: String,
    pub pr_state: Option<String>,
}

/// Max label length before the issue title is ellipsized.
const MAX_LABEL: usize = 56;

/// Build the lane model from local branches + synced rows. Ordering: default
/// lane (always), release lanes with their issue lanes indented beneath,
/// orphan issue branches, other branches. Joins are by branch name only:
/// releases via `release_branch_name(release_slug(name, id))`, issues via
/// `issue.branch` then the computed `<prefix><IDENTIFIER>`.
pub fn build_lanes(
    branches: &[BranchInfo],
    default_branch: &str,
    branch_prefix: &str,
    issues: &[IssueLite],
    releases: &[ReleaseLite],
    expanded: bool,
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

    // Release lanes: every local `exp/rel-*` branch, joined to a synced
    // release via the slugged branch name.
    let mut release_lanes: Vec<(Option<String>, Lane)> = Vec::new(); // (release_id, lane)
    for info in rest.iter().filter(|b| b.name.starts_with("exp/rel-")) {
        let matched = releases
            .iter()
            .find(|release| release_branch_name(&release_slug(&release.name, &release.id)) == info.name);
        release_lanes.push((
            matched.map(|release| release.id.clone()),
            Lane {
                branch: info.name.clone(),
                label: matched
                    .map(|release| release.name.clone())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| info.name.clone()),
                kind: LaneKind::Release,
                indent: 1,
                pr: pr_tone(matched.and_then(|release| release.pr_state.as_deref())),
                ahead: None,
                behind: None,
                current: info.current,
                worktree: info.worktree,
                issue_id: None,
            },
        ));
    }

    // Issue + other lanes from the remaining branches.
    let mut release_children: Vec<Vec<Lane>> = vec![Vec::new(); release_lanes.len()];
    let mut orphans: Vec<Lane> = Vec::new();
    let mut others: Vec<Lane> = Vec::new();
    for info in rest.iter().filter(|b| !b.name.starts_with("exp/rel-")) {
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
        let parent = issue.release_id.as_deref().and_then(|release_id| {
            release_lanes
                .iter()
                .position(|(id, _)| id.as_deref() == Some(release_id))
        });
        let lane = Lane {
            branch: info.name.clone(),
            label: truncate_label(&format!("{} · {}", issue.identifier, issue.title)),
            kind: LaneKind::Issue,
            indent: if parent.is_some() { 2 } else { 1 },
            pr: pr_tone(issue.pr_state.as_deref()),
            ahead: None,
            behind: None,
            current: info.current,
            worktree: info.worktree,
            issue_id: Some(issue.id.clone()),
        };
        match parent {
            Some(ix) => release_children[ix].push(lane),
            None => orphans.push(lane),
        }
    }

    // Flatten: default · releases (each followed by its issues) · orphan
    // issue branches · other branches.
    let mut flat: Vec<Lane> = Vec::new();
    for (ix, (_, lane)) in release_lanes.into_iter().enumerate() {
        flat.push(lane);
        flat.append(&mut release_children[ix]);
    }
    flat.append(&mut orphans);
    flat.append(&mut others);

    let hidden = if expanded || flat.len() <= MAX_LANES {
        0
    } else {
        flat.len() - MAX_LANES
    };
    if hidden > 0 {
        flat.truncate(MAX_LANES);
    }

    let mut lanes = Vec::with_capacity(flat.len() + 1);
    lanes.push(default_lane);
    lanes.extend(flat);
    FlowModel { lanes, hidden }
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
        release_id: Option<&str>,
    ) -> IssueLite {
        IssueLite {
            id: id.to_string(),
            identifier: identifier.to_string(),
            title: format!("Title of {identifier}"),
            branch: branch.map(str::to_string),
            pr_state: pr_state.map(str::to_string),
            release_id: release_id.map(str::to_string),
        }
    }

    fn release(id: &str, name: &str, pr_state: Option<&str>) -> ReleaseLite {
        ReleaseLite {
            id: id.to_string(),
            name: name.to_string(),
            pr_state: pr_state.map(str::to_string),
        }
    }

    #[test]
    fn default_lane_is_always_first_even_without_a_local_branch_row() {
        let model = build_lanes(&[], "main", "exp/", &[], &[], false);
        assert_eq!(model.lanes.len(), 1);
        assert_eq!(model.lanes[0].kind, LaneKind::Default);
        assert_eq!(model.lanes[0].branch, "main");
        assert!(!model.lanes[0].current);
        assert_eq!(model.hidden, 0);
    }

    #[test]
    fn release_lane_joins_by_slugged_branch_and_groups_its_issues() {
        let release_id = "1dc5fb4a-8923-471c-a940-53094cd33b76";
        let rel_branch = release_branch_name(&release_slug("Release 4", release_id));
        let branches = [
            branch("main", true, false),
            branch(&rel_branch, false, false),
            // Issue joined via server branch truth, bundled in the release.
            branch("exp/EXP-7", false, true),
            // Orphan issue branch (computed-name fallback, no release).
            branch("exp/EXP-9", false, false),
        ];
        let issues = [
            issue("i7", "EXP-7", Some("exp/EXP-7"), Some("open"), Some(release_id)),
            issue("i9", "EXP-9", None, None, None),
        ];
        let releases = [release(release_id, "Release 4", Some("merged"))];

        let model = build_lanes(&branches, "main", "exp/", &issues, &releases, false);
        let kinds: Vec<(LaneKind, u8)> =
            model.lanes.iter().map(|lane| (lane.kind, lane.indent)).collect();
        assert_eq!(
            kinds,
            vec![
                (LaneKind::Default, 0),
                (LaneKind::Release, 1),
                (LaneKind::Issue, 2), // EXP-7 under its release
                (LaneKind::Issue, 1), // EXP-9 orphan
            ]
        );
        assert!(model.lanes[0].current);
        // Release lane carries the release NAME + its PR tone.
        assert_eq!(model.lanes[1].label, "Release 4");
        assert_eq!(model.lanes[1].pr, PrTone::Merged);
        // Issue lane: identifier · title label, open tone, worktree tag,
        // click target.
        assert_eq!(model.lanes[2].label, "EXP-7 · Title of EXP-7");
        assert_eq!(model.lanes[2].pr, PrTone::Open);
        assert!(model.lanes[2].worktree);
        assert_eq!(model.lanes[2].issue_id.as_deref(), Some("i7"));
        // Computed-name fallback joined EXP-9 without server branch truth.
        assert_eq!(model.lanes[3].issue_id.as_deref(), Some("i9"));
        assert_eq!(model.lanes[3].pr, PrTone::None);
    }

    #[test]
    fn unmatched_release_branch_gets_a_generic_release_lane() {
        let branches = [branch("main", true, false), branch("exp/rel-mystery-12345678", false, false)];
        let model = build_lanes(&branches, "main", "exp/", &[], &[], false);
        assert_eq!(model.lanes[1].kind, LaneKind::Release);
        assert_eq!(model.lanes[1].label, "exp/rel-mystery-12345678");
        assert_eq!(model.lanes[1].pr, PrTone::None);
    }

    #[test]
    fn server_branch_truth_wins_over_the_computed_name() {
        // The issue's server branch is nonstandard; the computed name for a
        // DIFFERENT issue must not steal the lane.
        let branches = [branch("main", true, false), branch("feat/custom", false, false)];
        let issues = [
            issue("i1", "EXP-1", Some("feat/custom"), Some("closed"), None),
            issue("i2", "EXP-2", None, None, None),
        ];
        let model = build_lanes(&branches, "main", "exp/", &issues, &[], false);
        assert_eq!(model.lanes[1].issue_id.as_deref(), Some("i1"));
        assert_eq!(model.lanes[1].pr, PrTone::Closed);
    }

    #[test]
    fn unjoined_branches_are_muted_other_lanes() {
        let branches = [branch("main", false, false), branch("spike/wild-idea", true, false)];
        let model = build_lanes(&branches, "main", "exp/", &[], &[], false);
        assert_eq!(model.lanes[1].kind, LaneKind::Other);
        assert!(model.lanes[1].current);
        assert_eq!(model.lanes[1].indent, 1);
    }

    #[test]
    fn cap_hides_beyond_max_lanes_and_expanded_shows_all() {
        let names: Vec<String> = (1..=8).map(|n| format!("branch-{n}")).collect();
        let mut branches = vec![branch("main", true, false)];
        branches.extend(names.iter().map(|name| branch(name, false, false)));

        let collapsed = build_lanes(&branches, "main", "exp/", &[], &[], false);
        // default + MAX_LANES shown, the rest counted as hidden.
        assert_eq!(collapsed.lanes.len(), 1 + MAX_LANES);
        assert_eq!(collapsed.hidden, 8 - MAX_LANES);

        let expanded = build_lanes(&branches, "main", "exp/", &[], &[], true);
        assert_eq!(expanded.lanes.len(), 1 + 8);
        assert_eq!(expanded.hidden, 0);
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
            release_id: None,
        }];
        let model = build_lanes(&branches, "main", "exp/", &issues, &[], false);
        assert!(model.lanes[1].label.chars().count() <= MAX_LABEL);
        assert!(model.lanes[1].label.ends_with('…'));
    }
}
