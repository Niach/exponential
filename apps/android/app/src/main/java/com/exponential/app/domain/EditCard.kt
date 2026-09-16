package com.exponential.app.domain

/**
 * EXP-916: the ONE "edited files" card a transcript draws for a run of
 * consecutive file edits, and the ONE rule that decides which tool calls form
 * it. Hand-mirrored ×4 (TS `@exp/domain-contract` `edit-card.ts`, web
 * `@exp/ui` `EditedFilesCard`, desktop `domain::edit_card`, iOS
 * `ExpCore/Sources/Domain/EditCard.swift`) and byte-locked by
 * `fixtures/feed/edit-cards.json`, which every client's feed test replays
 * through its OWN [groupFeedRows].
 *
 * Grouping (a render ROW of the feed projection, [AgentFeedRow.Edits]):
 * - an edits row is a MAXIMAL run of consecutive feed items of ONE lane
 *   ([AgentFeedItem.Tool.subagentId], null = the main lane) that are all edit
 *   calls: a tool call whose [AgentFeedItem.Tool.toolKind] is in [KINDS] and
 *   which is not a workflow card's own call (EXP-850 §3 keeps a workflow call
 *   as its own card — on Android a workflow is named by its `callId`, which is
 *   what the TS mirror's `workflowId` field says);
 * - the rule reads ONLY the kind, the tool kind and the workflow set — never
 *   `settled`/`failed`/`diff` — so the card exists before any patch lands and a
 *   later `tool_update` can never re-split it;
 * - ANY other item ends the run: narration, a user turn, a question, a tool of
 *   another kind, a workflow call, another lane's item. "Nothing between" is
 *   literal;
 * - the row's id is its FIRST item's id (stable while a live run grows); the
 *   window start (EXP-783) opens a fresh card at its boundary like every other
 *   group.
 *
 * The card ([editCard]):
 * - one row per PATH: every member's patch is parsed and folded with
 *   [Diff.mergeFilesByPath] (a file touched twice = one row, counts summed,
 *   hunks concatenated); a pathless patch (bare hunks) borrows its call's
 *   `detail`;
 * - a member WITHOUT a patch still has a row on its `detail` — [RowState.PENDING]
 *   while the call runs, [RowState.FAILED] once it settled without one — unless
 *   a patch for that path already exists in the card;
 * - order: the ready rows in first-touch order, THEN the pending/failed stubs
 *   in first-touch order;
 * - [View.liveIndex] names the row of the card's LAST member when that member
 *   is the transcript's live tool row ([liveToolRowId]): the one row a client
 *   opens by itself, the diff inline. Everything else starts collapsed, and a
 *   tap toggles a row in place — a card never navigates anywhere.
 */
object EditCard {

    /** The tool kinds whose calls form an edited-files card. */
    val KINDS: List<String> = listOf("edit", "delete", "move")

    /** How many rows a card lists before it folds the rest behind "N more". */
    const val PREVIEW: Int = DomainContract.diffUiCardPreviewFiles

    enum class RowState(val wire: String) {
        READY("ready"),
        PENDING("pending"),
        FAILED("failed"),
    }

    data class Row(
        val path: String,
        val state: RowState,
        /** The merged patch for the path; null for a pending/failed row. */
        val file: Diff.File?,
    )

    data class View(
        /** `1 file edited` / `N files edited`. */
        val title: String,
        val rows: List<Row>,
        /** The row the client opens by itself, or null. */
        val liveIndex: Int?,
    )

    /**
     * Whether a feed item is a call that belongs in an edited-files card.
     * [workflowIds] are the call ids this screen holds a workflow card for —
     * such a call is its own card and never folds into a run.
     */
    fun isEditCall(item: AgentFeedItem, workflowIds: Set<String> = emptySet()): Boolean {
        if (item !is AgentFeedItem.Tool) return false
        val kind = item.toolKind ?: return false
        if (kind !in KINDS) return false
        return item.callId == null || item.callId !in workflowIds
    }

    /**
     * The inclusive end index of the maximal run of same-lane edit calls that
     * starts at [start] (which must itself be an edit call). A caller's group
     * scan uses it exactly like its tool-run scan.
     */
    fun editRunEnd(
        feed: List<AgentFeedItem>,
        start: Int,
        workflowIds: Set<String> = emptySet(),
    ): Int {
        val lane = (feed.getOrNull(start) as? AgentFeedItem.Tool)?.subagentId
        var end = start
        while (
            end + 1 < feed.size &&
            isEditCall(feed[end + 1], workflowIds) &&
            (feed[end + 1] as AgentFeedItem.Tool).subagentId == lane
        ) {
            end++
        }
        return end
    }

    /** The path a member names: its patch's first file, else its `detail`. */
    private fun itemPath(item: AgentFeedItem.Tool): String? {
        val diff = item.diff
        if (!diff.isNullOrEmpty()) {
            val first = Diff.parse(diff).files.firstOrNull()
            if (first != null && first.path.isNotEmpty()) return first.path
        }
        val detail = item.detail?.trim()
        return if (detail.isNullOrEmpty()) null else detail
    }

    fun editCard(items: List<AgentFeedItem.Tool>, liveItemId: Long? = null): View {
        val ready = mutableListOf<Diff.File>()
        val stubs = LinkedHashMap<String, RowState>()
        for (item in items) {
            val diff = item.diff
            if (!diff.isNullOrEmpty()) {
                for (file in Diff.parse(diff).files) {
                    // A pathless section (hunks with no header) borrows the
                    // call's own subject — the engine names the file in `detail`.
                    val path = file.path.ifEmpty { item.detail?.trim().orEmpty() }
                    if (path.isEmpty()) continue
                    ready.add(if (file.path == path) file else file.copy(path = path))
                }
                continue
            }
            val path = item.detail?.trim()
            if (path.isNullOrEmpty()) continue
            val state = if (item.settled) RowState.FAILED else RowState.PENDING
            // First touch wins the position; a later settle may still flip the
            // state (a pending call that failed without a patch).
            val seen = stubs[path]
            if (seen == null || (state == RowState.FAILED && seen == RowState.PENDING)) {
                stubs[path] = state
            }
        }
        val merged = Diff.mergeFilesByPath(ready)
        val readyPaths = merged.mapTo(mutableSetOf()) { it.path }
        val rows = merged.map { Row(it.path, RowState.READY, it) } +
            stubs.entries
                .filter { it.key !in readyPaths }
                .map { Row(it.key, it.value, null) }
        val last = items.lastOrNull()
        var liveIndex: Int? = null
        if (last != null && liveItemId != null && liveItemId == last.id) {
            val path = itemPath(last)
            val at = if (path == null) -1 else rows.indexOfFirst { it.path == path }
            liveIndex = if (at < 0) null else at
        }
        return View(title = editCardTitle(rows.size), rows = rows, liveIndex = liveIndex)
    }

    /** The card's title — `1 file edited` / `4 files edited`, ×4. */
    fun editCardTitle(count: Int): String = if (count == 1) {
        DomainContract.diffUiEditedFilesOne
    } else {
        DomainContract.diffUiEditedFilesMany.replace("{n}", count.toString())
    }

    /** The fold row under the first [PREVIEW] rows, or null. */
    fun editCardMoreLabel(count: Int): String? {
        val rest = count - PREVIEW
        return if (rest > 0) DomainContract.diffUiMoreFiles.replace("{n}", rest.toString()) else null
    }

    /**
     * The byte-lock projection of a card: `title | path +a -d | path pending |
     * path failed | live=path`. Deliberately ASCII (`-d`, unlike
     * [Diff.deletionsLabel]), like [Diff.render].
     */
    fun renderEditCard(view: View): String {
        val parts = mutableListOf(view.title)
        for (row in view.rows) {
            parts.add(
                if (row.state == RowState.READY && row.file != null) {
                    "${row.path} +${row.file.additions} -${row.file.deletions}"
                } else {
                    "${row.path} ${row.state.wire}"
                },
            )
        }
        val live = view.liveIndex
        if (live != null && live in view.rows.indices) parts.add("live=${view.rows[live].path}")
        return parts.joinToString(" | ")
    }
}
