//! EXP-1244: the Reviews queue, ONE pure function ×4 (web
//! `lib/reviews-queue.ts` `reviewsQueue`, iOS `ReviewsQueue.build`, Android
//! `ReviewsQueue.build`), locked by
//! `packages/domain-contract/fixtures/reviews-queue.json` (its `_doc` = the
//! rules). Synced issues + runs + the `repositories.openPulls` results →
//! board bands, "Agent runs" bands and the unlinked-PR repo bands.
//!
//! 1. Scope = the boards of `teams`; an issue counts only when its board is
//!    in scope, a session only when its team is.
//! 2. An ENTRY = the in-scope issues with `pr_state` open sharing one
//!    `pr_url` (an empty/absent url keys `issue:<id>`), issues in
//!    [`representative_order`]; `issues[0]` = the representative.
//! 3. Entries list under the representative's board, newest first.
//! 4. Board bands: team order, `sort_order` ASC (null last), name
//!    case-insensitive, id.
//! 5. LINKED urls = every in-scope issue's and session's `pr_url`, any state.
//! 6. RUN entries = in-scope issue-less sessions with an open PR no in-scope
//!    issue carries; newest per url; one band per team.
//! 7. REPO bands = the in-scope teams' pulls, team order then input order,
//!    linked pulls dropped, empty repos hidden.
//! 8. `count` = entries + runs + repo pulls.
//! 9. EXP-1248 (`_groupDoc`): each board band also carries [`queue_items`]
//!    = its entries as drawn: a PR TREE nests under its root (depth), a
//!    linear STACK is ONE item top first over its base branch, a lone PR is
//!    flat. Bands draw NO count; `count` only lights the nav dot.
//!
//! The Reviews nav entry reads the SAME queue ([`reviews_nav`], the
//! fixture's `navCases`): dot = `count > 0`; the entry shows iff no team is
//! in scope, some team in scope is not in yolo mode, or the dot is lit.
//!
//! Timestamps are the synced ISO strings, compared lexicographically (one
//! source, one format; `None` sorts last under the descending compare).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::rows::{Board, CodingSession, Issue};

/// A repository band's trailing caption ×4 (`labels.repoBandCaption`).
pub const REPO_BAND_CAPTION: &str = "not linked to an issue";

/// The "Agent runs" band's trailing caption ×4 (`labels.runBandCaption`).
pub const RUN_BAND_CAPTION: &str = "opened by a coding run";

const OPEN: &str = "open";

fn present(url: Option<&str>) -> Option<&str> {
    url.filter(|url| !url.is_empty())
}

/// Newest first by `created_at`, id ascending on a tie: which issue
/// REPRESENTS a pull request several share (EXP-917, the merge target) and
/// the order every queue list reads in.
pub fn representative_order(a: &Issue, b: &Issue) -> Ordering {
    b.created_at
        .cmp(&a.created_at)
        .then_with(|| a.id.cmp(&b.id))
}

fn session_order(a: &CodingSession, b: &CodingSession) -> Ordering {
    b.created_at
        .cmp(&a.created_at)
        .then_with(|| a.id.cmp(&b.id))
}

/// One repository's `repositories.openPulls` result, scoped to its team.
pub struct PullRepo<'a, P> {
    pub team_id: &'a str,
    pub repository_id: &'a str,
    pub pulls: &'a [P],
}

/// One open PR: every issue it links, `issues[0]` = the representative.
#[derive(Debug, Clone)]
pub struct QueueEntry<'a> {
    pub key: String,
    pub issues: Vec<&'a Issue>,
}

impl<'a> QueueEntry<'a> {
    pub fn representative(&self) -> &'a Issue {
        self.issues[0]
    }
}

/// EXP-1248: one display item of a board band (rule 9).
#[derive(Debug, Clone)]
pub enum QueueItem<'a> {
    /// A lone PR (depth 0) or a member of a PR TREE, pre-order under its root.
    Pr { entry: QueueEntry<'a>, depth: usize },
    /// A linear STACK: its entries TOP first, then the base-branch row.
    Stack {
        entries: Vec<QueueEntry<'a>>,
        base_branch: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct BoardGroup<'a> {
    pub board: &'a Board,
    /// Every entry, flat, newest first (rule 3).
    pub entries: Vec<QueueEntry<'a>>,
    /// The same entries as the band draws them (rule 9).
    pub items: Vec<QueueItem<'a>>,
}

fn edge(branch: Option<&str>) -> Option<&str> {
    branch.filter(|branch| !branch.is_empty())
}

/// (9) A band's entries as items (web `queueItems`). Edge: an entry sits on
/// the entry (same band) whose representative's `branch` is its
/// representative's `pr_base_branch` (the first entry in band order owns a
/// branch). A component lists where its NEWEST entry would, walked from its
/// ROOT (a cycle breaks where the climb first repeats). Any entry with 2+
/// children = a TREE (pre-order, children in band order, depth = distance
/// from the root); otherwise 2+ entries = ONE stack item, top first, its
/// base = the root's `pr_base_branch`. One entry = depth 0.
pub fn queue_items<'a>(entries: &[QueueEntry<'a>]) -> Vec<QueueItem<'a>> {
    let mut owner: HashMap<&str, usize> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if let Some(branch) = edge(entry.representative().branch.as_deref()) {
            owner.entry(branch).or_insert(index);
        }
    }
    let mut parent_of: HashMap<usize, usize> = HashMap::new();
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(parent) = edge(entry.representative().pr_base_branch.as_deref())
            .and_then(|base| owner.get(base).copied())
        else {
            continue;
        };
        if entries[parent].key == entry.key {
            continue;
        }
        parent_of.insert(index, parent);
        children.entry(parent).or_default().push(index);
    }
    let mut placed: HashSet<usize> = HashSet::new();
    let mut items: Vec<QueueItem<'a>> = Vec::new();
    for start in 0..entries.len() {
        if placed.contains(&start) {
            continue;
        }
        // Climb to the root; a cycle stops where it first repeats.
        let mut root = start;
        let mut climbed: HashSet<usize> = HashSet::from([root]);
        while let Some(&parent) = parent_of.get(&root) {
            if climbed.contains(&parent) || placed.contains(&parent) {
                break;
            }
            climbed.insert(parent);
            root = parent;
        }
        // The component under the root, pre-order, children in band order.
        let mut members: Vec<(usize, usize)> = Vec::new();
        let mut fork = false;
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some((index, depth)) = stack.pop() {
            if !placed.insert(index) {
                continue;
            }
            members.push((index, depth));
            let below: Vec<usize> = children
                .get(&index)
                .map(|kids| kids.iter().copied().filter(|kid| !placed.contains(kid)).collect())
                .unwrap_or_default();
            if below.len() > 1 {
                fork = true;
            }
            // Reversed onto the stack so the first child is visited first.
            for kid in below.into_iter().rev() {
                stack.push((kid, depth + 1));
            }
        }
        if members.len() > 1 && !fork {
            items.push(QueueItem::Stack {
                entries: members
                    .iter()
                    .rev()
                    .map(|(index, _)| entries[*index].clone())
                    .collect(),
                base_branch: edge(entries[root].representative().pr_base_branch.as_deref())
                    .map(str::to_string),
            });
        } else {
            for (index, depth) in members {
                items.push(QueueItem::Pr {
                    entry: entries[index].clone(),
                    depth,
                });
            }
        }
    }
    items
}

#[derive(Debug, Clone)]
pub struct RunGroup<'a> {
    pub team_id: String,
    pub sessions: Vec<&'a CodingSession>,
}

/// A repo band: `index` = its position in the `pulls` input (callers read
/// the rest of the repo, e.g. its `full_name`, off it).
#[derive(Debug, Clone)]
pub struct RepoGroup<'a, P> {
    pub index: usize,
    pub team_id: String,
    pub repository_id: String,
    pub pulls: Vec<&'a P>,
}

#[derive(Debug, Clone)]
pub struct ReviewsQueue<'a, P> {
    pub board_groups: Vec<BoardGroup<'a>>,
    pub run_groups: Vec<RunGroup<'a>>,
    pub repo_groups: Vec<RepoGroup<'a, P>>,
    pub count: usize,
}

/// The queue. `teams` = DISPLAY order (desktop: the active team alone);
/// `pull_url` reads a pull's html url (the pull type lives in the caller's
/// API crate).
pub fn reviews_queue<'a, P>(
    teams: &[&str],
    boards: impl IntoIterator<Item = &'a Board>,
    issues: impl IntoIterator<Item = &'a Issue>,
    sessions: impl IntoIterator<Item = &'a CodingSession>,
    pulls: &[PullRepo<'a, P>],
    pull_url: impl Fn(&P) -> &str,
) -> ReviewsQueue<'a, P> {
    let team_order: HashMap<&str, usize> = teams
        .iter()
        .enumerate()
        .map(|(index, team)| (*team, index))
        .collect();
    // (1) Scope.
    let board_by_id: HashMap<&str, &'a Board> = boards
        .into_iter()
        .filter(|board| team_order.contains_key(board.team_id.as_str()))
        .map(|board| (board.id.as_str(), board))
        .collect();
    let issues: Vec<&'a Issue> = issues
        .into_iter()
        .filter(|issue| board_by_id.contains_key(issue.board_id.as_str()))
        .collect();
    let sessions: Vec<&'a CodingSession> = sessions
        .into_iter()
        .filter(|session| {
            session
                .team_id
                .as_deref()
                .is_some_and(|team| team_order.contains_key(team))
        })
        .collect();

    // (2) One entry per open PR.
    let mut by_key: HashMap<String, Vec<&'a Issue>> = HashMap::new();
    for issue in &issues {
        if issue.pr_state.as_deref() != Some(OPEN) {
            continue;
        }
        let key = match present(issue.pr_url.as_deref()) {
            Some(url) => url.to_string(),
            None => format!("issue:{}", issue.id),
        };
        by_key.entry(key).or_default().push(issue);
    }
    let mut entries: Vec<QueueEntry<'a>> = by_key
        .into_iter()
        .map(|(key, mut rows)| {
            rows.sort_by(|a, b| representative_order(a, b));
            QueueEntry { key, issues: rows }
        })
        .collect();
    // (3) Newest representative first, under the representative's board.
    entries.sort_by(|a, b| representative_order(a.representative(), b.representative()));
    let mut board_groups: Vec<BoardGroup<'a>> = Vec::new();
    let mut slot: HashMap<&str, usize> = HashMap::new();
    let entry_count = entries.len();
    for entry in entries {
        let board_id = entry.representative().board_id.as_str();
        let index = *slot.entry(board_id).or_insert_with(|| {
            board_groups.push(BoardGroup {
                board: board_by_id[board_id],
                entries: Vec::new(),
                items: Vec::new(),
            });
            board_groups.len() - 1
        });
        board_groups[index].entries.push(entry);
    }
    // (9) The band's entries as drawn.
    for group in &mut board_groups {
        group.items = queue_items(&group.entries);
    }
    // (4) Team order, sort_order (null last), name, id.
    board_groups.sort_by(|a, b| {
        let (a, b) = (a.board, b.board);
        team_order[a.team_id.as_str()]
            .cmp(&team_order[b.team_id.as_str()])
            .then_with(|| {
                a.sort_order
                    .unwrap_or(f64::INFINITY)
                    .total_cmp(&b.sort_order.unwrap_or(f64::INFINITY))
            })
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });

    // (5) Linked = any in-scope issue's or run's PR, whatever its state.
    let issue_urls: HashSet<&str> = issues
        .iter()
        .filter_map(|issue| present(issue.pr_url.as_deref()))
        .collect();
    let linked: HashSet<&str> = issue_urls
        .iter()
        .copied()
        .chain(
            sessions
                .iter()
                .filter_map(|session| present(session.pr_url.as_deref())),
        )
        .collect();

    // (6) A run's OWN PR: newest row per pr_url, banded per team.
    let mut run_by_url: HashMap<&str, &'a CodingSession> = HashMap::new();
    for session in &sessions {
        if session.issue_id.is_some() || session.pr_state.as_deref() != Some(OPEN) {
            continue;
        }
        let Some(url) = present(session.pr_url.as_deref()) else {
            continue;
        };
        if issue_urls.contains(url) {
            continue;
        }
        match run_by_url.get(url) {
            Some(current) if session_order(session, current) != Ordering::Less => {}
            _ => {
                run_by_url.insert(url, session);
            }
        }
    }
    let mut runs: Vec<&'a CodingSession> = run_by_url.into_values().collect();
    runs.sort_by(|a, b| session_order(a, b));
    let run_count = runs.len();
    let run_groups: Vec<RunGroup<'a>> = teams
        .iter()
        .map(|team| RunGroup {
            team_id: team.to_string(),
            sessions: runs
                .iter()
                .copied()
                .filter(|session| session.team_id.as_deref() == Some(*team))
                .collect(),
        })
        .filter(|group| !group.sessions.is_empty())
        .collect();

    // (7) The unlinked pulls, team order then fetch order.
    let mut scoped: Vec<(usize, &PullRepo<'a, P>)> = pulls
        .iter()
        .enumerate()
        .filter(|(_, repo)| team_order.contains_key(repo.team_id))
        .collect();
    scoped.sort_by(|(a_index, a), (b_index, b)| {
        team_order[a.team_id]
            .cmp(&team_order[b.team_id])
            .then(a_index.cmp(b_index))
    });
    let repo_groups: Vec<RepoGroup<'a, P>> = scoped
        .into_iter()
        .map(|(index, repo)| RepoGroup {
            index,
            team_id: repo.team_id.to_string(),
            repository_id: repo.repository_id.to_string(),
            pulls: repo
                .pulls
                .iter()
                .filter(|pull| !linked.contains(pull_url(pull)))
                .collect(),
        })
        .filter(|repo| !repo.pulls.is_empty())
        .collect();

    let count = entry_count
        + run_count
        + repo_groups.iter().map(|repo| repo.pulls.len()).sum::<usize>();
    ReviewsQueue {
        board_groups,
        run_groups,
        repo_groups,
        count,
    }
}

/// The Reviews nav entry ×4 (web `reviewsNav`, iOS/Android
/// `ReviewsQueue.nav`): `dot` = the queue holds anything; `shows` = no team
/// in scope, some team not in yolo mode, or the dot is lit (yolo mode merges
/// every agent PR at once, so an open PR there = a failed merge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewsNav {
    pub dot: bool,
    pub shows: bool,
}

/// `yolo` = each in-scope team's yolo mode; `count` = the queue's `count`.
pub fn reviews_nav(yolo: &[bool], count: usize) -> ReviewsNav {
    let dot = count > 0;
    ReviewsNav {
        dot,
        shows: yolo.is_empty() || yolo.iter().any(|yolo| !yolo) || dot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::{json, Value};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        labels: Labels,
        cases: Vec<Case>,
        nav_cases: Vec<NavCase>,
        grouping_cases: Vec<GroupingCase>,
    }
    #[derive(Deserialize)]
    struct GroupingCase {
        name: String,
        input: Input,
        expected: GroupingExpected,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    struct GroupingExpected {
        boards: Vec<GroupingBoard>,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct GroupingBoard {
        board_id: String,
        items: Vec<GroupingItem>,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(tag = "kind", rename_all = "camelCase")]
    enum GroupingItem {
        #[serde(rename = "pr")]
        Pr { key: String, depth: usize },
        #[serde(rename = "stack", rename_all = "camelCase")]
        Stack {
            keys: Vec<String>,
            base_branch: Option<String>,
        },
    }
    #[derive(Deserialize)]
    struct NavCase {
        name: String,
        input: NavInput,
        expected: NavExpected,
    }
    #[derive(Deserialize)]
    struct NavInput {
        yolo: Vec<bool>,
        count: usize,
    }
    #[derive(Deserialize)]
    struct NavExpected {
        dot: bool,
        shows: bool,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Labels {
        repo_band_caption: String,
        run_band_caption: String,
    }
    #[derive(Deserialize)]
    struct Case {
        name: String,
        input: Input,
        expected: Expected,
    }
    #[derive(Deserialize)]
    struct Input {
        teams: Vec<Team>,
        boards: Vec<Value>,
        issues: Vec<Value>,
        sessions: Vec<Value>,
        pulls: Vec<Repo>,
    }
    #[derive(Deserialize)]
    struct Team {
        id: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Repo {
        team_id: String,
        repository_id: String,
        pulls: Vec<Pull>,
    }
    #[derive(Deserialize)]
    struct Pull {
        number: u64,
        url: String,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Expected {
        board_groups: Vec<ExpectedBoard>,
        run_groups: Vec<ExpectedRuns>,
        repo_groups: Vec<ExpectedRepo>,
        count: usize,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedBoard {
        board_id: String,
        entries: Vec<ExpectedEntry>,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedEntry {
        key: String,
        issue_ids: Vec<String>,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedRuns {
        team_id: String,
        session_ids: Vec<String>,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedRepo {
        team_id: String,
        repository_id: String,
        pull_numbers: Vec<u64>,
    }

    fn board(row: &Value) -> Board {
        serde_json::from_value(json!({
            "id": row["id"],
            "team_id": row["teamId"],
            "name": row["name"],
            "slug": row["id"],
            "sort_order": row["sortOrder"],
        }))
        .unwrap()
    }

    fn issue(row: &Value) -> Issue {
        serde_json::from_value(json!({
            "id": row["id"],
            "board_id": row["boardId"],
            "number": 1,
            "identifier": row["id"],
            "title": row["id"],
            "status": "in_review",
            "created_at": row["createdAt"],
            "pr_url": row["prUrl"],
            "pr_state": row["prState"],
            "branch": row["branch"],
            "pr_base_branch": row["prBaseBranch"],
        }))
        .unwrap()
    }

    fn session(row: &Value) -> CodingSession {
        serde_json::from_value(json!({
            "id": row["id"],
            "team_id": row["teamId"],
            "issue_id": row["issueId"],
            "status": "in_review",
            "pr_url": row["prUrl"],
            "pr_state": row["prState"],
            "created_at": row["createdAt"],
        }))
        .unwrap()
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/reviews-queue.json"
        ))
        .expect("reviews-queue.json parses")
    }

    /// EXP-1244: `navCases` lock the Reviews nav entry's dot + visibility ×4.
    #[test]
    fn reviews_nav_matches_the_fixture() {
        let cases = fixture().nav_cases;
        assert!(!cases.is_empty());
        for case in cases {
            let nav = reviews_nav(&case.input.yolo, case.input.count);
            assert_eq!(
                nav,
                ReviewsNav {
                    dot: case.expected.dot,
                    shows: case.expected.shows,
                },
                "{}",
                case.name
            );
        }
    }

    /// EXP-1244: `reviews-queue.json` locks the queue ×4.
    #[test]
    fn reviews_queue_matches_the_fixture() {
        let fixture = fixture();
        assert_eq!(REPO_BAND_CAPTION, fixture.labels.repo_band_caption);
        assert_eq!(RUN_BAND_CAPTION, fixture.labels.run_band_caption);
        assert!(!fixture.cases.is_empty());

        for case in fixture.cases {
            let teams: Vec<&str> = case.input.teams.iter().map(|t| t.id.as_str()).collect();
            let boards: Vec<Board> = case.input.boards.iter().map(board).collect();
            let issues: Vec<Issue> = case.input.issues.iter().map(issue).collect();
            let sessions: Vec<CodingSession> = case.input.sessions.iter().map(session).collect();
            let pulls: Vec<PullRepo<'_, Pull>> = case
                .input
                .pulls
                .iter()
                .map(|repo| PullRepo {
                    team_id: &repo.team_id,
                    repository_id: &repo.repository_id,
                    pulls: &repo.pulls,
                })
                .collect();
            let queue = reviews_queue(&teams, &boards, &issues, &sessions, &pulls, |pull| {
                pull.url.as_str()
            });
            let actual = Expected {
                board_groups: queue
                    .board_groups
                    .iter()
                    .map(|group| ExpectedBoard {
                        board_id: group.board.id.clone(),
                        entries: group
                            .entries
                            .iter()
                            .map(|entry| ExpectedEntry {
                                key: entry.key.clone(),
                                issue_ids: entry.issues.iter().map(|i| i.id.clone()).collect(),
                            })
                            .collect(),
                    })
                    .collect(),
                run_groups: queue
                    .run_groups
                    .iter()
                    .map(|group| ExpectedRuns {
                        team_id: group.team_id.clone(),
                        session_ids: group.sessions.iter().map(|s| s.id.clone()).collect(),
                    })
                    .collect(),
                repo_groups: queue
                    .repo_groups
                    .iter()
                    .map(|repo| ExpectedRepo {
                        team_id: repo.team_id.clone(),
                        repository_id: repo.repository_id.clone(),
                        pull_numbers: repo.pulls.iter().map(|p| p.number).collect(),
                    })
                    .collect(),
                count: queue.count,
            };
            assert_eq!(actual, case.expected, "{}", case.name);
        }
    }

    /// EXP-1248: `groupingCases` lock rule 9 ×4 (web `queueItems`).
    #[test]
    fn queue_items_match_the_fixture() {
        let cases = fixture().grouping_cases;
        assert!(!cases.is_empty());
        for case in cases {
            let teams: Vec<&str> = case.input.teams.iter().map(|t| t.id.as_str()).collect();
            let boards: Vec<Board> = case.input.boards.iter().map(board).collect();
            let issues: Vec<Issue> = case.input.issues.iter().map(issue).collect();
            let sessions: Vec<CodingSession> = case.input.sessions.iter().map(session).collect();
            let pulls: Vec<PullRepo<'_, Pull>> = case
                .input
                .pulls
                .iter()
                .map(|repo| PullRepo {
                    team_id: &repo.team_id,
                    repository_id: &repo.repository_id,
                    pulls: &repo.pulls,
                })
                .collect();
            let queue = reviews_queue(&teams, &boards, &issues, &sessions, &pulls, |pull| {
                pull.url.as_str()
            });
            let actual = GroupingExpected {
                boards: queue
                    .board_groups
                    .iter()
                    .map(|group| GroupingBoard {
                        board_id: group.board.id.clone(),
                        items: group
                            .items
                            .iter()
                            .map(|item| match item {
                                QueueItem::Pr { entry, depth } => GroupingItem::Pr {
                                    key: entry.key.clone(),
                                    depth: *depth,
                                },
                                QueueItem::Stack {
                                    entries,
                                    base_branch,
                                } => GroupingItem::Stack {
                                    keys: entries.iter().map(|entry| entry.key.clone()).collect(),
                                    base_branch: base_branch.clone(),
                                },
                            })
                            .collect(),
                    })
                    .collect(),
            };
            assert_eq!(actual, case.expected, "{}", case.name);
        }
    }
}
