//! EXP-818: the session TREE — a run started by another run through
//! `exponential_sessions_start` carries `parent_session_id`, and every
//! session list (the rail, the Agent page, Devices on the phones) nests it
//! under its parent instead of listing it as a stranger.
//!
//! ONE pure rule, mirrored ×4 (web `lib/session-tree.ts`, iOS
//! `SessionTree.swift`, Android `SessionTree.kt`) with the same four tests:
//!
//! 1. The caller's order is the ROOT order — the tree never re-sorts roots.
//! 2. A row is a child iff its parent names ANOTHER row of the input; a
//!    parent that is not listed leaves the child a root at depth 0.
//! 3. Children follow their parent directly, oldest start first (then id),
//!    recursively — depth grows by one per level.
//! 4. A cycle (defensive) breaks at the first repeat.
//!
//! EXP-996/EXP-1049 layer the TREE proper on top ([`session_tree`], the web
//! `lib/sessions/session-tree.ts` twin, same rules and same test names):
//! resume successions COLLAPSE into one node, the runs of one workflow and of
//! one PR stack fold under GROUP rows, and [`visible_session_tree_rows`] is
//! the flattening every client paints its connector over. EXP-1068 groups a
//! workflow's runs by the server-stamped membership ONLY (`workflow_id` /
//! `workflow_node_id` / `workflow_role`, EXP-1082), nests reviews under their
//! node's author and flags duplicate live runs; the strings drawn off it
//! ([`workflow_group_caption`], [`review_row_caption`]) are byte-identical ×4.
//! `nest_sessions` stays exactly as it was — the Automations log and
//! [`crate::pr_graph`] still nest plain parent/child lists through it.

use std::collections::{HashMap, HashSet};

/// One row of the flattened tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow<T> {
    pub session: T,
    /// 0 for a root, +1 per nesting level.
    pub depth: usize,
    /// Whether at least one child is nested right below.
    pub has_children: bool,
}

/// Flatten `sessions` into nested order. `id`/`parent`/`started_at` read the
/// three columns off whatever row type the caller holds (ISO-8601 UTC
/// strings compare lexicographically in time order).
pub fn nest_sessions<T, I, P, S>(sessions: Vec<T>, id: I, parent: P, started_at: S) -> Vec<TreeRow<T>>
where
    I: Fn(&T) -> &str,
    P: Fn(&T) -> Option<&str>,
    S: Fn(&T) -> Option<&str>,
{
    let ids: HashSet<String> = sessions.iter().map(|s| id(s).to_string()).collect();
    let is_child = |s: &T| -> bool {
        parent(s).is_some_and(|p| p != id(s) && ids.contains(p))
    };
    let mut children_of: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, session) in sessions.iter().enumerate() {
        if is_child(session) {
            let key = parent(session).unwrap_or_default().to_string();
            children_of.entry(key).or_default().push(index);
        }
    }
    for list in children_of.values_mut() {
        list.sort_by(|a, b| {
            let (sa, sb) = (&sessions[*a], &sessions[*b]);
            started_at(sa)
                .unwrap_or_default()
                .cmp(started_at(sb).unwrap_or_default())
                .then_with(|| id(sa).cmp(id(sb)))
        });
    }
    let mut order: Vec<(usize, usize, bool)> = Vec::with_capacity(sessions.len());
    let mut placed = vec![false; sessions.len()];
    fn visit(
        index: usize,
        depth: usize,
        sessions_ids: &[String],
        children_of: &HashMap<String, Vec<usize>>,
        placed: &mut [bool],
        order: &mut Vec<(usize, usize, bool)>,
    ) {
        if placed[index] {
            return;
        }
        placed[index] = true;
        let children: Vec<usize> = children_of
            .get(&sessions_ids[index])
            .map(|c| c.iter().copied().filter(|child| !placed[*child]).collect())
            .unwrap_or_default();
        order.push((index, depth, !children.is_empty()));
        for child in children {
            visit(child, depth + 1, sessions_ids, children_of, placed, order);
        }
    }
    let session_ids: Vec<String> = sessions.iter().map(|s| id(s).to_string()).collect();
    for (index, session) in sessions.iter().enumerate() {
        if !is_child(session) {
            visit(index, 0, &session_ids, &children_of, &mut placed, &mut order);
        }
    }
    // A child whose ancestry cycled without a root — keep it, at depth 0.
    for index in 0..sessions.len() {
        visit(index, 0, &session_ids, &children_of, &mut placed, &mut order);
    }
    let mut slots: Vec<Option<T>> = sessions.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|(index, depth, has_children)| TreeRow {
            session: slots[index].take().expect("each index placed once"),
            depth,
            has_children,
        })
        .collect()
}

/// The ids of every row nested (at any depth) under `id`.
pub fn descendant_ids<T, I>(rows: &[TreeRow<T>], id: &str, row_id: I) -> Vec<String>
where
    I: Fn(&T) -> &str,
{
    let Some(start) = rows.iter().position(|row| row_id(&row.session) == id) else {
        return Vec::new();
    };
    let depth = rows[start].depth;
    rows[start + 1..]
        .iter()
        .take_while(|row| row.depth > depth)
        .map(|row| row_id(&row.session).to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// EXP-996 — the session TREE proper (web `lib/sessions/session-tree.ts`)
// ---------------------------------------------------------------------------

/// EXP-996 — a workflow group row's label while its `workflows` row has not
/// synced a name (the tab title's own fallback, `ui/src/navigation.rs`).
pub const WORKFLOW_GROUP_FALLBACK_NAME: &str = "Workflow";

/// EXP-996 — the STACK group row's label. A workflow group row needs none (it
/// wears its workflow's name); a stack has no synced row to take a name from,
/// so this is the one NEW string of the tree. It carries no COUNT: every
/// client draws a group's child count in its own trailing cell, so a number in
/// the label would say it twice. Byte-identical ×4 (web `STACK_GROUP_LABEL`,
/// iOS `stackGroupLabel`, Android `STACK_GROUP_LABEL`); it lives here beside
/// [`crate::pr_stack`]'s stack copy.
pub const STACK_GROUP_LABEL: &str = "Stacked pull requests";

/// EXP-996 — a GROUP row's fold labels. [`crate::pr_stack`]'s own pair says
/// CHILD RUNS, which a group has none of: its rows are siblings that belong to
/// one workflow or one stack. Byte-identical ×4 (web `COLLAPSE_GROUP_LABEL`,
/// iOS `collapseGroupLabel`, Android `COLLAPSE_GROUP_LABEL`).
pub const COLLAPSE_GROUP_LABEL: &str = "Collapse these runs";
pub const EXPAND_GROUP_LABEL: &str = "Expand these runs";

/// EXP-996 — the row fields the tree reads, lifted off whatever row type the
/// caller holds. ONE mapper, not [`nest_sessions`]' accessor closures: a batch
/// row's covered ids are PARSED out of jsonb
/// ([`crate::batch_run::parse_batch_issue_ids`]), so they cannot be borrowed.
/// `started_reason` is deliberately ABSENT — `workflow` alone never groups a
/// run (EXP-1068: only the stamped `workflow_id` does).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionFacts {
    pub id: String,
    pub resumed_from_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub issue_id: Option<String>,
    /// The issues a BATCH run covers (EXP-876).
    pub batch_issue_ids: Vec<String>,
    /// ISO-8601 UTC, compared lexicographically (this file's convention);
    /// `""` = unknown, which sorts OLDEST — the web's `stamp` of 0.
    pub created_at: String,
    pub updated_at: String,
    /// The raw `status` word; anything but `ended` (incl. `""`) is LIVE
    /// ([`session_row_is_live`]).
    pub status: String,
    /// The run's branch — a review run's round rides its suffix
    /// ([`review_branch_round`]).
    pub branch: Option<String>,
    /// EXP-1082: the server-stamped workflow membership
    /// (`coding_sessions.workflow_id` / `workflow_node_id` /
    /// `workflow_role`) — the ONLY thing that groups a run (EXP-1068).
    pub workflow_id: Option<String>,
    pub workflow_node_id: Option<String>,
    pub workflow_role: Option<String>,
}

/// The facts of a synced `coding_sessions` row — every desktop caller's
/// mapper, so no two surfaces can read the columns differently.
pub fn coding_session_facts(session: &crate::rows::CodingSession) -> SessionFacts {
    SessionFacts {
        id: session.id.clone(),
        resumed_from_id: session.resumed_from_id.clone(),
        parent_session_id: session.parent_session_id.clone(),
        issue_id: session.issue_id.clone(),
        batch_issue_ids: crate::batch_run::parse_batch_issue_ids(session.batch_issue_ids.as_ref()),
        created_at: session.created_at.clone().unwrap_or_default(),
        updated_at: session.updated_at.clone().unwrap_or_default(),
        status: session.status.clone().unwrap_or_default(),
        branch: session.branch.clone(),
        workflow_id: session.workflow_id.clone(),
        workflow_node_id: session.workflow_node_id.clone(),
        workflow_role: session.workflow_role.clone(),
    }
}

/// EXP-996 — the group row's side of one `workflows` row. Being LISTED is what
/// makes a workflow groupable, and the NAME is what its group row wears — the
/// fat synced row is projected down to this because the rail re-derives its
/// tree on every paint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowFacts {
    pub id: String,
    pub name: String,
    /// contract `wfStatus`, raw (`""` when the row carries none) — the group
    /// row's glyph.
    pub status: String,
}

impl WorkflowFacts {
    /// A synced row's facts — the name it carries, else the generic word while
    /// the row has not landed one.
    pub fn from_row(row: &crate::rows::WorkflowRow) -> Self {
        Self {
            id: row.id.clone(),
            name: row
                .name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(WORKFLOW_GROUP_FALLBACK_NAME)
                .to_string(),
            status: row.status.clone().unwrap_or_default(),
        }
    }
}

/// EXP-996 — one `workflow_nodes` row. Since EXP-1068 it groups NOTHING (the
/// session rows carry their membership); `state` (contract `wfNodeState`)
/// feeds the group caption's `5 of 8 done`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowNodeFacts {
    pub id: Option<String>,
    pub workflow_id: String,
    pub issue_id: Option<String>,
    pub session_id: Option<String>,
    pub state: Option<String>,
}

impl WorkflowNodeFacts {
    /// A synced row's facts — `None` on a row that names no workflow.
    pub fn from_row(row: &crate::rows::WorkflowNodeRow) -> Option<Self> {
        Some(Self {
            id: Some(row.id.clone()),
            workflow_id: row.workflow_id.clone()?,
            issue_id: row.issue_id.clone(),
            session_id: row.session_id.clone(),
            state: row.state.clone(),
        })
    }
}

/// EXP-996 — what the rows alone cannot say: which workflow is called what,
/// how its nodes stand, and which issues stack on which. Every slice may be
/// empty — a caller with no workflows synced still gets the session/parent
/// tree.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionTreeContext<'a> {
    pub workflows: &'a [WorkflowFacts],
    pub workflow_nodes: &'a [WorkflowNodeFacts],
    /// The issues the sessions name, for the stack edges
    /// ([`crate::pr_stack::stack_chain`]). Callers scope them the way Reviews
    /// does — the OPEN pull requests, one team.
    pub issues: &'a [crate::rows::Issue],
}

/// One node of the tree: a RUN (its resume succession collapsed into one row),
/// or the group row a workflow / stack of runs folds under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTreeNode<T> {
    Session(SessionNode<T>),
    Workflow(WorkflowGroupNode<T>),
    Stack(StackGroupNode<T>),
}

/// ONE run — every row of its resume succession (EXP-974) folded together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionNode<T> {
    /// The NEWEST row of the succession: the node's identity and its key.
    pub id: String,
    /// The succession oldest-first (`run_chain`'s order), the newest last; one
    /// entry when the run was never resumed.
    pub chain: Vec<T>,
    pub children: Vec<SessionTreeNode<T>>,
    /// The newest `updated_at` across the chain AND the whole subtree, so
    /// folding a parent never moves it.
    pub last_activity_at: String,
    /// EXP-1068: a REVIEW chain's round, off its branch
    /// ([`review_branch_round`]); `None` on every other row.
    pub review_round: Option<i64>,
    /// EXP-1068: this AUTHOR row's node has two live author chains or two
    /// live review chains — the warning glyph. Never set on a review row.
    pub duplicate_live: bool,
}

impl<T> SessionNode<T> {
    /// The row the node DRAWS — the newest of its resume succession.
    pub fn session(&self) -> &T {
        self.chain.last().expect("a chain always holds its own row")
    }
}

/// Rule 3 — the runs of ONE workflow (EXP-978), newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGroupNode<T> {
    pub workflow_id: String,
    /// The synced name, else [`WORKFLOW_GROUP_FALLBACK_NAME`].
    pub name: String,
    /// contract `wfStatus` — the group row's glyph.
    pub status: String,
    /// Live session nodes in the group's whole subtree.
    pub live_runs: usize,
    /// `workflow_nodes` of this workflow in state `landed`, of `nodes_total`
    /// ([`workflow_group_caption`]).
    pub nodes_done: usize,
    pub nodes_total: usize,
    pub children: Vec<SessionTreeNode<T>>,
    pub last_activity_at: String,
}

/// Rule 4 — one PR stack (EXP-897), LINEAR: lowest member first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackGroupNode<T> {
    /// The lowest issue of the chain — the group's identity and its key.
    pub root_issue_id: String,
    pub children: Vec<SessionTreeNode<T>>,
    pub last_activity_at: String,
}

impl<T> SessionTreeNode<T> {
    pub fn children(&self) -> &[SessionTreeNode<T>] {
        match self {
            SessionTreeNode::Session(node) => &node.children,
            SessionTreeNode::Workflow(node) => &node.children,
            SessionTreeNode::Stack(node) => &node.children,
        }
    }

    /// A group row IS its children: with none left it is not drawn
    /// ([`visible_session_tree_rows`]).
    pub fn is_group(&self) -> bool {
        !matches!(self, SessionTreeNode::Session(_))
    }

    pub fn last_activity_at(&self) -> &str {
        match self {
            SessionTreeNode::Session(node) => &node.last_activity_at,
            SessionTreeNode::Workflow(node) => &node.last_activity_at,
            SessionTreeNode::Stack(node) => &node.last_activity_at,
        }
    }

    /// The run a SESSION node draws, `None` on a group row.
    pub fn session(&self) -> Option<&T> {
        match self {
            SessionTreeNode::Session(node) => Some(node.session()),
            _ => None,
        }
    }
}

/// A node's stable identity — the key a collapsed set and a list row use
/// (web `sessionTreeNodeKey`).
pub fn session_tree_node_key<T>(node: &SessionTreeNode<T>) -> String {
    match node {
        SessionTreeNode::Session(node) => node.id.clone(),
        SessionTreeNode::Workflow(node) => format!("workflow:{}", node.workflow_id),
        SessionTreeNode::Stack(node) => format!("stack:{}", node.root_issue_id),
    }
}

/// A row is LIVE until the server ends it (`running` and `in_review` both
/// are; `needs_input`/`blocked` are flags on a live row).
pub fn session_row_is_live(status: &str) -> bool {
    status != "ended"
}

/// EXP-1068: the round a review branch carries — `exp/wf-<id8>-review-
/// <IDENT>-r<n>` → n (> 0). `None` for any other branch. Only the SUFFIX is
/// read, so the four clients cannot drift on the prefix.
pub fn review_branch_round(branch: Option<&str>) -> Option<i64> {
    let branch = branch?;
    let digits = &branch[branch.rfind("-r")? + 2..];
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<i64>().ok().filter(|round| *round > 0)
}

/// What a review row says about its verdict. `Submitted` = an older round
/// whose verdict the node row no longer carries (only the latest is stored).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewRowVerdict {
    Approved,
    ChangesRequested,
    Submitted,
    None,
}

/// EXP-1068: the verdict of the review of `round`, from the node's
/// `(review_round, latest review (round, verdict))` — `verdict` a contract
/// `wfReviewVerdict` word (`approve` | `request_changes`).
pub fn review_round_verdict(
    round: Option<i64>,
    node: Option<(i64, Option<(i64, &str)>)>,
) -> ReviewRowVerdict {
    let (Some(round), Some((review_round, latest))) = (round, node) else {
        return ReviewRowVerdict::None;
    };
    if let Some((latest_round, verdict)) = latest {
        if latest_round == round {
            return if verdict == "approve" {
                ReviewRowVerdict::Approved
            } else {
                ReviewRowVerdict::ChangesRequested
            };
        }
    }
    if round <= review_round {
        ReviewRowVerdict::Submitted
    } else {
        ReviewRowVerdict::None
    }
}

/// EXP-1068: a review row's title — `Review r2 · approved`, `Review r2 ·
/// changes requested`, `Review r2 · submitted`, `Review r2 · no verdict`
/// (ended, nothing submitted), `Review r2` (still reviewing), `Review` (no
/// round known). Byte-identical ×4.
pub fn review_row_caption(round: Option<i64>, verdict: ReviewRowVerdict, live: bool) -> String {
    let title = match round {
        Some(round) => format!("Review r{round}"),
        None => "Review".to_string(),
    };
    match verdict {
        ReviewRowVerdict::Approved => format!("{title} · approved"),
        ReviewRowVerdict::ChangesRequested => format!("{title} · changes requested"),
        ReviewRowVerdict::Submitted => format!("{title} · submitted"),
        ReviewRowVerdict::None if live => title,
        ReviewRowVerdict::None => format!("{title} · no verdict"),
    }
}

/// EXP-1068: a workflow group row's trailing caption — `3 running · 5 of 8
/// done`; `5 of 8 done` with nothing live; `3 running` before the nodes
/// synced; empty with neither. Byte-identical ×4.
pub fn workflow_group_caption(live_runs: usize, nodes_done: usize, nodes_total: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    if live_runs > 0 {
        parts.push(format!("{live_runs} running"));
    }
    if nodes_total > 0 {
        parts.push(format!("{nodes_done} of {nodes_total} done"));
    }
    parts.join(" · ")
}

/// A chain's workflow membership: its NEWEST stamped row's.
#[derive(Debug, Clone)]
struct Membership {
    workflow_id: String,
    node_id: Option<String>,
    role: Option<String>,
}

fn membership_of(chain: &[usize], facts: &[SessionFacts]) -> Option<Membership> {
    chain.iter().rev().find_map(|member| {
        let row = &facts[*member];
        row.workflow_id.as_ref().map(|workflow_id| Membership {
            workflow_id: workflow_id.clone(),
            node_id: row.workflow_node_id.clone(),
            role: row.workflow_role.clone(),
        })
    })
}

/// EXP-996 — the sessions list as a TREE, the web `sessionTree`'s twin: the
/// ONE selector every session list draws from (the rail's Running section, the
/// IDE's Recent list, the phones' session screens). Rules, in this order:
///
/// 1. Resume successions COLLAPSE into one node keyed by their newest row.
/// 2. Children nest under their `parent_session_id`, following the parent's
///    whole succession (EXP-906) — UNLESS the child chain has a workflow
///    membership the parent's chain does not share (EXP-1068).
/// 3. Runs group under a workflow node by their chain's membership (its
///    newest stamped row's `workflow_id`), only when that workflow is LISTED
///    (the name source). Inside: one row per AUTHOR chain; a node's REVIEW
///    chains nest under its head author (live first, then newest activity),
///    else sit as plain group children like every other stamped row; a node
///    with ≥2 live authors or ≥2 live reviewers flags `duplicate_live` on
///    each author row. The group carries the status and caption counts.
/// 4. A PR stack groups under a stack node, LINEAR, lowest first — a stack
///    with fewer than two listed runs is no group.
/// 5. Groups and top-level nodes sort by last activity, newest first;
///    children keep creation order (a group's: newest activity first).
/// 6. An orphan child (parent swept, another team, not synced) sits at top
///    level; so does a row naming itself, and a cycle breaks where it closes.
///
/// Pure: no clock, no IO; every tie breaks on the node key, so two clients
/// agree row for row.
pub fn session_tree<T, F>(
    sessions: Vec<T>,
    facts: F,
    context: &SessionTreeContext<'_>,
) -> Vec<SessionTreeNode<T>>
where
    F: Fn(&T) -> SessionFacts,
{
    let facts: Vec<SessionFacts> = sessions.iter().map(&facts).collect();
    let index_of: HashMap<&str, usize> = facts
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id.as_str(), index))
        .collect();

    // 1. Resume successions collapse. OLDEST row first, so the primary
    //    succession (the newest-successor walk) claims its members before an
    //    older fork sibling does; whatever is left becomes its own node.
    let mut ordered: Vec<usize> = (0..facts.len()).collect();
    ordered.sort_by(|a, b| {
        facts[*a]
            .created_at
            .cmp(&facts[*b].created_at)
            .then_with(|| facts[*a].id.cmp(&facts[*b].id))
    });
    let mut canonical_of: HashMap<usize, usize> = HashMap::new();
    // (canonical, chain oldest-first), in discovery order.
    let mut chains: Vec<(usize, Vec<usize>)> = Vec::new();
    for &start in &ordered {
        if canonical_of.contains_key(&start) {
            continue;
        }
        let mut chain: Vec<usize> = resume_chain(&facts, &index_of, start)
            .into_iter()
            .filter(|member| !canonical_of.contains_key(member))
            .collect();
        if chain.is_empty() {
            chain.push(start);
        }
        let canonical = *chain.last().expect("non-empty");
        for &member in &chain {
            canonical_of.insert(member, canonical);
        }
        chains.push((canonical, chain));
    }

    let memberships: HashMap<usize, Membership> = chains
        .iter()
        .filter_map(|(canonical, chain)| {
            membership_of(chain, &facts).map(|membership| (*canonical, membership))
        })
        .collect();

    // 2. Children nest under their parent's SUCCESSION — the whole chain
    //    answers for the newest `parent_session_id` it names — unless the
    //    child is a workflow's the parent is not (the group claims it).
    let mut parent_of: HashMap<usize, usize> = HashMap::new();
    for (canonical, chain) in &chains {
        let named = chain
            .iter()
            .rev()
            .find_map(|member| facts[*member].parent_session_id.as_deref());
        let parent = named
            .and_then(|id| index_of.get(id))
            .and_then(|index| canonical_of.get(index))
            .copied();
        // Rule 6: a parent that is gone leaves the child at top level, and so
        // does a row naming its own succession.
        let Some(parent) = parent.filter(|parent| parent != canonical) else {
            continue;
        };
        if let Some(own) = memberships.get(canonical) {
            let parents = memberships.get(&parent).map(|m| m.workflow_id.as_str());
            if parents != Some(own.workflow_id.as_str()) {
                continue;
            }
        }
        parent_of.insert(*canonical, parent);
    }

    let mut children_of: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (canonical, _) in &chains {
        match ancestor(*canonical, &parent_of) {
            Some(parent) => children_of.entry(parent).or_default().push(*canonical),
            None => roots.push(*canonical),
        }
    }
    // Rule 5: children keep CREATION order, ties on their id.
    for list in children_of.values_mut() {
        list.sort_by(|a, b| {
            facts[*a]
                .created_at
                .cmp(&facts[*b].created_at)
                .then_with(|| facts[*a].id.cmp(&facts[*b].id))
        });
    }

    let chain_of: HashMap<usize, Vec<usize>> = chains.into_iter().collect();
    let mut slots: Vec<Option<T>> = sessions.into_iter().map(Some).collect();
    let built: Vec<(usize, SessionNode<T>)> = roots
        .iter()
        .map(|canonical| {
            (
                *canonical,
                build_node(*canonical, &chain_of, &children_of, &facts, &memberships, &mut slots),
            )
        })
        .collect();

    // 3./4. Workflow groups, then stack groups — over the TOP-LEVEL nodes
    //       only (a child run stays under its parent wherever that lands).
    let grouped = group_stacks(
        group_workflows(built, &memberships, &facts, context),
        &facts,
        context.issues,
    );

    // 5. Groups and lone nodes sort by last activity, newest FIRST.
    let mut out = grouped;
    out.sort_by(|a, b| {
        b.last_activity_at()
            .cmp(a.last_activity_at())
            .then_with(|| session_tree_node_key(a).cmp(&session_tree_node_key(b)))
    });
    out
}

/// The resume succession `start` belongs to, oldest first — `queries::run_chain`
/// / the web `runChain`, over indices: BACKWARDS through `resumed_from_id` to
/// the first row, FORWARDS to the newest successor at every fork (`created_at`
/// then id, descending). Cycle-safe both ways. It lives here rather than in
/// the ui crate's copy because the tree must collapse successions on every
/// client, not just the one with a query layer.
fn resume_chain(
    facts: &[SessionFacts],
    index_of: &HashMap<&str, usize>,
    start: usize,
) -> Vec<usize> {
    let mut seen: HashSet<usize> = HashSet::new();
    seen.insert(start);
    let mut before: Vec<usize> = Vec::new();
    let mut cursor = start;
    while let Some(previous_id) = facts[cursor].resumed_from_id.as_deref() {
        let Some(&previous) = index_of.get(previous_id) else {
            break;
        };
        if !seen.insert(previous) {
            break;
        }
        before.push(previous);
        cursor = previous;
    }
    before.reverse();

    let mut after: Vec<usize> = Vec::new();
    let mut cursor = start;
    loop {
        let next = (0..facts.len())
            .filter(|index| {
                facts[*index].resumed_from_id.as_deref() == Some(facts[cursor].id.as_str())
            })
            .filter(|index| !seen.contains(index))
            .max_by(|a, b| {
                facts[*a]
                    .created_at
                    .cmp(&facts[*b].created_at)
                    .then_with(|| facts[*a].id.cmp(&facts[*b].id))
            });
        let Some(next) = next else {
            break;
        };
        seen.insert(next);
        after.push(next);
        cursor = next;
    }

    before
        .into_iter()
        .chain(std::iter::once(start))
        .chain(after)
        .collect()
}

/// The top of `index`' parent walk, or `None` when it is already a root.
/// Breaks a cycle by returning `None`, so the row stays where it is.
fn ancestor(index: usize, parent_of: &HashMap<usize, usize>) -> Option<usize> {
    let parent = *parent_of.get(&index)?;
    let mut seen: HashSet<usize> = HashSet::new();
    seen.insert(index);
    let mut cursor = Some(parent);
    while let Some(current) = cursor {
        if !seen.insert(current) {
            return None;
        }
        cursor = parent_of.get(&current).copied();
    }
    Some(parent)
}

/// One session node and its subtree, rows moved out of `slots`. A node's
/// activity counts its whole subtree's (rule 5).
fn build_node<T>(
    canonical: usize,
    chain_of: &HashMap<usize, Vec<usize>>,
    children_of: &HashMap<usize, Vec<usize>>,
    facts: &[SessionFacts],
    memberships: &HashMap<usize, Membership>,
    slots: &mut Vec<Option<T>>,
) -> SessionNode<T> {
    let chain = chain_of.get(&canonical).cloned().unwrap_or_default();
    let mut last_activity_at = chain
        .iter()
        .map(|member| facts[*member].updated_at.clone())
        .max()
        .unwrap_or_default();
    let children: Vec<SessionTreeNode<T>> = children_of
        .get(&canonical)
        .map(|kids| {
            kids.iter()
                .map(|kid| {
                    SessionTreeNode::Session(build_node(
                        *kid,
                        chain_of,
                        children_of,
                        facts,
                        memberships,
                        slots,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    for child in &children {
        if child.last_activity_at() > last_activity_at.as_str() {
            last_activity_at = child.last_activity_at().to_string();
        }
    }
    SessionNode {
        id: facts[canonical].id.clone(),
        chain: chain
            .iter()
            .map(|member| slots[*member].take().expect("each row placed once"))
            .collect(),
        children,
        last_activity_at,
        review_round: memberships
            .get(&canonical)
            .filter(|membership| membership.role.as_deref() == Some("review"))
            .and_then(|_| review_branch_round(facts[canonical].branch.as_deref())),
        duplicate_live: false,
    }
}

/// Rule 3 — the runs of ONE workflow under one group row, by their chain's
/// own membership; a workflow the caller did not list leaves its runs
/// ungrouped (the group's NAME lives on that row). The `Option<usize>` a bare
/// run keeps is its facts index, which the stack pass reads its issue out of.
fn group_workflows<T>(
    roots: Vec<(usize, SessionNode<T>)>,
    memberships: &HashMap<usize, Membership>,
    facts: &[SessionFacts],
    context: &SessionTreeContext<'_>,
) -> Vec<(Option<usize>, SessionTreeNode<T>)> {
    let listed: HashMap<&str, &WorkflowFacts> = context
        .workflows
        .iter()
        .map(|workflow| (workflow.id.as_str(), workflow))
        .collect();
    if listed.is_empty() {
        return roots
            .into_iter()
            .map(|(index, node)| (Some(index), SessionTreeNode::Session(node)))
            .collect();
    }
    let by_id: HashMap<&str, &SessionFacts> =
        facts.iter().map(|row| (row.id.as_str(), row)).collect();
    let live = |node: &SessionNode<T>| {
        by_id
            .get(node.id.as_str())
            .is_some_and(|row| session_row_is_live(&row.status))
    };
    let created = |node: &SessionTreeNode<T>| -> String {
        match node {
            SessionTreeNode::Session(session) => by_id
                .get(session.id.as_str())
                .map(|row| row.created_at.clone())
                .unwrap_or_default(),
            _ => String::new(),
        }
    };

    /// One group's pieces: author and review chains by node id, the rest.
    struct Parts<T> {
        position: usize,
        authors: HashMap<String, Vec<SessionNode<T>>>,
        reviews: HashMap<String, Vec<SessionNode<T>>>,
        plain: Vec<SessionNode<T>>,
    }
    let mut out: Vec<(Option<usize>, SessionTreeNode<T>)> = Vec::new();
    let mut parts: Vec<Parts<T>> = Vec::new();
    let mut part_of: HashMap<String, usize> = HashMap::new();
    for (index, node) in roots {
        let Some((membership, workflow)) = memberships.get(&index).and_then(|membership| {
            listed
                .get(membership.workflow_id.as_str())
                .map(|workflow| (membership, *workflow))
        }) else {
            out.push((Some(index), SessionTreeNode::Session(node)));
            continue;
        };
        let at = match part_of.get(&workflow.id) {
            Some(at) => *at,
            None => {
                let nodes_of: Vec<&WorkflowNodeFacts> = context
                    .workflow_nodes
                    .iter()
                    .filter(|entry| entry.workflow_id == workflow.id)
                    .collect();
                out.push((
                    None,
                    SessionTreeNode::Workflow(WorkflowGroupNode {
                        workflow_id: workflow.id.clone(),
                        name: workflow.name.clone(),
                        status: workflow.status.clone(),
                        live_runs: 0,
                        nodes_done: nodes_of
                            .iter()
                            .filter(|entry| entry.state.as_deref() == Some("landed"))
                            .count(),
                        nodes_total: nodes_of.len(),
                        children: Vec::new(),
                        last_activity_at: String::new(),
                    }),
                ));
                parts.push(Parts {
                    position: out.len() - 1,
                    authors: HashMap::new(),
                    reviews: HashMap::new(),
                    plain: Vec::new(),
                });
                part_of.insert(workflow.id.clone(), parts.len() - 1);
                parts.len() - 1
            }
        };
        let part = &mut parts[at];
        match (membership.node_id.as_deref(), membership.role.as_deref()) {
            (Some(node_id), Some("author")) => {
                part.authors.entry(node_id.to_string()).or_default().push(node)
            }
            (Some(node_id), Some("review")) => {
                part.reviews.entry(node_id.to_string()).or_default().push(node)
            }
            _ => part.plain.push(node),
        }
    }

    for part in parts {
        let Parts { position, authors, mut reviews, plain } = part;
        let mut children: Vec<SessionTreeNode<T>> = Vec::new();
        for (node_id, mut authors) in authors {
            // The node's HEAD author: live first, then newest activity.
            authors.sort_by(|a, b| {
                live(b)
                    .cmp(&live(a))
                    .then_with(|| b.last_activity_at.cmp(&a.last_activity_at))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let node_reviews = reviews.remove(&node_id).unwrap_or_default();
            let live_authors = authors.iter().filter(|node| live(node)).count();
            let live_reviews = node_reviews.iter().filter(|node| live(node)).count();
            let duplicate = live_authors > 1 || live_reviews > 1;
            for author in authors.iter_mut() {
                author.duplicate_live = duplicate;
            }
            if !node_reviews.is_empty() {
                let head = &mut authors[0];
                head.children
                    .extend(node_reviews.into_iter().map(SessionTreeNode::Session));
                head.children.sort_by(|a, b| {
                    created(a)
                        .cmp(&created(b))
                        .then_with(|| session_tree_node_key(a).cmp(&session_tree_node_key(b)))
                });
                if let Some(newest) = head
                    .children
                    .iter()
                    .map(|child| child.last_activity_at().to_string())
                    .max()
                {
                    if newest > head.last_activity_at {
                        head.last_activity_at = newest;
                    }
                }
            }
            children.extend(authors.into_iter().map(SessionTreeNode::Session));
        }
        // A review whose node has no author listed is a plain child.
        for (_, orphans) in reviews {
            children.extend(orphans.into_iter().map(SessionTreeNode::Session));
        }
        children.extend(plain.into_iter().map(SessionTreeNode::Session));
        sort_by_activity(&mut children);
        let live_runs = live_count(&children, &live);
        if let (_, SessionTreeNode::Workflow(group)) = &mut out[position] {
            group.last_activity_at = children
                .iter()
                .map(|child| child.last_activity_at().to_string())
                .max()
                .unwrap_or_default();
            group.live_runs = live_runs;
            group.children = children;
        }
    }
    out
}

/// How many session nodes of a subtree are live.
fn live_count<T>(nodes: &[SessionTreeNode<T>], live: &dyn Fn(&SessionNode<T>) -> bool) -> usize {
    nodes
        .iter()
        .map(|node| {
            let own = matches!(node, SessionTreeNode::Session(session) if live(session));
            usize::from(own) + live_count(node.children(), live)
        })
        .sum()
}

/// Rule 4 — a stack under one group row, LINEAR, lowest first. A stack with
/// only ONE of its runs listed is no group: the lone node stays where it was.
fn group_stacks<T>(
    entries: Vec<(Option<usize>, SessionTreeNode<T>)>,
    facts: &[SessionFacts],
    issues: &[crate::rows::Issue],
) -> Vec<SessionTreeNode<T>> {
    if issues.is_empty() {
        return entries.into_iter().map(|(_, node)| node).collect();
    }
    let mut slots: Vec<Option<(Option<usize>, SessionTreeNode<T>)>> =
        entries.into_iter().map(Some).collect();
    // Every top-level SESSION node that names a listed issue, by issue id.
    let mut position_of_issue: HashMap<&str, usize> = HashMap::new();
    for (position, slot) in slots.iter().enumerate() {
        let Some((Some(index), SessionTreeNode::Session(_))) = slot else {
            continue;
        };
        let Some(issue_id) = facts[*index].issue_id.as_deref() else {
            continue;
        };
        if issues.iter().any(|issue| issue.id == issue_id) {
            position_of_issue.entry(issue_id).or_insert(position);
        }
    }

    let mut out: Vec<SessionTreeNode<T>> = Vec::new();
    for position in 0..slots.len() {
        // Claimed by a stack already grouped above. The index is COPIED out:
        // the take below moves rows out of `slots`.
        let index = match slots[position].as_ref() {
            Some((index, _)) => *index,
            None => continue,
        };
        let take = |slots: &mut Vec<Option<(Option<usize>, SessionTreeNode<T>)>>, at: usize| {
            slots[at].take().expect("claimed once").1
        };
        // A group row passes through, and so does a run that names no issue (a
        // chat, an action, a batch): it can be in no stack.
        let Some(issue) = index
            .and_then(|index| facts[index].issue_id.as_deref())
            .and_then(|id| issues.iter().find(|issue| issue.id == id))
        else {
            out.push(take(&mut slots, position));
            continue;
        };
        let chain = crate::pr_stack::stack_chain(issue, issues);
        let members: Vec<usize> = chain
            .iter()
            .filter_map(|member| position_of_issue.get(member.id.as_str()).copied())
            .filter(|member| slots[*member].is_some())
            .collect();
        if chain.len() < 2 || members.len() < 2 {
            out.push(take(&mut slots, position));
            continue;
        }
        let children: Vec<SessionTreeNode<T>> = members
            .into_iter()
            .map(|member| take(&mut slots, member))
            .collect();
        let last_activity_at = children
            .iter()
            .map(|child| child.last_activity_at().to_string())
            .max()
            .unwrap_or_default();
        out.push(SessionTreeNode::Stack(StackGroupNode {
            root_issue_id: chain[0].id.clone(),
            children,
            last_activity_at,
        }));
    }
    out
}

/// Newest activity first, ties on the node key — the group children's order
/// and the top level's.
fn sort_by_activity<T>(nodes: &mut [SessionTreeNode<T>]) {
    nodes.sort_by(|a, b| {
        b.last_activity_at()
            .cmp(a.last_activity_at())
            .then_with(|| session_tree_node_key(a).cmp(&session_tree_node_key(b)))
    });
}

/// One row of a DRAWN session tree: the node, how deep it sits and whether it
/// can fold — what [`crate::tree_guides::guides_for`] paints its connector
/// over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTreeFlatRow<'a, T> {
    pub node: &'a SessionTreeNode<T>,
    pub key: String,
    pub depth: usize,
    pub has_children: bool,
}

/// Every SESSION node of the tree, depth-first, groups flattened away (web
/// `flattenSessionTree`, iOS `flatten`, Android `flattenSessionTree`). What a
/// caller wants when it needs the runs a tree holds and not its structure —
/// counting them, or finding the one it should reveal.
pub fn flatten_session_tree<T>(nodes: &[SessionTreeNode<T>]) -> Vec<&SessionNode<T>> {
    fn walk<'a, T>(nodes: &'a [SessionTreeNode<T>], out: &mut Vec<&'a SessionNode<T>>) {
        for node in nodes {
            if let SessionTreeNode::Session(session) = node {
                out.push(session);
            }
            walk(node.children(), out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut out);
    out
}

/// The tree flattened top to bottom, skipping everything under a COLLAPSED
/// node (keyed by [`session_tree_node_key`]). A group row with no children
/// left is DROPPED: a group is its children. Mirrored ×4 with
/// [`session_tree`] itself — every client flattens before it paints.
pub fn visible_session_tree_rows<'a, T>(
    nodes: &'a [SessionTreeNode<T>],
    collapsed: &HashSet<String>,
) -> Vec<SessionTreeFlatRow<'a, T>> {
    fn walk<'a, T>(
        nodes: &'a [SessionTreeNode<T>],
        depth: usize,
        collapsed: &HashSet<String>,
        out: &mut Vec<SessionTreeFlatRow<'a, T>>,
    ) {
        for node in nodes {
            if node.is_group() && node.children().is_empty() {
                continue;
            }
            let key = session_tree_node_key(node);
            let hidden = collapsed.contains(&key);
            out.push(SessionTreeFlatRow {
                node,
                key,
                depth,
                has_children: !node.children().is_empty(),
            });
            if !hidden {
                walk(node.children(), depth + 1, collapsed, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(nodes, 0, collapsed, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Row {
        id: &'static str,
        parent: Option<&'static str>,
        started_at: &'static str,
    }

    fn row(id: &'static str, parent: Option<&'static str>) -> Row {
        Row { id, parent, started_at: "2026-09-10T10:00:00Z" }
    }

    fn row_at(id: &'static str, parent: Option<&'static str>, started_at: &'static str) -> Row {
        Row { id, parent, started_at }
    }

    fn nest(rows: Vec<Row>) -> Vec<TreeRow<Row>> {
        nest_sessions(rows, |r| r.id, |r| r.parent, |r| Some(r.started_at))
    }

    fn shape(rows: &[TreeRow<Row>]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}@{}{}", r.session.id, r.depth, if r.has_children { "+" } else { "" }))
            .collect()
    }

    #[test]
    fn nest_sessions_keeps_the_callers_root_order() {
        let rows = nest(vec![row("b", None), row("a", None), row("c", None)]);
        assert_eq!(shape(&rows), ["b@0", "a@0", "c@0"]);
    }

    #[test]
    fn nest_sessions_nests_a_child_only_under_a_parent_that_is_listed() {
        let rows = nest(vec![row("p", None), row("c", Some("p")), row("orphan", Some("gone"))]);
        assert_eq!(shape(&rows), ["p@0+", "c@1", "orphan@0"]);
    }

    #[test]
    fn nest_sessions_lists_children_right_after_their_parent_oldest_first_recursively() {
        let rows = nest(vec![
            row_at("late", Some("p"), "2026-09-10T12:00:00Z"),
            row("p", None),
            row_at("grand", Some("early"), "2026-09-10T13:00:00Z"),
            row_at("early", Some("p"), "2026-09-10T11:00:00Z"),
            row("z", None),
        ]);
        assert_eq!(shape(&rows), ["p@0+", "early@1+", "grand@2", "late@1", "z@0"]);
        assert_eq!(descendant_ids(&rows, "p", |r| r.id), ["early", "grand", "late"]);
        assert_eq!(descendant_ids(&rows, "early", |r| r.id), ["grand"]);
        assert!(descendant_ids(&rows, "z", |r| r.id).is_empty());
    }

    #[test]
    fn nest_sessions_breaks_a_cycle_where_it_first_appears() {
        let rows = nest(vec![row("a", Some("b")), row("b", Some("a")), row("self", Some("self"))]);
        assert_eq!(shape(&rows), ["self@0", "a@0+", "b@1"]);
    }
}

/// EXP-996 — the web `session-tree.test.ts` table, case for case (the same
/// names, snake_cased), plus the cases only a second implementation can catch.
#[cfg(test)]
mod tree_tests {
    use super::*;
    use crate::rows::Issue;

    /// The web test's `row()`: the columns the tree reads, nothing else.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Run {
        id: &'static str,
        resumed_from_id: Option<&'static str>,
        parent_session_id: Option<&'static str>,
        issue_id: Option<&'static str>,
        batch_issue_ids: Vec<&'static str>,
        /// `created_at` == `updated_at`, like the web fixture's `at()`.
        at: &'static str,
        /// The raw `status` word (`running` unless a case ends it).
        status: &'static str,
        branch: Option<&'static str>,
        /// EXP-1082: the workflow membership columns.
        workflow_id: Option<&'static str>,
        workflow_node_id: Option<&'static str>,
        workflow_role: Option<&'static str>,
    }

    fn run(id: &'static str) -> Run {
        Run {
            id,
            resumed_from_id: None,
            parent_session_id: None,
            issue_id: None,
            batch_issue_ids: Vec::new(),
            at: "2026-09-01T10:00:00Z",
            status: "running",
            branch: None,
            workflow_id: None,
            workflow_node_id: None,
            workflow_role: None,
        }
    }

    impl Run {
        fn at(mut self, at: &'static str) -> Self {
            self.at = at;
            self
        }
        fn resuming(mut self, id: &'static str) -> Self {
            self.resumed_from_id = Some(id);
            self
        }
        fn under(mut self, id: &'static str) -> Self {
            self.parent_session_id = Some(id);
            self
        }
        fn on(mut self, issue_id: &'static str) -> Self {
            self.issue_id = Some(issue_id);
            self
        }
        fn covering(mut self, issue_ids: &[&'static str]) -> Self {
            self.batch_issue_ids = issue_ids.to_vec();
            self
        }
        fn ended(mut self) -> Self {
            self.status = "ended";
            self
        }
        fn branch(mut self, branch: &'static str) -> Self {
            self.branch = Some(branch);
            self
        }
        /// The row's `workflow_id` / `workflow_node_id` / `workflow_role`
        /// (the web test's `member()`).
        fn member(
            mut self,
            workflow_id: &'static str,
            node_id: Option<&'static str>,
            role: &'static str,
        ) -> Self {
            self.workflow_id = Some(workflow_id);
            self.workflow_node_id = node_id;
            self.workflow_role = Some(role);
            self
        }
    }

    fn facts(run: &Run) -> SessionFacts {
        SessionFacts {
            id: run.id.to_string(),
            resumed_from_id: run.resumed_from_id.map(str::to_string),
            parent_session_id: run.parent_session_id.map(str::to_string),
            issue_id: run.issue_id.map(str::to_string),
            batch_issue_ids: run.batch_issue_ids.iter().map(|id| id.to_string()).collect(),
            created_at: run.at.to_string(),
            updated_at: run.at.to_string(),
            status: run.status.to_string(),
            branch: run.branch.map(str::to_string),
            workflow_id: run.workflow_id.map(str::to_string),
            workflow_node_id: run.workflow_node_id.map(str::to_string),
            workflow_role: run.workflow_role.map(str::to_string),
        }
    }

    fn tree(runs: Vec<Run>, context: &SessionTreeContext<'_>) -> Vec<SessionTreeNode<Run>> {
        session_tree(runs, facts, context)
    }

    /// The web test's `ids()`, as one line: keys in order, children in
    /// parentheses (`workflow:w(n2 n1)`).
    fn shape(nodes: &[SessionTreeNode<Run>]) -> String {
        nodes
            .iter()
            .map(|node| {
                let children = shape(node.children());
                let key = session_tree_node_key(node);
                if children.is_empty() {
                    key
                } else {
                    format!("{key}({children})")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A synced `workflows` row's facts, through the projection every caller
    /// uses (so the name fallback is exercised too).
    fn workflow(id: &str, name: Option<&str>) -> WorkflowFacts {
        workflow_in(id, name, "running")
    }

    fn workflow_in(id: &str, name: Option<&str>, status: &str) -> WorkflowFacts {
        WorkflowFacts::from_row(
            &serde_json::from_value(serde_json::json!({
                "id": id,
                "team_id": "team-1",
                "name": name,
                "status": status,
            }))
            .unwrap(),
        )
    }

    fn workflow_node(workflow_id: &str, issue_id: &str) -> WorkflowNodeFacts {
        workflow_node_in(&format!("node-{issue_id}"), workflow_id, issue_id, None)
    }

    fn workflow_node_in(
        id: &str,
        workflow_id: &str,
        issue_id: &str,
        state: Option<&str>,
    ) -> WorkflowNodeFacts {
        WorkflowNodeFacts::from_row(
            &serde_json::from_value(serde_json::json!({
                "id": id,
                "workflow_id": workflow_id,
                "team_id": "team-1",
                "issue_id": issue_id,
                "state": state,
            }))
            .unwrap(),
        )
        .expect("a node row that names its workflow")
    }

    /// APP-1 on the default branch, APP-2 on APP-1's branch, APP-3 on APP-2's
    /// — ids `id-APP-1` … (`pr_stack`'s own fixture builder).
    fn stacked_issues() -> Vec<Issue> {
        vec![
            crate::pr_stack::tests::issue("APP-3", Some("exp/APP-3"), Some("exp/APP-2")),
            crate::pr_stack::tests::issue("APP-2", Some("exp/APP-2"), Some("exp/APP-1")),
            crate::pr_stack::tests::issue("APP-1", Some("exp/APP-1"), Some("master")),
        ]
    }

    #[test]
    fn lists_unrelated_sessions_at_top_level_newest_activity_first() {
        let nodes = tree(
            vec![
                run("a").at("2026-09-01T10:00:00Z"),
                run("b").at("2026-09-01T11:00:00Z"),
            ],
            &SessionTreeContext::default(),
        );
        assert_eq!(shape(&nodes), "b a");
    }

    #[test]
    fn nests_a_child_under_its_parent_session_id() {
        let nodes = tree(
            vec![run("c").under("p").at("2026-09-01T10:30:00Z"), run("p")],
            &SessionTreeContext::default(),
        );
        assert_eq!(shape(&nodes), "p(c)");
    }

    #[test]
    fn collapses_a_resume_succession_into_one_node_keyed_by_its_newest_row() {
        let nodes = tree(
            vec![
                run("r1").at("2026-09-01T10:00:00Z"),
                run("r2").resuming("r1").at("2026-09-01T12:00:00Z"),
            ],
            &SessionTreeContext::default(),
        );
        assert_eq!(shape(&nodes), "r2");
        let SessionTreeNode::Session(node) = &nodes[0] else {
            panic!("a session node");
        };
        assert_eq!(
            node.chain.iter().map(|run| run.id).collect::<Vec<_>>(),
            ["r1", "r2"]
        );
        assert_eq!(node.session().id, "r2");
    }

    #[test]
    fn follows_a_child_to_its_parents_resume_succession() {
        let nodes = tree(
            vec![
                run("p1"),
                run("p2").resuming("p1").at("2026-09-01T11:00:00Z"),
                run("c").under("p1"),
            ],
            &SessionTreeContext::default(),
        );
        assert_eq!(shape(&nodes), "p2(c)");
    }

    #[test]
    fn groups_the_sessions_of_one_workflow_under_a_workflow_node() {
        let workflows = vec![workflow("w", Some("EXP-996 +5"))];
        let workflow_nodes = vec![workflow_node("w", "i1"), workflow_node("w", "i2")];
        let nodes = tree(
            vec![
                run("n1").on("i1").member("w", Some("n1"), "author"),
                run("n2")
                    .on("i2")
                    .member("w", Some("n2"), "author")
                    .at("2026-09-01T11:00:00Z"),
            ],
            &SessionTreeContext {
                workflows: &workflows,
                workflow_nodes: &workflow_nodes,
                issues: &[],
            },
        );
        assert_eq!(shape(&nodes), "workflow:w(n2 n1)");
        let SessionTreeNode::Workflow(group) = &nodes[0] else {
            panic!("a workflow group");
        };
        assert_eq!(group.name, "EXP-996 +5");
    }

    #[test]
    fn groups_a_stack_under_its_lowest_issue_in_linear_order() {
        let issues = stacked_issues();
        let nodes = tree(
            vec![
                run("s-top").on("id-APP-3").at("2026-09-01T12:00:00Z"),
                run("s-low").on("id-APP-1"),
                run("s-mid").on("id-APP-2").at("2026-09-01T11:00:00Z"),
            ],
            &SessionTreeContext {
                issues: &issues,
                ..SessionTreeContext::default()
            },
        );
        assert_eq!(shape(&nodes), "stack:id-APP-1(s-low s-mid s-top)");
    }

    #[test]
    fn sorts_groups_by_their_last_activity_among_the_top_level_nodes() {
        let workflows = vec![workflow("w", Some("W"))];
        let workflow_nodes = vec![workflow_node("w", "i1"), workflow_node("w", "i2")];
        let nodes = tree(
            vec![
                run("lone").at("2026-09-01T11:30:00Z"),
                run("n1").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                run("n2").on("i2").member("w", Some("n2"), "author").at("2026-09-01T12:00:00Z"),
            ],
            &SessionTreeContext {
                workflows: &workflows,
                workflow_nodes: &workflow_nodes,
                issues: &[],
            },
        );
        assert_eq!(shape(&nodes), "workflow:w(n2 n1) lone");
        assert_eq!(nodes[0].last_activity_at(), "2026-09-01T12:00:00Z");
    }

    #[test]
    fn puts_an_orphan_child_whose_parent_is_gone_at_top_level() {
        let nodes = tree(vec![run("o").under("gone")], &SessionTreeContext::default());
        assert_eq!(shape(&nodes), "o");
    }

    #[test]
    fn keeps_children_in_creation_order_under_their_parent() {
        let nodes = tree(
            vec![
                run("c2").under("p").at("2026-09-01T10:20:00Z"),
                run("p"),
                run("c1").under("p").at("2026-09-01T10:10:00Z"),
            ],
            &SessionTreeContext::default(),
        );
        assert_eq!(shape(&nodes), "p(c1 c2)");
    }

    // -----------------------------------------------------------------------
    // The cases a second implementation is written to catch (EXP-1049).
    // -----------------------------------------------------------------------

    /// A fork: the primary succession claims its members oldest-first, so the
    /// OLDER sibling is left over as a node of its own.
    #[test]
    fn an_older_fork_sibling_becomes_its_own_node() {
        let nodes = tree(
            vec![
                run("r1").at("2026-09-01T10:00:00Z"),
                run("r2").resuming("r1").at("2026-09-01T11:00:00Z"),
                run("r3").resuming("r1").at("2026-09-01T12:00:00Z"),
            ],
            &SessionTreeContext::default(),
        );
        assert_eq!(shape(&nodes), "r3 r2");
    }

    /// A stamped run's group row takes its name from the `workflows` row, so
    /// an unsynced workflow leaves its runs where they are.
    #[test]
    fn never_groups_the_runs_of_a_workflow_it_was_not_handed() {
        let workflow_nodes = vec![workflow_node("w", "i1"), workflow_node("w", "i2")];
        let nodes = tree(
            vec![
                run("n1").on("i1").member("w", Some("n1"), "author"),
                run("n2").on("i2").member("w", Some("n2"), "author").at("2026-09-01T11:00:00Z"),
            ],
            &SessionTreeContext {
                workflow_nodes: &workflow_nodes,
                ..SessionTreeContext::default()
            },
        );
        assert_eq!(shape(&nodes), "n2 n1");
    }

    /// A BATCH node run (EXP-876/EXP-978) groups by its stamp like any other;
    /// a nameless workflow row falls back to the generic word.
    #[test]
    fn groups_a_batch_node_run_by_the_issues_it_covers() {
        let workflows = vec![workflow("w", None)];
        let workflow_nodes = vec![workflow_node("w", "i1"), workflow_node("w", "i2")];
        let nodes = tree(
            vec![
                run("batch").covering(&["i1", "i9"]).member("w", Some("n1"), "author"),
                run("n2").on("i2").member("w", Some("n2"), "author").at("2026-09-01T11:00:00Z"),
            ],
            &SessionTreeContext {
                workflows: &workflows,
                workflow_nodes: &workflow_nodes,
                issues: &[],
            },
        );
        assert_eq!(shape(&nodes), "workflow:w(n2 batch)");
        let SessionTreeNode::Workflow(group) = &nodes[0] else {
            panic!("a workflow group");
        };
        assert_eq!(group.name, WORKFLOW_GROUP_FALLBACK_NAME);
    }

    /// Workflow grouping WINS: two stacked issues of one workflow make a
    /// workflow group, never a stack group inside it.
    #[test]
    fn workflow_grouping_beats_stack_grouping() {
        let issues = stacked_issues();
        let workflows = vec![workflow("w", Some("W"))];
        let workflow_nodes = vec![
            workflow_node("w", "id-APP-1"),
            workflow_node("w", "id-APP-2"),
        ];
        let nodes = tree(
            vec![
                run("s-low").on("id-APP-1").member("w", Some("n1"), "author"),
                run("s-mid")
                    .on("id-APP-2")
                    .member("w", Some("n2"), "author")
                    .at("2026-09-01T11:00:00Z"),
            ],
            &SessionTreeContext {
                workflows: &workflows,
                workflow_nodes: &workflow_nodes,
                issues: &issues,
            },
        );
        assert_eq!(shape(&nodes), "workflow:w(s-mid s-low)");
    }

    /// A stack with only ONE of its runs listed is no group.
    #[test]
    fn a_stack_with_one_listed_run_is_no_group() {
        let issues = stacked_issues();
        let nodes = tree(
            vec![run("s-low").on("id-APP-1")],
            &SessionTreeContext {
                issues: &issues,
                ..SessionTreeContext::default()
            },
        );
        assert_eq!(shape(&nodes), "s-low");
    }

    /// A run that names NO issue (a chat, an action, a batch) can be in no
    /// stack: it stays at top level beside the group, never swallowed and
    /// never dropped.
    #[test]
    fn keeps_an_issue_less_run_beside_a_stack_group() {
        let issues = vec![
            crate::pr_stack::tests::issue("APP-2", Some("exp/APP-2"), Some("exp/APP-1")),
            crate::pr_stack::tests::issue("APP-1", Some("exp/APP-1"), None),
        ];
        let nodes = tree(
            vec![
                run("chat").at("2026-09-01T09:00:00Z"),
                run("s-low").on("id-APP-1"),
                run("s-top").on("id-APP-2").at("2026-09-01T11:00:00Z"),
            ],
            &SessionTreeContext {
                issues: &issues,
                ..SessionTreeContext::default()
            },
        );
        assert_eq!(shape(&nodes), "stack:id-APP-1(s-low s-top) chat");
    }

    // -----------------------------------------------------------------------
    // The flattening (`visibleSessionTreeRows`)
    // -----------------------------------------------------------------------

    /// The web's `workflowTree()`: a workflow group with a parent run (one
    /// child) and a second node run.
    fn workflow_tree() -> (Vec<WorkflowFacts>, Vec<WorkflowNodeFacts>, Vec<Run>) {
        (
            vec![workflow("w", Some("W"))],
            vec![workflow_node("w", "i1"), workflow_node("w", "i2")],
            vec![
                run("p").on("i1").member("w", Some("n1"), "author").at("2026-09-01T11:00:00Z"),
                run("c").under("p").at("2026-09-01T10:30:00Z"),
                run("n2").on("i2").member("w", Some("n2"), "author").at("2026-09-01T10:00:00Z"),
            ],
        )
    }

    fn flattened(collapsed: &[&str]) -> Vec<(String, usize, bool)> {
        let (workflows, workflow_nodes, runs) = workflow_tree();
        let nodes = tree(
            runs,
            &SessionTreeContext {
                workflows: &workflows,
                workflow_nodes: &workflow_nodes,
                issues: &[],
            },
        );
        let collapsed: HashSet<String> = collapsed.iter().map(|key| key.to_string()).collect();
        visible_session_tree_rows(&nodes, &collapsed)
            .into_iter()
            .map(|row| (row.key, row.depth, row.has_children))
            .collect()
    }

    /// The web's `flattenSessionTree (EXP-1029 contract)` case, same name.
    #[test]
    fn walks_groups_and_children_depth_first_sessions_only() {
        let (workflows, workflow_nodes, runs) = workflow_tree();
        let nodes = tree(
            runs,
            &SessionTreeContext {
                workflows: &workflows,
                workflow_nodes: &workflow_nodes,
                issues: &[],
            },
        );
        assert_eq!(
            flatten_session_tree(&nodes)
                .into_iter()
                .map(|node| node.id.clone())
                .collect::<Vec<_>>(),
            vec!["p".to_string(), "c".to_string(), "n2".to_string()]
        );
    }

    #[test]
    fn flattens_groups_and_children_with_their_depths() {
        assert_eq!(
            flattened(&[])
                .into_iter()
                .map(|(key, depth, _)| (key, depth))
                .collect::<Vec<_>>(),
            vec![
                ("workflow:w".to_string(), 0),
                ("p".to_string(), 1),
                ("c".to_string(), 2),
                ("n2".to_string(), 1),
            ]
        );
    }

    #[test]
    fn hides_everything_under_a_collapsed_node() {
        let rows = flattened(&["p"]);
        assert_eq!(
            rows.iter().map(|(key, _, _)| key.as_str()).collect::<Vec<_>>(),
            ["workflow:w", "p", "n2"]
        );
        assert!(rows.iter().find(|(key, _, _)| key == "p").unwrap().2);
    }

    #[test]
    fn folds_a_whole_group_away() {
        assert_eq!(
            flattened(&["workflow:w"])
                .iter()
                .map(|(key, _, _)| key.as_str())
                .collect::<Vec<_>>(),
            ["workflow:w"]
        );
    }

    #[test]
    fn keys_a_session_by_its_id_and_a_group_by_its_kind() {
        let (workflows, workflow_nodes, runs) = workflow_tree();
        let nodes = tree(
            runs,
            &SessionTreeContext {
                workflows: &workflows,
                workflow_nodes: &workflow_nodes,
                issues: &[],
            },
        );
        assert_eq!(session_tree_node_key(&nodes[0]), "workflow:w");
        let stack: SessionTreeNode<Run> = SessionTreeNode::Stack(StackGroupNode {
            root_issue_id: "i-low".to_string(),
            children: Vec::new(),
            last_activity_at: String::new(),
        });
        assert_eq!(session_tree_node_key(&stack), "stack:i-low");
        // A childless group row is not drawn: a group IS its children.
        assert!(visible_session_tree_rows(&[stack], &HashSet::new()).is_empty());
    }

    /// The group rows' copy — the workflow fallback and the one NEW string of
    /// the tree, mirrored ×4.
    #[test]
    fn the_group_row_copy_is_locked() {
        assert_eq!(WORKFLOW_GROUP_FALLBACK_NAME, "Workflow");
        assert_eq!(STACK_GROUP_LABEL, "Stacked pull requests");
        assert_eq!(COLLAPSE_GROUP_LABEL, "Collapse these runs");
        assert_eq!(EXPAND_GROUP_LABEL, "Expand these runs");
    }

    /// EXP-1082 → EXP-1068 — membership-first grouping, with the web twin's
    /// rows and case names (snake_cased).
    mod workflow_membership {
        use super::*;

        /// One workflow `w` named "Checkout rewrite", and NO `workflow_nodes`
        /// rows unless a case hands its own: membership alone must group.
        fn membership_tree(runs: Vec<Run>, workflow_nodes: &[WorkflowNodeFacts]) -> Vec<SessionTreeNode<Run>> {
            let workflows = vec![workflow("w", Some("Checkout rewrite"))];
            tree(
                runs,
                &SessionTreeContext {
                    workflows: &workflows,
                    workflow_nodes,
                    issues: &[],
                },
            )
        }

        fn group(nodes: &[SessionTreeNode<Run>]) -> &WorkflowGroupNode<Run> {
            match nodes.first() {
                Some(SessionTreeNode::Workflow(group)) => group,
                _ => panic!("expected a workflow group first"),
            }
        }

        fn session_at<'a>(nodes: &'a [SessionTreeNode<Run>], path: &[usize]) -> &'a SessionNode<Run> {
            let mut cursor = &nodes[path[0]];
            for index in &path[1..] {
                cursor = &cursor.children()[*index];
            }
            match cursor {
                SessionTreeNode::Session(node) => node,
                _ => panic!("expected a session at {path:?}"),
            }
        }

        #[test]
        fn groups_by_workflow_id_before_any_heuristic() {
            // No `workflow_nodes` row names the run or its issue: the row's
            // own `workflow_id` is what folds it under the group.
            let nodes = membership_tree(
                vec![
                    run("a").on("i9").member("w", Some("n1"), "author").at("2026-09-01T11:00:00Z"),
                    run("x").on("i-other"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(a) x");
        }

        #[test]
        fn never_groups_an_unstamped_row_whatever_workflow_nodes_say() {
            let mut node = workflow_node_in("n1", "w", "i1", None);
            node.session_id = Some("n1".to_string());
            let nodes = membership_tree(vec![run("n1").on("i1")], &[node]);
            assert_eq!(shape(&nodes), "n1");
        }

        #[test]
        fn leaves_a_stamped_run_ungrouped_when_the_workflow_is_not_listed() {
            let nodes = tree(
                vec![run("a").on("i1").member("w", Some("n1"), "author")],
                &SessionTreeContext::default(),
            );
            assert_eq!(shape(&nodes), "a");
        }

        #[test]
        fn nests_a_review_run_under_its_nodes_author_row() {
            // `review` on node n1 sits as a child of n1's `author` run — no
            // `parent_session_id` needed — its round read off the branch.
            let nodes = membership_tree(
                vec![
                    run("a").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("r")
                        .member("w", Some("n1"), "review")
                        .branch("exp/wf-2f353e88-review-EXP-1068-r1")
                        .at("2026-09-01T11:00:00Z"),
                    run("b").on("i2").member("w", Some("n2"), "author").at("2026-09-01T09:00:00Z"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(a(r) b)");
            assert_eq!(session_at(&nodes, &[0, 0, 0]).review_round, Some(1));
            assert_eq!(session_at(&nodes, &[0, 0]).review_round, None);
            assert!(!session_at(&nodes, &[0, 0]).duplicate_live);
        }

        #[test]
        fn keeps_a_switched_reviewer_under_its_node() {
            // An account-switch resume of the reviewer (a new row, same
            // membership) collapses into the same chain under n1's author.
            let nodes = membership_tree(
                vec![
                    run("a").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("r1")
                        .member("w", Some("n1"), "review")
                        .ended()
                        .branch("exp/wf-2f353e88-review-EXP-1068-r2")
                        .at("2026-09-01T11:00:00Z"),
                    run("r2")
                        .resuming("r1")
                        .member("w", Some("n1"), "review")
                        .branch("exp/wf-2f353e88-review-EXP-1068-r2")
                        .at("2026-09-01T12:00:00Z"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(a(r2))");
            let reviewer = session_at(&nodes, &[0, 0, 0]);
            assert_eq!(
                reviewer.chain.iter().map(|run| run.id).collect::<Vec<_>>(),
                ["r1", "r2"]
            );
            assert_eq!(reviewer.review_round, Some(2));
            // ONE live reviewer: no duplicate flag on the node.
            assert!(!session_at(&nodes, &[0, 0]).duplicate_live);
        }

        #[test]
        fn nests_a_review_under_the_live_author_not_an_ended_one() {
            let nodes = membership_tree(
                vec![
                    run("a0").on("i1").ended().member("w", Some("n1"), "author").at("2026-09-01T12:00:00Z"),
                    run("a1").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("r").member("w", Some("n1"), "review").at("2026-09-01T11:00:00Z"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(a0 a1(r))");
        }

        #[test]
        fn lists_a_review_whose_node_has_no_author_as_a_child_of_the_group() {
            let nodes = membership_tree(vec![run("r").member("w", Some("n1"), "review")], &[]);
            assert_eq!(shape(&nodes), "workflow:w(r)");
        }

        #[test]
        fn lists_a_base_merge_as_a_child_of_the_group() {
            // `base_merge`, `plan` and `replan` name no node: each is a plain
            // child of the group, never under a node's author run. The
            // group's children sort newest activity first.
            let nodes = membership_tree(
                vec![
                    run("a").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("m").member("w", None, "base_merge").at("2026-09-01T11:00:00Z"),
                    run("p").member("w", None, "plan").at("2026-09-01T08:00:00Z"),
                    run("rp").member("w", None, "replan").at("2026-09-01T12:00:00Z"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(rp m a p)");
        }

        #[test]
        fn names_a_plan_only_group_after_the_plan() {
            // A draft whose only row is its `plan` run still draws a group,
            // named after the plan's working name (the workflow row's name).
            let workflows = vec![workflow_in("w", Some("Checkout rewrite"), "draft")];
            let nodes = tree(
                vec![run("p").member("w", None, "plan")],
                &SessionTreeContext {
                    workflows: &workflows,
                    ..SessionTreeContext::default()
                },
            );
            assert_eq!(shape(&nodes), "workflow:w(p)");
            assert_eq!(group(&nodes).name, "Checkout rewrite");
            assert_eq!(group(&nodes).status, "draft");
        }

        #[test]
        fn keeps_a_foreign_chat_that_resumed_a_workflow_run_inside_the_group() {
            // A resume performed from a chat: its row names the chat as its
            // parent but keeps the workflow membership (EXP-906 inherits it),
            // so the succession stays in the group, not under the chat.
            let nodes = membership_tree(
                vec![
                    run("c").at("2026-09-01T11:00:00Z"),
                    run("a").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("a2")
                        .resuming("a")
                        .under("c")
                        .on("i1")
                        .member("w", Some("n1"), "author")
                        .at("2026-09-01T12:00:00Z"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(a2) c");
        }

        #[test]
        fn nests_a_child_of_a_node_run_under_it_inside_the_group() {
            // A `sessions_start` child inherits the membership and nests under
            // its parent as before; a child with NO workflow of its own nests
            // too.
            let mut kid = run("kid")
                .under("a")
                .member("w", Some("n1"), "author")
                .at("2026-09-01T10:30:00Z");
            kid.workflow_role = None;
            let nodes = membership_tree(
                vec![
                    run("a").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    kid,
                    run("plain").under("a").at("2026-09-01T10:40:00Z"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(a(kid plain))");
        }

        #[test]
        fn groups_a_persons_run_on_a_compound_nodes_sub_issue() {
            // Compound node n1 = parent i1 + sub-issue i2; only the parent
            // has a `workflow_nodes` row. A person's fresh run on i2, stamped
            // `author` of n1 by the server, joins the node's group.
            let workflow_nodes = vec![workflow_node_in("n1", "w", "i1", Some("running"))];
            let nodes = membership_tree(
                vec![
                    run("mine").on("i2").member("w", Some("n1"), "author").at("2026-09-01T11:00:00Z"),
                    run("b").on("i3").member("w", Some("n2"), "author").at("2026-09-01T10:00:00Z"),
                ],
                &workflow_nodes,
            );
            assert_eq!(shape(&nodes), "workflow:w(mine b)");
        }

        #[test]
        fn flags_a_node_with_two_live_author_runs() {
            // Two live `author` rows on one node (a double start): BOTH are
            // listed, neither nested under the other, and both carry the
            // warning.
            let nodes = membership_tree(
                vec![
                    run("a1").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("a2").on("i1").member("w", Some("n1"), "author").at("2026-09-01T11:00:00Z"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(a2 a1)");
            assert!(session_at(&nodes, &[0, 0]).duplicate_live);
            assert!(session_at(&nodes, &[0, 1]).duplicate_live);
        }

        #[test]
        fn flags_a_node_with_two_live_reviewers_on_its_author_row() {
            let nodes = membership_tree(
                vec![
                    run("a").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("r1").member("w", Some("n1"), "review").at("2026-09-01T11:00:00Z"),
                    run("r2").member("w", Some("n1"), "review").at("2026-09-01T11:30:00Z"),
                ],
                &[],
            );
            assert_eq!(shape(&nodes), "workflow:w(a(r1 r2))");
            assert!(session_at(&nodes, &[0, 0]).duplicate_live);
            assert!(!session_at(&nodes, &[0, 0, 0]).duplicate_live);
        }

        #[test]
        fn does_not_flag_a_node_whose_second_author_run_has_ended() {
            let nodes = membership_tree(
                vec![
                    run("a1").on("i1").ended().member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("a2").on("i1").member("w", Some("n1"), "author").at("2026-09-01T11:00:00Z"),
                ],
                &[],
            );
            assert!(!session_at(&nodes, &[0, 0]).duplicate_live);
        }

        #[test]
        fn counts_the_groups_live_runs_and_landed_nodes() {
            let workflow_nodes = vec![
                workflow_node_in("n1", "w", "i1", Some("running")),
                workflow_node_in("n2", "w", "i2", Some("landed")),
                workflow_node_in("n3", "w", "i3", Some("blocked")),
                workflow_node_in("other", "w2", "i4", Some("landed")),
            ];
            let nodes = membership_tree(
                vec![
                    run("a").on("i1").member("w", Some("n1"), "author").at("2026-09-01T10:00:00Z"),
                    run("r").member("w", Some("n1"), "review").at("2026-09-01T11:00:00Z"),
                    run("d").on("i2").ended().member("w", Some("n2"), "author").at("2026-09-01T09:00:00Z"),
                ],
                &workflow_nodes,
            );
            let node = group(&nodes);
            assert_eq!(node.live_runs, 2);
            assert_eq!(node.nodes_done, 1);
            assert_eq!(node.nodes_total, 3);
            assert_eq!(
                workflow_group_caption(node.live_runs, node.nodes_done, node.nodes_total),
                "2 running · 1 of 3 done"
            );
        }

        #[test]
        fn still_groups_a_stack_beside_a_workflow_from_the_leftover_top_level() {
            let workflows = vec![workflow("w", Some("Checkout rewrite"))];
            let issues = vec![
                crate::pr_stack::tests::issue("APP-2", Some("exp/APP-2"), Some("exp/APP-1")),
                crate::pr_stack::tests::issue("APP-1", Some("exp/APP-1"), None),
            ];
            let nodes = tree(
                vec![
                    run("a").on("i1").member("w", Some("n1"), "author"),
                    run("s-low").on("id-APP-1"),
                    run("s-top").on("id-APP-2").at("2026-09-01T11:00:00Z"),
                ],
                &SessionTreeContext {
                    workflows: &workflows,
                    workflow_nodes: &[],
                    issues: &issues,
                },
            );
            assert_eq!(shape(&nodes), "stack:id-APP-1(s-low s-top) workflow:w(a)");
        }
    }

    // EXP-1068: the strings every client draws off the tree, byte-identical ×4.
    mod workflow_group_caption {
        use super::super::workflow_group_caption;

        #[test]
        fn says_running_and_done() {
            assert_eq!(workflow_group_caption(3, 5, 8), "3 running · 5 of 8 done");
        }
        #[test]
        fn drops_the_running_part_with_nothing_live() {
            assert_eq!(workflow_group_caption(0, 5, 8), "5 of 8 done");
        }
        #[test]
        fn drops_the_done_part_before_the_nodes_synced() {
            assert_eq!(workflow_group_caption(1, 0, 0), "1 running");
        }
        #[test]
        fn is_empty_with_neither() {
            assert_eq!(workflow_group_caption(0, 0, 0), "");
        }
    }

    mod review_branch_round {
        use super::super::review_branch_round;

        #[test]
        fn reads_the_round_off_a_review_branch() {
            assert_eq!(review_branch_round(Some("exp/wf-2f353e88-review-EXP-1068-r3")), Some(3));
            assert_eq!(review_branch_round(Some("exp/wf-2f353e88-review-EXP-10-r12")), Some(12));
        }
        #[test]
        fn is_null_for_every_other_branch() {
            assert_eq!(review_branch_round(Some("exp/EXP-1068")), None);
            assert_eq!(review_branch_round(Some("exp/wf-2f353e88-review-EXP-1068-r")), None);
            assert_eq!(review_branch_round(Some("exp/wf-2f353e88-review-EXP-1068-r0")), None);
            assert_eq!(review_branch_round(None), None);
            assert_eq!(review_branch_round(Some("")), None);
        }
    }

    mod review_round_verdict {
        use super::super::{review_round_verdict, ReviewRowVerdict};

        const NODE: (i64, Option<(i64, &str)>) = (2, Some((2, "request_changes")));

        #[test]
        fn reads_the_latest_verdict_for_its_round() {
            assert_eq!(review_round_verdict(Some(2), Some(NODE)), ReviewRowVerdict::ChangesRequested);
            assert_eq!(
                review_round_verdict(Some(2), Some((2, Some((2, "approve"))))),
                ReviewRowVerdict::Approved
            );
        }
        #[test]
        fn calls_an_older_submitted_round_submitted() {
            assert_eq!(review_round_verdict(Some(1), Some(NODE)), ReviewRowVerdict::Submitted);
        }
        #[test]
        fn has_no_verdict_for_the_pending_round_or_without_a_node() {
            assert_eq!(review_round_verdict(Some(3), Some(NODE)), ReviewRowVerdict::None);
            assert_eq!(review_round_verdict(None, Some(NODE)), ReviewRowVerdict::None);
            assert_eq!(review_round_verdict(Some(1), None), ReviewRowVerdict::None);
            assert_eq!(review_round_verdict(Some(1), Some((0, None))), ReviewRowVerdict::None);
        }
    }

    mod review_row_caption {
        use super::super::{review_row_caption, ReviewRowVerdict};

        #[test]
        fn names_the_round_and_the_verdict() {
            assert_eq!(review_row_caption(Some(2), ReviewRowVerdict::Approved, false), "Review r2 · approved");
            assert_eq!(
                review_row_caption(Some(2), ReviewRowVerdict::ChangesRequested, false),
                "Review r2 · changes requested"
            );
            assert_eq!(review_row_caption(Some(1), ReviewRowVerdict::Submitted, false), "Review r1 · submitted");
        }
        #[test]
        fn says_no_verdict_only_once_the_run_ended() {
            assert_eq!(review_row_caption(Some(3), ReviewRowVerdict::None, true), "Review r3");
            assert_eq!(review_row_caption(Some(3), ReviewRowVerdict::None, false), "Review r3 · no verdict");
        }
        #[test]
        fn falls_back_to_a_bare_review_without_a_round() {
            assert_eq!(review_row_caption(None, ReviewRowVerdict::None, true), "Review");
            assert_eq!(review_row_caption(None, ReviewRowVerdict::None, false), "Review · no verdict");
        }
    }
}
