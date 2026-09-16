//! EXP-850 — the steering transcript's PURE derivations: the row projection
//! rules (§9 pending-last, §3/§4 workflow nesting), the working caption (§5),
//! the bottom strip's lines (§1/§2) and the per-turn file cards (§12).
//!
//! Everything here is a function of the feed plus a clock. The view
//! ([`crate::steer_viewer`]) owns the gpui, this owns the rules — so the edge
//! cases (a pending ask under later rows, a workflow agent that must not be a
//! loose row, a 1h03m turn, two edits to the same file) are unit tests rather
//! than something a reader discovers by steering an agent.

use std::collections::HashMap;
use std::rc::Rc;

use domain::edit_card::{EditCardMember, EditCardView};
use steer::{BackgroundTask, FeedItem, FeedItemId, FeedKind, FeedRowSpec, ToolKind};

/// The fallback caption while the run says nothing about its turn (a codex
/// run publishes no `startedAt`; §5 says the whole group is omitted then).
pub(crate) const WORKING_FALLBACK: &str = "Working…";

// ---------------------------------------------------------------------------
// §9 — the pending ask/plan card sits at the bottom
// ---------------------------------------------------------------------------

/// Move every row that carries a PENDING question card after all later rows,
/// keeping the pending rows in their original relative order (§9).
///
/// A resolved card returns to its natural position on its own: this reads the
/// feed each frame and never remembers where a row used to sit.
pub(crate) fn pending_last(rows: &mut Vec<FeedRowSpec>, items: &[FeedItem]) {
    let mut pending: Vec<FeedRowSpec> = Vec::new();
    let mut rest: Vec<FeedRowSpec> = Vec::with_capacity(rows.len());
    for row in rows.drain(..) {
        if row_is_pending(&row, items) {
            pending.push(row);
        } else {
            rest.push(row);
        }
    }
    if pending.is_empty() {
        *rows = rest;
        return;
    }
    rest.append(&mut pending);
    *rows = rest;
}

/// Whether a row holds a question card still waiting on the reader — not
/// resolved and not dismissed (`steer::active_question_ids`' rule, applied per
/// ROW so an ask group moves as one).
fn row_is_pending(row: &FeedRowSpec, items: &[FeedItem]) -> bool {
    row.item_indices()
        .iter()
        .filter_map(|&ix| items.get(ix))
        .any(|item| {
            item.question()
                .is_some_and(|card| !card.resolved && !card.dismissed)
        })
}

// ---------------------------------------------------------------------------
// §3/§4 — a workflow's agents are never loose rows
// ---------------------------------------------------------------------------

/// Drop the subagent rows whose agent belongs to a WORKFLOW the feed HOLDS a
/// card for (§3: "agents of a workflow are never subagent tabs", §4: their
/// edges nest under the card). The card itself renders them, so leaving the
/// row in would print each agent twice.
///
/// `held` is `feed.workflow_ids()`. Hiding on the edge's `workflowId` alone
/// would swallow the agent entirely when the card is gone (evicted past the
/// cap, or a `workflow` frame that never arrived) — web, iOS and Android all
/// gate the same way.
pub(crate) fn hide_workflow_agent_rows(
    rows: &mut Vec<FeedRowSpec>,
    items: &[FeedItem],
    held: &[&str],
) {
    rows.retain(|row| !row_is_workflow_agent(row, items, held));
}

fn row_is_workflow_agent(row: &FeedRowSpec, items: &[FeedItem], held: &[&str]) -> bool {
    matches!(row, FeedRowSpec::Subagent { .. })
        && row
            .item_indices()
            .iter()
            .filter_map(|&ix| items.get(ix))
            .any(|item| match &item.kind {
                FeedKind::Subagent { workflow_id, .. } => workflow_id
                    .as_deref()
                    .is_some_and(|id| held.contains(&id)),
                _ => false,
            })
}

/// §3 — the workflow cards with NO tool row in the rendered projection: the
/// `Workflow` call scrolled out of the window, or the feed trimmed it away.
/// The card is the run's only trace of that workflow, so it renders as its
/// own row at the transcript tail instead of vanishing (web/iOS/Android do
/// the same). Ids come back in the feed's first-appearance order.
pub(crate) fn orphan_workflow_ids(
    rows: &[FeedRowSpec],
    items: &[FeedItem],
    held: &[&str],
) -> Vec<String> {
    let rendered: Vec<&str> = rows
        .iter()
        .flat_map(|row| row.item_indices().iter())
        .filter_map(|&ix| items.get(ix))
        .filter_map(|item| item.call_id())
        .collect();
    held.iter()
        .filter(|id| !rendered.contains(*id))
        .map(|id| (*id).to_string())
        .collect()
}

/// Every feed item tagged with `subagent_id`, by index — what a workflow
/// agent row unfolds into (§3: "a workflow agent row expands to a collapsible
/// preview of nested events when any exist"). The lifecycle markers stay out;
/// only what the agent produced is a preview.
pub(crate) fn nested_agent_items(items: &[FeedItem], subagent_id: &str) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.subagent_id() == Some(subagent_id))
        .filter(|(_, item)| !matches!(item.kind, FeedKind::Subagent { .. }))
        .map(|(ix, _)| ix)
        .collect()
}

/// The DUPLICATE warning edges (§4) this feed holds, in feed order: the item
/// id, the workflow the agent belongs to (when any) and the wire's own
/// sentence, rendered verbatim.
pub(crate) fn duplicate_warnings(items: &[FeedItem]) -> Vec<DuplicateWarning> {
    items
        .iter()
        .filter_map(|item| match &item.kind {
            FeedKind::Subagent {
                status: steer::SubagentStatus::Duplicate,
                detail,
                workflow_id,
                subagent_id,
                ..
            } => Some(DuplicateWarning {
                item: item.id,
                subagent_id: subagent_id.clone(),
                workflow_id: workflow_id.clone(),
                detail: detail.clone().unwrap_or_default(),
            }),
            _ => None,
        })
        .collect()
}

/// One `duplicate` subagent edge, as the amber row renders it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DuplicateWarning {
    pub(crate) item: FeedItemId,
    pub(crate) subagent_id: String,
    pub(crate) workflow_id: Option<String>,
    /// The wire `detail` — never rebuilt per client (EXP-856).
    pub(crate) detail: String,
}

// ---------------------------------------------------------------------------
// §5 — the working caption
// ---------------------------------------------------------------------------

/// What the synthetic working row says this frame.
pub(crate) struct WorkingFacts<'a> {
    /// The turn's start (unix ms), when the publisher sent one.
    pub started_at: Option<i64>,
    /// Output tokens produced in this turn so far.
    pub tokens: Option<u64>,
    pub now_ms: i64,
    /// The caption of the newest RUNNING workflow (`steer::workflow_caption`),
    /// which replaces the verb while one is up.
    pub workflow: Option<&'a str>,
}

/// §5 — `{verb}… ({duration} · ↓ {tokens} tokens)`.
///
/// The verb is picked off the contract's `steerWorking.verbs` by the turn's
/// start, so it is stable for the whole turn and differs between turns. While
/// a workflow runs the head is its caption instead (§7), with the same
/// suffix. With nothing known at all the row falls back to
/// [`WORKING_FALLBACK`].
pub(crate) fn working_caption(facts: &WorkingFacts<'_>) -> String {
    let head = match facts.workflow {
        Some(caption) if !caption.trim().is_empty() => caption.trim().to_string(),
        _ => match facts.started_at {
            Some(started_at) => format!("{}…", working_verb(started_at)),
            None => return WORKING_FALLBACK.to_string(),
        },
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(started_at) = facts.started_at {
        parts.push(format_duration(facts.now_ms.saturating_sub(started_at)));
    }
    if let Some(tokens) = facts.tokens.filter(|tokens| *tokens > 0) {
        parts.push(format!("↓ {} tokens", format_tokens(tokens)));
    }
    if parts.is_empty() {
        return head;
    }
    format!("{head} ({})", parts.join(" · "))
}

/// The turn's verb: `steerWorking.verbs[startedAt % verbs.length]` (§5, the
/// same index on every client).
pub(crate) fn working_verb(started_at: i64) -> &'static str {
    let verbs = domain::contract::STEER_WORKING_VERBS;
    let index = started_at.rem_euclid(verbs.len() as i64) as usize;
    verbs[index]
}

/// `37s` / `2m 04s` / `1h 03m` — the ×4 duration format. A negative span (a
/// host clock ahead of ours) reads as zero.
pub(crate) fn format_duration(ms: i64) -> String {
    let seconds = (ms.max(0) / 1000) as u64;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m {:02}s", seconds % 60);
    }
    format!("{}h {:02}m", minutes / 60, minutes % 60)
}

/// `812` / `2.0k` / `1.2M` — the ×4 token format: one decimal from a
/// thousand up, TRUNCATED, never rounded up past a count the agent has not
/// reached (web `formatTokenCount`, iOS `AgentFeed.workingTokens`, Android
/// `formatWorkingTokens`). 1250 reads `1.2k` and 999_990 `999.9k`.
pub(crate) fn format_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        return tokens.to_string();
    }
    let (tenths, unit) = if tokens < 1_000_000 {
        (tokens / 100, "k")
    } else {
        (tokens / 100_000, "M")
    };
    format!("{}.{}{unit}", tenths / 10, tenths % 10)
}

// ---------------------------------------------------------------------------
// §1/§2 — the strip above the composer
// ---------------------------------------------------------------------------

/// One line of the strip above the composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StripLine {
    /// `↻ {description}` — a background task the CLI reports as running.
    Task(String),
    /// `Waiting on {detail}` — an UNSETTLED `wait` tool row.
    Waiting(String),
}

impl StripLine {
    pub(crate) fn text(&self) -> &str {
        match self {
            StripLine::Task(text) | StripLine::Waiting(text) => text,
        }
    }
}

/// §1/§2 — the strip's lines: one per background task, then one per open wait
/// row, in feed order. Empty means the strip is absent.
pub(crate) fn strip_lines(tasks: &[BackgroundTask], items: &[FeedItem]) -> Vec<StripLine> {
    let mut lines: Vec<StripLine> = tasks
        .iter()
        .map(|task| task.description.trim())
        .filter(|description| !description.is_empty())
        .map(|description| StripLine::Task(description.to_string()))
        .collect();
    for item in items {
        let FeedKind::Tool {
            name,
            detail,
            tool_kind,
            settled,
            ..
        } = &item.kind
        else {
            continue;
        };
        if *settled || *tool_kind != Some(ToolKind::Wait) {
            continue;
        }
        let label = detail
            .as_deref()
            .map(str::trim)
            .filter(|detail| !detail.is_empty())
            .unwrap_or(name.trim());
        if label.is_empty() {
            continue;
        }
        lines.push(StripLine::Waiting(format!("Waiting on {label}")));
    }
    lines
}

// ---------------------------------------------------------------------------
// EXP-916 — the edited-files card's memo
// ---------------------------------------------------------------------------

/// EXP-884/EXP-916 — the PARSE behind one edited-files card, memoised by the
/// card.
///
/// The card's view ([`domain::edit_card::edit_card`]) re-parses every member's
/// unified diff, and the transcript rebuilds its rows on every frame of a live
/// run: without this a run that edited two hundred files re-parsed two hundred
/// patches per frame — the EXP-884 lag, in its EXP-916 shape.
///
/// A card is keyed by its FIRST and LAST member's id, the total bytes of
/// patch behind it and which member is live: those are exactly the four
/// things that move a card's content (it grows at the tail, a `tool_update`
/// lands a patch, the live row moves on). Entries leave with their rows
/// ([`Self::prune_before`], the viewer's eviction hook). Interior mutability,
/// so the read-only render path fills it.
#[derive(Default)]
pub(crate) struct EditMemo {
    by_card: std::cell::RefCell<HashMap<FeedItemId, (CardKey, Rc<EditCardView>)>>,
}

/// What makes one painting of a card different from the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CardKey {
    first: FeedItemId,
    last: FeedItemId,
    bytes: usize,
    live: Option<FeedItemId>,
}

impl EditMemo {
    /// The card `id`'s view, parsed at most once per change.
    pub(crate) fn card(
        &self,
        id: FeedItemId,
        members: &[EditCardMember<'_>],
        live: Option<FeedItemId>,
    ) -> Rc<EditCardView> {
        let key = CardKey {
            first: members.first().map_or(id, |member| member.id),
            last: members.last().map_or(id, |member| member.id),
            bytes: members
                .iter()
                .map(|member| member.diff.map_or(0, str::len))
                .sum(),
            live,
        };
        if let Some((held, view)) = self.by_card.borrow().get(&id) {
            if *held == key {
                return view.clone();
            }
        }
        let view = Rc::new(domain::edit_card::edit_card(members, live));
        self.by_card.borrow_mut().insert(id, (key, view.clone()));
        view
    }

    /// Drop the entries of cards the feed no longer holds (ids below `first`).
    pub(crate) fn prune_before(&self, first: FeedItemId) {
        self.by_card.borrow_mut().retain(|id, _| *id >= first);
    }

    /// Forget everything. The viewer calls this on a replay swap: the feed
    /// re-mints ids from the old anchor (or continues above a retained prefix
    /// over ids the discarded tail held), so a re-minted card id whose members
    /// happen to key the same would otherwise read back the previous card.
    pub(crate) fn clear(&self) {
        self.by_card.borrow_mut().clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.by_card.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steer::{QuestionCard, SubagentStatus};

    fn item(id: FeedItemId, kind: FeedKind) -> FeedItem {
        FeedItem {
            id,
            kind,
            seq: None,
        }
    }

    fn narration(id: FeedItemId) -> FeedItem {
        item(
            id,
            FeedKind::Narration {
                text: "hello".to_string(),
                message_id: None,
                subagent_id: None,
            },
        )
    }

    fn user(id: FeedItemId) -> FeedItem {
        item(
            id,
            FeedKind::UserMessage {
                text: "do it".to_string(),
                subagent_id: None,
            },
        )
    }

    fn question(id: FeedItemId, resolved: bool) -> FeedItem {
        item(
            id,
            FeedKind::Question(QuestionCard {
                text: "pick".to_string(),
                options: Vec::new(),
                multi_select: false,
                plan_mode: false,
                question_id: format!("q-{id}"),
                ask_id: None,
                index: None,
                total: None,
                header: None,
                resolved,
                answer: None,
                dismissed: false,
            }),
        )
    }

    fn tool(id: FeedItemId, kind: Option<ToolKind>, settled: bool, diff: Option<&str>) -> FeedItem {
        item(
            id,
            FeedKind::Tool {
                name: "Edit".to_string(),
                detail: Some("src/a.rs".to_string()),
                subagent_id: None,
                call_id: Some(format!("call-{id}")),
                tool_kind: kind,
                settled,
                failed: false,
                diff: diff.map(str::to_string),
                output: None,
                preview: None,
            },
        )
    }

    fn patch(path: &str, additions: usize, deletions: usize) -> String {
        let mut out = format!("--- a/{path}\n+++ b/{path}\n@@ -1,{deletions} +1,{additions} @@\n");
        for ix in 0..deletions {
            out.push_str(&format!("-old {ix}\n"));
        }
        for ix in 0..additions {
            out.push_str(&format!("+new {ix}\n"));
        }
        out
    }

    fn subagent(id: FeedItemId, agent: &str, workflow: Option<&str>) -> FeedItem {
        item(
            id,
            FeedKind::Subagent {
                subagent_id: agent.to_string(),
                agent_type: "general-purpose".to_string(),
                status: SubagentStatus::Started,
                detail: None,
                tool_calls: None,
                title: None,
                workflow_id: workflow.map(str::to_string),
            },
        )
    }

    fn specs(items: &[FeedItem]) -> Vec<FeedRowSpec> {
        steer::group_feed_row_specs(items, &[])
    }

    /// The same projection with the feed's workflow ids in hand (§3).
    fn specs_with(items: &[FeedItem], workflows: &[&str]) -> Vec<FeedRowSpec> {
        steer::group_feed_row_specs(items, workflows)
    }

    /// §9: an unresolved card moves after every later row, and its answered
    /// twin stays exactly where the feed put it.
    #[test]
    fn a_pending_card_moves_to_the_bottom_and_comes_back_when_answered() {
        let items = vec![narration(1), question(2, false), narration(3), narration(4)];
        let mut rows = specs(&items);
        pending_last(&mut rows, &items);
        assert_eq!(
            rows.iter().map(FeedRowSpec::id).collect::<Vec<_>>(),
            vec![1, 3, 4, 2]
        );

        let answered = vec![narration(1), question(2, true), narration(3), narration(4)];
        let mut rows = specs(&answered);
        pending_last(&mut rows, &answered);
        assert_eq!(
            rows.iter().map(FeedRowSpec::id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
    }

    /// §9: two pending cards keep their own order at the bottom.
    #[test]
    fn pending_cards_keep_their_relative_order() {
        let items = vec![question(1, false), narration(2), question(3, false), narration(4)];
        let mut rows = specs(&items);
        pending_last(&mut rows, &items);
        assert_eq!(
            rows.iter().map(FeedRowSpec::id).collect::<Vec<_>>(),
            vec![2, 4, 1, 3]
        );
    }

    /// §3 review: a card whose `Workflow` row is outside the rendered
    /// projection (scrolled out of the window, or trimmed off the feed) is
    /// an ORPHAN — rendered as its own row at the tail rather than lost. A
    /// card whose row IS on screen is not.
    #[test]
    fn a_card_without_its_tool_row_is_an_orphan() {
        let items = vec![
            narration(1),
            tool(2, Some(ToolKind::Other), true, None),
            narration(3),
            tool(4, Some(ToolKind::Other), true, None),
        ];
        let held = ["call-2", "call-4"];

        // The whole feed: both `Workflow` rows are on screen, no orphans.
        let rows = specs_with(&items, &held);
        assert!(orphan_workflow_ids(&rows, &items, &held).is_empty());

        // A window that starts past the first call orphans exactly that card.
        let windowed = steer::group_feed_row_specs_from(&items, 2, &held);
        assert_eq!(
            orphan_workflow_ids(&windowed, &items, &held),
            vec!["call-2".to_string()]
        );

        // A card whose row the feed never held at all is an orphan too, and
        // the order is the feed's own.
        let held = ["call-9", "call-2", "call-8"];
        let rows = specs_with(&items, &held);
        assert_eq!(
            orphan_workflow_ids(&rows, &items, &held),
            vec!["call-9".to_string(), "call-8".to_string()]
        );
    }

    /// §3/§4: a workflow agent's row never renders on the main line (the card
    /// holds it); an ordinary subagent's does.
    #[test]
    fn workflow_agent_rows_leave_the_main_transcript() {
        let items = vec![
            narration(1),
            subagent(2, "a-1", Some("toolu_wf")),
            subagent(3, "a-2", None),
        ];
        let mut rows = specs(&items);
        hide_workflow_agent_rows(&mut rows, &items, &["toolu_wf"]);
        assert_eq!(
            rows.iter().map(FeedRowSpec::id).collect::<Vec<_>>(),
            vec![1, 3]
        );

        // Review: with NO card held for that workflow the agent's row is the
        // only trace of it left, so it stays on the main line (×4).
        let mut rows = specs(&items);
        hide_workflow_agent_rows(&mut rows, &items, &[]);
        assert_eq!(
            rows.iter().map(FeedRowSpec::id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    /// §4: the wire sentence is rendered verbatim, and the edge names the
    /// card it belongs under.
    #[test]
    fn a_duplicate_edge_carries_its_sentence_and_its_workflow() {
        let detail = "Second copy of slowpoke started while the first is still running (resumed by SendMessage)";
        let mut dup = subagent(7, "a-1", Some("toolu_wf"));
        if let FeedKind::Subagent { status, detail: d, .. } = &mut dup.kind {
            *status = SubagentStatus::Duplicate;
            *d = Some(detail.to_string());
        }
        let items = vec![narration(1), dup];
        let warnings = duplicate_warnings(&items);
        assert_eq!(
            warnings,
            vec![DuplicateWarning {
                item: 7,
                subagent_id: "a-1".to_string(),
                workflow_id: Some("toolu_wf".to_string()),
                detail: detail.to_string(),
            }]
        );
        // A started edge is not a warning.
        assert!(duplicate_warnings(&[narration(1), subagent(2, "a-1", None)]).is_empty());
    }

    /// §3: an agent's nested events are its prose and calls, never its own
    /// lifecycle markers.
    #[test]
    fn nested_agent_items_are_the_agents_own_output() {
        let mut prose = narration(3);
        if let FeedKind::Narration { subagent_id, .. } = &mut prose.kind {
            *subagent_id = Some("a-1".to_string());
        }
        let items = vec![narration(1), subagent(2, "a-1", Some("wf")), prose];
        assert_eq!(nested_agent_items(&items, "a-1"), vec![2usize]);
        assert!(nested_agent_items(&items, "a-2").is_empty());
    }

    /// §5: the caption, in all four shapes.
    #[test]
    fn the_working_caption_names_the_verb_the_clock_and_the_tokens() {
        let verbs = domain::contract::STEER_WORKING_VERBS;
        let started_at = 1_789_204_409_163i64;
        let verb = verbs[(started_at as usize) % verbs.len()];
        assert_eq!(
            working_caption(&WorkingFacts {
                started_at: Some(started_at),
                tokens: Some(812),
                now_ms: started_at + 37_000,
                workflow: None,
            }),
            format!("{verb}… (37s · ↓ 812 tokens)")
        );
        // No turn start at all (codex): the bare fallback.
        assert_eq!(
            working_caption(&WorkingFacts {
                started_at: None,
                tokens: None,
                now_ms: started_at,
                workflow: None,
            }),
            WORKING_FALLBACK
        );
        // Tokens unknown: the group holds the duration alone.
        assert_eq!(
            working_caption(&WorkingFacts {
                started_at: Some(started_at),
                tokens: None,
                now_ms: started_at + 124_000,
                workflow: None,
            }),
            format!("{verb}… (2m 04s)")
        );
        // A running workflow replaces the verb, keeps the suffix.
        assert_eq!(
            working_caption(&WorkingFacts {
                started_at: Some(started_at),
                tokens: Some(2_000),
                now_ms: started_at + 3_780_000,
                workflow: Some("Workflow wire-probe · 2/3 agents done · Beta"),
            }),
            "Workflow wire-probe · 2/3 agents done · Beta (1h 03m · ↓ 2.0k tokens)"
        );
    }

    /// §5: the verb is stable for a turn and indexed exactly as the contract
    /// says, including for a negative (clock-skewed) start.
    #[test]
    fn the_verb_indexes_the_contract_list() {
        let verbs = domain::contract::STEER_WORKING_VERBS;
        assert_eq!(working_verb(0), verbs[0]);
        assert_eq!(working_verb(verbs.len() as i64), verbs[0]);
        assert_eq!(working_verb(verbs.len() as i64 + 3), verbs[3]);
        assert_eq!(working_verb(-1), verbs[verbs.len() - 1]);
    }

    /// §5: the two formatters, at every boundary.
    #[test]
    fn durations_and_tokens_read_the_way_the_spec_writes_them() {
        assert_eq!(format_duration(-5), "0s");
        assert_eq!(format_duration(37_000), "37s");
        assert_eq!(format_duration(59_999), "59s");
        assert_eq!(format_duration(60_000), "1m 00s");
        assert_eq!(format_duration(124_000), "2m 04s");
        assert_eq!(format_duration(3_599_000), "59m 59s");
        assert_eq!(format_duration(3_780_000), "1h 03m");

        // Truncated, never rounded — the ×4 rule.
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(812), "812");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1_000), "1.0k");
        assert_eq!(format_tokens(1_250), "1.2k");
        assert_eq!(format_tokens(1_960), "1.9k");
        assert_eq!(format_tokens(2_000), "2.0k");
        assert_eq!(format_tokens(12_345), "12.3k");
        assert_eq!(format_tokens(999_990), "999.9k");
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(1_250_000), "1.2M");
        assert_eq!(format_tokens(1_200_000), "1.2M");
    }

    /// §1/§2: the strip lists the tasks, then every OPEN wait row; a settled
    /// wait is gone and an empty pair draws nothing.
    #[test]
    fn the_strip_lists_background_tasks_then_open_waits() {
        let tasks = vec![BackgroundTask {
            id: "b4mwz6csc".to_string(),
            kind: steer::BackgroundTaskKind::Shell,
            description: "Sleep in the background".to_string(),
            tool_id: None,
        }];
        let mut waiting = tool(2, Some(ToolKind::Wait), false, None);
        if let FeedKind::Tool { detail, name, .. } = &mut waiting.kind {
            *name = "TaskOutput".to_string();
            *detail = Some("Sleep in the background".to_string());
        }
        let settled = tool(3, Some(ToolKind::Wait), true, None);
        let items = vec![narration(1), waiting, settled];
        assert_eq!(
            strip_lines(&tasks, &items),
            vec![
                StripLine::Task("Sleep in the background".to_string()),
                StripLine::Waiting("Waiting on Sleep in the background".to_string()),
            ]
        );
        assert!(strip_lines(&[], &[narration(1)]).is_empty());
    }

    /// §1: a wait row with no detail still names the call.
    #[test]
    fn a_detail_less_wait_falls_back_to_the_tool_name() {
        let mut waiting = tool(2, Some(ToolKind::Wait), false, None);
        if let FeedKind::Tool { detail, name, .. } = &mut waiting.kind {
            *name = "Monitor".to_string();
            *detail = None;
        }
        assert_eq!(
            strip_lines(&[], &[waiting]),
            vec![StripLine::Waiting("Waiting on Monitor".to_string())]
        );
    }

    /// EXP-916: the transcript's rows carry the edited-files CARD — the
    /// grouping is `steer::feed`'s (byte-locked by
    /// `fixtures/feed/edit-cards.json`), and a subagent's edits never join
    /// the main line's.
    #[test]
    fn consecutive_edits_group_into_one_card_row() {
        let mut a = tool(2, Some(ToolKind::Edit), true, Some(&patch("src/a.rs", 3, 1)));
        if let FeedKind::Tool { detail, .. } = &mut a.kind {
            *detail = Some("src/a.rs".to_string());
        }
        let b = tool(3, Some(ToolKind::Edit), true, Some(&patch("src/b.rs", 2, 0)));
        let read = tool(4, Some(ToolKind::Read), true, None);
        let mut nested = tool(5, Some(ToolKind::Edit), true, Some(&patch("src/c.rs", 1, 0)));
        if let FeedKind::Tool { subagent_id, .. } = &mut nested.kind {
            *subagent_id = Some("a-1".to_string());
        }
        let items = vec![user(1), a, b, read, nested];
        let rows = steer::feed::group_feed_row_specs(&items, &[]);
        assert!(matches!(rows[0], FeedRowSpec::Single { item: 0, .. }));
        assert!(
            matches!(rows[1], FeedRowSpec::Edits { id: 2, ref items } if items == &[1, 2]),
            "the two edits are ONE card, keyed by the first"
        );
        assert!(matches!(rows[2], FeedRowSpec::Single { item: 3, .. }), "a read is its own row");
        assert!(
            matches!(rows[3], FeedRowSpec::Subagent { .. }),
            "and the subagent's edit stays in its own lane"
        );
    }

    // ── The recorded wire, end to end ─────────────────────────────────────

    /// The EXACT frames the engine publishes for a claude workflow run, cut
    /// from `crates/engine/tests/fixtures/claude/wire-captures/` (the
    /// `workflow`, `background-tasks` and `duplicate-agent` captures) and
    /// pinned in the wave's wire notes. Decoding them here is what proves the
    /// transcript reads what the engine actually sends, rather than what this
    /// module's own constructors happen to build.
    const WORKFLOW_FRAME: &str = r#"{"kind":"workflow","id":"toolu_017aGvi2moAfSykrRA4LmyT4","name":"wire-probe","description":"Probe the workflow progress wire","status":"running","phases":[{"index":1,"title":"Alpha"},{"index":2,"title":"Beta"}],"agents":[{"index":1,"label":"alpha:one","phaseIndex":1,"agentId":"a0ce244c651aaa623","model":"claude-haiku-4-5-20251001","state":"done","tokens":9629,"toolCalls":0,"durationMs":1075,"resultPreview":"ok"},{"index":2,"label":"alpha:two","phaseIndex":1,"state":"queued"}],"summary":"Dynamic workflow completed"}"#;
    const BACKGROUND_TASKS_FRAME: &str = r#"{"kind":"background_tasks","tasks":[{"id":"b4mwz6csc","kind":"shell","description":"Sleep in the background","toolId":"toolu_01MCRoRaXN1cvEsHJzDEg2B3"}]}"#;
    const DUPLICATE_FRAME: &str = r#"{"kind":"subagent","id":"a55b7012793deae02","agentType":"general-purpose","status":"duplicate","detail":"Second copy of slowpoke started while the first is still running (resumed by SendMessage)","title":"slowpoke","workflowId":"toolu_017Lh63mYhRJ3MrA4A1PXytt"}"#;
    const WAIT_ROW_FRAME: &str = r#"{"kind":"tool","name":"TaskOutput","detail":"Sleep in the background","id":"toolu_1","toolKind":"wait"}"#;
    const TURN_FRAME: &str = r#"{"kind":"turn","state":"started","startedAt":1789204409163,"tokens":1432}"#;

    fn event(json: &str) -> steer::ActivityEvent {
        serde_json::from_str(json).expect("the recorded frame decodes")
    }

    /// §3: the recorded `workflow` frame lands as a SIDE MAP keyed by the
    /// `Workflow` call's id — the card the transcript renders in place of
    /// that tool row — and its caption is the ×4 one.
    #[test]
    fn the_recorded_workflow_frame_patches_onto_its_tool_row() {
        let mut feed = steer::SteerFeed::new();
        feed.apply(steer::ActivityEvent::Tool {
            name: "Workflow".to_string(),
            detail: Some("wire-probe".to_string()),
            id: Some("toolu_017aGvi2moAfSykrRA4LmyT4".to_string()),
            tool_kind: None,
            subagent_id: None,
            at: None,
        });
        feed.apply(event(WORKFLOW_FRAME));
        // The tool row is still ONE row; the card hangs off its call id.
        let rows = feed.row_specs();
        assert_eq!(rows.len(), 1);
        let call_id = match &feed.items()[0].kind {
            FeedKind::Tool { call_id, .. } => call_id.clone().expect("the row keeps its id"),
            other => panic!("expected a tool row, got {other:?}"),
        };
        let workflow = feed.workflow_for(&call_id).expect("the card is keyed by it");
        assert_eq!(workflow.name, "wire-probe");
        assert_eq!(
            steer::workflow_caption(workflow),
            // Both recorded agents sit in phase Alpha, and the caption names
            // the lead agent's phase (§7's fixture rule).
            "Workflow wire-probe · 1/2 agents done · Alpha"
        );
        assert_eq!(
            crate::workflow_card::agent_meta(&workflow.agents[0]),
            "claude-haiku-4-5-20251001 · 9.6k tokens · 1s"
        );
        assert_eq!(
            crate::workflow_card::phase_counts(workflow)
                .iter()
                .map(|phase| phase.caption())
                .collect::<Vec<_>>(),
            vec!["1 done · 1 queued".to_string(), String::new()]
        );
        // …and it is the newest RUNNING one, so it drives the working caption.
        assert!(feed.running_workflow().is_some());
    }

    /// §1/§2/§5: the recorded `background_tasks`, `wait` and `turn` frames
    /// drive the strip and the working caption.
    #[test]
    fn the_recorded_strip_and_turn_frames_drive_the_bottom_surfaces() {
        let mut feed = steer::SteerFeed::new();
        feed.apply(event(WAIT_ROW_FRAME));
        feed.apply(event(BACKGROUND_TASKS_FRAME));
        feed.apply(event(TURN_FRAME));
        assert_eq!(
            strip_lines(feed.background_tasks(), feed.items()),
            vec![
                StripLine::Task("Sleep in the background".to_string()),
                StripLine::Waiting("Waiting on Sleep in the background".to_string()),
            ]
        );
        // The wait row stays an ordinary tool row in the transcript (§1).
        assert_eq!(feed.row_specs().len(), 1);
        // …and it leaves the strip the moment it settles.
        feed.apply(steer::ActivityEvent::tool_update(
            "toolu_1",
            Some(steer::ToolUpdateStatus::Completed),
            None,
        ));
        assert_eq!(
            strip_lines(feed.background_tasks(), feed.items()),
            vec![StripLine::Task("Sleep in the background".to_string())]
        );

        let started_at = feed.turn_started_at().expect("the turn named its start");
        assert_eq!(started_at, 1_789_204_409_163);
        assert_eq!(feed.turn_tokens(), Some(1_432));
        assert_eq!(
            working_caption(&WorkingFacts {
                started_at: Some(started_at),
                tokens: feed.turn_tokens(),
                now_ms: started_at + 9_000,
                workflow: None,
            }),
            format!("{}… (9s · ↓ 1.4k tokens)", working_verb(started_at))
        );
    }

    /// §4: the recorded duplicate edge keeps its sentence through the feed and
    /// belongs to the workflow card, whose agent rows never reach the main
    /// transcript (§3).
    #[test]
    fn the_recorded_duplicate_edge_nests_under_its_card() {
        let mut feed = steer::SteerFeed::new();
        feed.apply(steer::ActivityEvent::narration("working"));
        feed.apply(event(DUPLICATE_FRAME));
        let warnings = duplicate_warnings(feed.items());
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].detail,
            "Second copy of slowpoke started while the first is still running (resumed by SendMessage)"
        );
        assert_eq!(
            warnings[0].workflow_id.as_deref(),
            Some("toolu_017Lh63mYhRJ3MrA4A1PXytt")
        );
        // Review: the row nests under the card only once the feed HOLDS one
        // — until then the edge is the only trace of that agent.
        let mut loose = feed.row_specs();
        hide_workflow_agent_rows(&mut loose, feed.items(), &feed.workflow_ids());
        assert_eq!(loose.len(), 2, "no card yet, so the edge stays on the line");
        feed.apply(steer::ActivityEvent::workflow(steer::WorkflowState {
            id: "toolu_017Lh63mYhRJ3MrA4A1PXytt".to_string(),
            name: "wire-probe".to_string(),
            status: steer::WorkflowStatus::Running,
            ..steer::WorkflowState::default()
        }));
        let mut rows = feed.row_specs();
        hide_workflow_agent_rows(&mut rows, feed.items(), &feed.workflow_ids());
        assert_eq!(rows.len(), 1, "the agent's row belongs to the card");
        // …and with no `Workflow` tool row in the projection the card is an
        // ORPHAN, rendered at the tail rather than lost.
        assert_eq!(
            orphan_workflow_ids(&rows, feed.items(), &feed.workflow_ids()),
            vec!["toolu_017Lh63mYhRJ3MrA4A1PXytt".to_string()]
        );
        // A workflow agent is never a steerable TAB either.
        assert!(steer::feed::visible_subagent_tabs(&feed.subagents(), None).is_empty());
    }

    /// EXP-884/EXP-916: the memo parses each CARD once, re-reads it only
    /// when its members, their bytes or the live row moved, and forgets the
    /// cards the feed evicted.
    #[test]
    fn the_edit_memo_parses_each_card_once_and_follows_eviction() {
        let a = patch("src/a.rs", 2, 1);
        let b = patch("src/b.rs", 1, 0);
        let grown = patch("src/a.rs", 5, 1);
        let memo = EditMemo::default();
        let members = vec![
            EditCardMember {
                id: 2,
                detail: Some("src/a.rs"),
                diff: Some(a.as_str()),
                settled: true,
            },
            EditCardMember {
                id: 3,
                detail: Some("src/b.rs"),
                diff: Some(b.as_str()),
                settled: true,
            },
        ];
        let first = memo.card(2, &members, None);
        assert_eq!(first.title, "2 files edited");
        assert_eq!(memo.len(), 1);
        assert!(
            Rc::ptr_eq(&first, &memo.card(2, &members, None)),
            "a second frame re-uses the parse"
        );
        // A longer patch for the same member is a different card.
        let regrown = vec![
            EditCardMember {
                diff: Some(grown.as_str()),
                ..members[0]
            },
            members[1],
        ];
        let next = memo.card(2, &regrown, None);
        assert_eq!(next.rows[0].file.as_ref().expect("a patch").additions, 5);
        // The live member is part of the key — it names the one open row.
        assert_eq!(memo.card(2, &regrown, Some(3)).live_index, Some(1));
        // Eviction: cards below the surviving first id leave the memo.
        memo.prune_before(3);
        assert_eq!(memo.len(), 0);
    }
}

#[cfg(test)]
mod bench {
    //! EXP-884 — the per-frame whole-feed passes on a LONG run, timed. Run
    //! with `cargo test -p ui --release -- --ignored bench_long_feed --nocapture`.
    use std::time::Instant;

    fn diff(path: &str, lines: usize) -> String {
        let mut out = format!("--- a/{path}\n+++ b/{path}\n@@ -1,{lines} +1,{lines} @@\n");
        for ix in 0..lines {
            out.push_str(&format!("-old line number {ix} with some text in it\n"));
            out.push_str(&format!("+new line number {ix} with some other text\n"));
        }
        out
    }

    pub(crate) fn long_feed(turns: usize) -> steer::SteerFeed {
        let mut feed = steer::SteerFeed::new();
        let mut call = 0u64;
        for turn in 0..turns {
            feed.apply(steer::ActivityEvent::user_message(format!("turn {turn}: please do it")));
            for n in 0..12 {
                feed.apply(steer::ActivityEvent::Narration {
                    text: format!("Narration {n} of turn {turn}. ").repeat(12),
                    before_question_id: None,
                    message_id: Some(format!("m-{turn}-{n}")),
                    subagent_id: None,
                    at: None,
                });
                for _ in 0..2 {
                    call += 1;
                    let id = format!("toolu_{call}");
                    let edit = call % 3 == 0;
                    feed.apply(steer::ActivityEvent::Tool {
                        name: if edit { "Edit".into() } else { "Read".into() },
                        detail: Some(format!("src/file_{}.rs", call % 40)),
                        id: Some(id.clone()),
                        tool_kind: Some(if edit { steer::ToolKind::Edit } else { steer::ToolKind::Read }),
                        subagent_id: None,
                        at: None,
                    });
                    feed.apply(steer::ActivityEvent::tool_update(
                        id,
                        Some(steer::ToolUpdateStatus::Completed),
                        edit.then(|| diff(&format!("src/file_{}.rs", call % 40), 30)),
                    ));
                }
            }
            let agent = format!("agent-{turn}");
            feed.apply(steer::ActivityEvent::Subagent {
                id: agent.clone(),
                agent_type: "Explore".into(),
                status: steer::SubagentStatus::Started,
                detail: None,
                at: None,
                tool_calls: None,
                title: Some("look around".into()),
                workflow_id: None,
            });
            for _ in 0..10 {
                call += 1;
                feed.apply(steer::ActivityEvent::Tool {
                    name: "Grep".into(),
                    detail: Some("pattern".into()),
                    id: Some(format!("toolu_{call}")),
                    tool_kind: Some(steer::ToolKind::Search),
                    subagent_id: Some(agent.clone()),
                    at: None,
                });
            }
            feed.apply(steer::ActivityEvent::Subagent {
                id: agent,
                agent_type: "Explore".into(),
                status: steer::SubagentStatus::Completed,
                detail: Some("done".into()),
                at: None,
                tool_calls: Some(10),
                title: Some("look around".into()),
                workflow_id: None,
            });
        }
        feed
    }

    fn timed<T>(label: &str, reps: u32, mut f: impl FnMut() -> T) -> T {
        let started = Instant::now();
        let mut out = None;
        for _ in 0..reps {
            out = Some(f());
        }
        let per = started.elapsed() / reps;
        eprintln!("{label:<28} {per:>10.1?} per frame");
        out.unwrap()
    }

    #[test]
    #[ignore]
    fn bench_long_feed() {
        let feed = long_feed(600);
        let items = feed.items();
        eprintln!("items: {} ({} KiB)", items.len(), feed.bytes() / 1024);
        let reps = 20;
        timed("active_question_ids", reps, || feed.active_question_ids());
        let prose = "Narration 3 of turn 7. ".repeat(12);
        timed("markdown parse (1 body)", reps, || crate::markdown::markdown_to_blocks(&prose));
        timed("collect_subagents x2", reps, || (feed.subagents(), feed.subagents()));
        timed("strip_lines", reps, || super::strip_lines(feed.background_tasks(), items));
        timed("duplicate_warnings", reps, || super::duplicate_warnings(items));
        let start = items.len().saturating_sub(domain::contract::STEER_FEED_WINDOW);
        let mut rows = Vec::new();
        timed("row_specs (window)", reps, || feed.row_specs_from_into(start, &mut rows));
        let held = feed.workflow_ids();
        timed("window post-passes", reps, || {
            let mut rows = rows.clone();
            super::hide_workflow_agent_rows(&mut rows, items, &held);
            super::pending_last(&mut rows, items);
            super::orphan_workflow_ids(&rows, items, &held)
        });
    }
}
