package com.exponential.app.domain

/**
 * EXP-1097: what the issue detail DRAWS for its relations, on every client —
 * the "Sub-issue of" parent line above the title, the Sub-issues section
 * (completion ring + `done/total`, rows, only a `+`), and ONE foldable band
 * per remaining relation side (Blocked by / Blocking / Duplicate of /
 * Duplicated by / Related), which phones draw inside the properties sheet.
 *
 * Pure and FIXTURE-LOCKED ×4
 * (`packages/domain-contract/fixtures/issue-relations-view.json`): web
 * `lib/issue-relations-view.ts`, desktop `domain::relations_view`, iOS
 * `IssueRelationsView.swift` return byte-identical output, copy included.
 *
 * Storage stays canonical-direction: `parent` = issueId is the parent of
 * relatedIssueId, `blocks` = issueId blocks relatedIssueId, `duplicate` =
 * issueId duplicates relatedIssueId, `related` = symmetric. A row whose other
 * end is not synced is dropped; an unknown type folds into Related.
 */
object IssueRelationsView {
    /** A band's side key — its fold state is stored per key. */
    enum class BandKey(val wire: String) {
        BlockedBy("blocked_by"),
        Blocking("blocking"),
        DuplicateOf("duplicate_of"),
        DuplicatedBy("duplicated_by"),
        Related("related");

        companion object {
            fun fromWire(value: String): BandKey? = entries.firstOrNull { it.wire == value }
        }
    }

    data class Issue(
        val id: String,
        val identifier: String,
        val title: String,
        /** The dual-written ANCHOR enum (`issues.status`). */
        val status: String,
    )

    data class Relation(
        val type: String,
        val issueId: String,
        val relatedIssueId: String,
    )

    data class Input(
        val subjectId: String,
        val relations: List<Relation>,
        /** Every synced issue the relations may name (the team's issues). */
        val issues: List<Issue>,
        /** Bands the user folded/unfolded this session, overriding the default. */
        val toggled: Set<BandKey> = emptySet(),
        /** Bands whose "Show N more" was pressed. */
        val showAll: Set<BandKey> = emptySet(),
    )

    data class Row(
        val id: String,
        val identifier: String,
        val title: String,
        val status: String,
        val open: Boolean,
    )

    data class Band(
        val key: BandKey,
        val title: String,
        val count: Int,
        val openCount: Int,
        val expanded: Boolean,
        /** The rows drawn right now (none while folded). */
        val rows: List<Row>,
        /** "Show N more", null when nothing is hidden. */
        val more: String?,
        /** "Show less", only once "Show N more" was pressed and rows exceed the cap. */
        val less: String?,
    )

    data class SubIssues(
        val rows: List<Row>,
        val done: Int,
        val total: Int,
        /** `2/5`, null when there are no sub-issues. */
        val progress: String?,
    )

    data class View(
        /** The "Sub-issue of" line's parent, null when the subject has none. */
        val parent: Row?,
        val subIssues: SubIssues,
        val bands: List<Band>,
        /** Total rows across the bands (the phone sheet's "Relations" count). */
        val relationCount: Int,
    ) {
        companion object {
            val EMPTY = View(null, SubIssues(emptyList(), 0, 0, null), emptyList(), 0)
        }
    }

    // ── Copy (byte-identical ×4) ────────────────────────────────────────
    object Copy {
        const val SUB_ISSUES = "Sub-issues"
        const val SUB_ISSUE_OF = "Sub-issue of"
        const val ADD_SUB_ISSUES = "Add sub-issues"
        const val RELATIONS = "Relations"
        const val ADD = "Add"
        const val BLOCKED_BY = "Blocked by"
        const val BLOCKING = "Blocking"
        const val DUPLICATE_OF = "Duplicate of"
        const val DUPLICATED_BY = "Duplicated by"
        const val RELATED = "Related"
        const val SHOW_LESS = "Show less"
    }

    fun showMore(count: Int): String = "Show $count more"

    fun progress(done: Int, total: Int): String = "$done/$total"

    /** Rows a band shows before "Show N more". */
    const val BAND_CAP = 3

    private val CLOSED_ANCHORS = setOf("done", "cancelled", "duplicate")

    /** Whether an issue on this status anchor reads as OPEN (not closed). */
    fun isOpenAnchor(status: String): Boolean = status !in CLOSED_ANCHORS

    private val BAND_ORDER = listOf(
        BandKey.BlockedBy,
        BandKey.Blocking,
        BandKey.DuplicateOf,
        BandKey.DuplicatedBy,
        BandKey.Related,
    )

    fun bandTitle(key: BandKey): String = when (key) {
        BandKey.BlockedBy -> Copy.BLOCKED_BY
        BandKey.Blocking -> Copy.BLOCKING
        BandKey.DuplicateOf -> Copy.DUPLICATE_OF
        BandKey.DuplicatedBy -> Copy.DUPLICATED_BY
        BandKey.Related -> Copy.RELATED
    }

    private val TRAILING_DIGITS = Regex("(\\d+)$")

    /** The identifier's trailing number, for a natural `EXP-9 < EXP-10` order. */
    private fun identifierNumber(identifier: String): Long =
        TRAILING_DIGITS.find(identifier)?.groupValues?.get(1)?.toLongOrNull() ?: Long.MAX_VALUE

    private val byIdentifier = Comparator<Row> { a, b ->
        val byNumber = identifierNumber(a.identifier).compareTo(identifierNumber(b.identifier))
        if (byNumber != 0) byNumber else a.identifier.compareTo(b.identifier).coerceIn(-1, 1)
    }

    /** Open rows first, each half in identifier order. */
    private val openFirst = Comparator<Row> { a, b ->
        if (a.open != b.open) (if (a.open) -1 else 1) else byIdentifier.compare(a, b)
    }

    private fun toRow(issue: Issue) = Row(
        id = issue.id,
        identifier = issue.identifier,
        title = issue.title,
        status = issue.status,
        open = issue.status !in CLOSED_ANCHORS,
    )

    fun build(input: Input): View {
        val byId = input.issues.associateBy { it.id }
        val subject = input.subjectId
        var parent: Row? = null
        val children = ArrayList<Row>()
        val buckets = LinkedHashMap<BandKey, MutableList<Row>>()
        val seen = HashSet<String>()

        fun push(key: BandKey, row: Row) {
            if (!seen.add("${key.wire}:${row.id}")) return
            buckets.getOrPut(key) { ArrayList() }.add(row)
        }

        for (relation in input.relations) {
            val forward = relation.issueId == subject
            val inverse = relation.relatedIssueId == subject
            if (forward == inverse) continue // not about the subject (or a self-loop)
            val other = byId[if (forward) relation.relatedIssueId else relation.issueId] ?: continue
            val row = toRow(other)
            when (relation.type) {
                "parent" -> if (forward) {
                    if (seen.add("child:${row.id}")) children.add(row)
                } else {
                    val current = parent
                    if (current == null || byIdentifier.compare(row, current) < 0) parent = row
                }
                "blocks" -> push(if (forward) BandKey.Blocking else BandKey.BlockedBy, row)
                "duplicate" -> push(if (forward) BandKey.DuplicateOf else BandKey.DuplicatedBy, row)
                else -> push(BandKey.Related, row)
            }
        }

        val sortedChildren = children.sortedWith(byIdentifier)
        val done = sortedChildren.count { !it.open }

        val bands = BAND_ORDER.filter { it in buckets }.map { key ->
            val all = buckets.getValue(key).sortedWith(openFirst)
            val openCount = all.count { it.open }
            // Blockers are the actionable relation: they open by default while
            // one is still open. Duplicates and Related stay folded.
            val openByDefault =
                (key == BandKey.BlockedBy || key == BandKey.Blocking) && openCount > 0
            val expanded = if (key in input.toggled) !openByDefault else openByDefault
            val everything = key in input.showAll
            val overflow = all.size > BAND_CAP
            val rows = when {
                !expanded -> emptyList()
                everything || !overflow -> all
                else -> all.take(BAND_CAP)
            }
            Band(
                key = key,
                title = bandTitle(key),
                count = all.size,
                openCount = openCount,
                expanded = expanded,
                rows = rows,
                more = if (expanded && overflow && !everything) showMore(all.size - BAND_CAP) else null,
                less = if (expanded && overflow && everything) Copy.SHOW_LESS else null,
            )
        }

        return View(
            parent = parent,
            subIssues = SubIssues(
                rows = sortedChildren,
                done = done,
                total = sortedChildren.size,
                progress = if (sortedChildren.isNotEmpty()) progress(done, sortedChildren.size) else null,
            ),
            bands = bands,
            relationCount = bands.sumOf { it.count },
        )
    }
}
