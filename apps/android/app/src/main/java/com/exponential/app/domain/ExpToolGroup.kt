package com.exponential.app.domain

/**
 * EXP-948: our OWN tools never hide. An Exponential MCP call is the most
 * meaningful row a transcript has — "Read issue EXP-901", "Opened pull
 * request" — and until now a pile of them vanished inside a collapsed
 * "Ran 1 command · 21 other tools" fold. This object owns the ONE rule that
 * keeps them visible and the ONE caption a run of them renders, over the
 * contract's `expToolDisplay` table ([ExpToolDisplay] resolves the row, so the
 * name lookup is written down once).
 *
 * Hand-mirrored ×4 (TS `@exp/domain-contract` `exp-tool-group.ts`, web
 * `lib/agent-feed.ts` `scanToolRuns`, desktop `steer::exp_tool_group`, iOS
 * `ExpCore/Sources/Domain/ExpToolGroup.swift`) and byte-locked by
 * `fixtures/feed/exp-tool-groups.json`, which every client's feed test replays
 * through its OWN feed-row projection.
 *
 * Grouping (a render ROW of the feed projection, [AgentFeedRow.ExpRun]):
 * - an Exponential call is a `tool` item that is not a workflow card's own call
 *   (EXP-850 §3 — on Android a workflow is named by its `callId`, which is what
 *   the TS mirror's `workflowId` field says) and whose NAME resolves to an
 *   `expToolDisplay` row ([ExpToolDisplay.row]: the contract prefix right in
 *   front of a known row name, whatever namespace an adapter put in front of
 *   THAT);
 * - an Exponential call NEVER joins a generic tool run — it breaks one exactly
 *   like an edit call does (EXP-916), so it is never counted as "N other
 *   tools";
 * - an [AgentFeedRow.ExpRun] is a MAXIMAL run of ≥2 consecutive items of ONE
 *   lane ([AgentFeedItem.Tool.subagentId], null = the main lane) that are
 *   Exponential calls of the SAME contract row. A single call stays the single
 *   visible row it always was, and two DIFFERENT Exponential tools in sequence
 *   are two rows;
 * - the rule reads ONLY the kind, the name, the workflow set and the lane —
 *   never `settled`/`failed` — so a group never re-splits when a call settles;
 * - the row's id is its FIRST item's id (stable while a live run grows); the
 *   window start (EXP-783) opens a fresh group at its boundary like every other
 *   group.
 *
 * The caption ([expToolGroupCaption]):
 * - the contract's `progressiveMany` while ANY member is still running, its
 *   `doneMany` once every member settled, `{n}` = the member count
 *   ("Reading 3 issues" → "Read 3 issues");
 * - a failed member appends ` · N failed`, the same tail (and the same
 *   separator) [ToolGroupSummary] writes.
 */
object ExpToolGroup {

    /**
     * Whether a feed item is a call to one of OUR tools. [workflowIds] are the
     * call ids this screen holds a workflow card for — such a call is its own
     * card and never folds into a run.
     */
    fun isExpToolCall(item: AgentFeedItem, workflowIds: Set<String> = emptySet()): Boolean {
        if (item !is AgentFeedItem.Tool) return false
        if (item.callId != null && item.callId in workflowIds) return false
        return ExpToolDisplay.row(item.name) != null
    }

    /**
     * The inclusive end index of the maximal run of same-lane calls to the SAME
     * Exponential tool that starts at [start] (which must itself be one). A
     * caller's group scan uses it exactly like its edit-run scan.
     */
    fun expToolRunEnd(
        feed: List<AgentFeedItem>,
        start: Int,
        workflowIds: Set<String> = emptySet(),
    ): Int {
        val opener = feed.getOrNull(start) as? AgentFeedItem.Tool ?: return start
        val lane = opener.subagentId
        val row = ExpToolDisplay.row(opener.name) ?: return start
        var end = start
        while (
            end + 1 < feed.size &&
            isExpToolCall(feed[end + 1], workflowIds) &&
            (feed[end + 1] as AgentFeedItem.Tool).let {
                it.subagentId == lane && ExpToolDisplay.row(it.name) == row
            }
        ) {
            end++
        }
        return end
    }

    /**
     * The caption an [AgentFeedRow.ExpRun] reads: the contract's plural copy for
     * the run's tool, progressive while any member is still in flight, done once
     * they all settled, plus ` · N failed` when members failed. Empty when the
     * items are not ours (unreachable from the projection, which only ever hands
     * this a run it scanned).
     */
    fun expToolGroupCaption(items: List<AgentFeedItem.Tool>): String {
        val row = ExpToolDisplay.row(items.firstOrNull()?.name) ?: return ""
        val at = DomainContract.expToolNames.indexOf(row)
        val running = items.any { !it.settled }
        val captions = if (running) {
            DomainContract.expToolProgressiveMany
        } else {
            DomainContract.expToolDoneMany
        }
        val caption = (captions.getOrNull(at) ?: row).replace("{n}", items.size.toString())
        val failed = items.count { it.failed }
        return if (failed == 0) {
            caption
        } else {
            "$caption${ToolGroupSummary.SEPARATOR}$failed failed"
        }
    }
}
