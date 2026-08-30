package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity

// ---------------------------------------------------------------------------
// Custom issue statuses (EXP-314) — the CROSS-PLATFORM resolution contract.
// The identical layer ships on web, iOS, Android and desktop; change it only
// in lockstep (the clock table below is locked by IssueStatusResolutionTest on
// every client with the same literal name lists).
//
//  1. teamStatuses(rows): order by category display order
//     [backlog, unstarted, started, completed, cancelled, duplicate], then
//     sort_order asc, then created_at asc, then id. A started row's clock
//     position is its index among the started rows in THAT order. A row whose
//     category this client does not know sorts LAST but RENDERS backlog.
//  2. resolve(issue): (a) the team row whose id == issue.statusId,
//     (b) else the team row whose builtin_key == issue.status (the anchor,
//     with an unknown/missing anchor normalized to backlog FIRST so it joins
//     the team's real Backlog row), (c) else a locally CONSTRUCTED default
//     from the generated contract defaults. Rendering can never fail —
//     constructed rows carry the synthetic id "builtin:<key>".
//  3. Builtin + constructed rows render TODAY's design-token colors keyed on
//     the builtin key (see resolvedStatusColor); only CUSTOM rows use the
//     synced hex.
// ---------------------------------------------------------------------------

/** Wire categories from the generated contract (`issueStatusCategoryValues`). */
enum class IssueStatusCategory(val wire: String) {
    Backlog("backlog"),
    Unstarted("unstarted"),
    Started("started"),
    Completed("completed"),
    Cancelled("cancelled"),
    Duplicate("duplicate");

    companion object {
        /** Null-tolerant: an absent or unknown wire value returns null. */
        fun fromWire(value: String?): IssueStatusCategory? =
            entries.firstOrNull { it.wire == value }
    }
}

/**
 * The ONE category order (EXP-448) — issue-list groups, status pickers and the
 * (web/desktop-owned) statuses settings sections all speak it. Locked to
 * `DomainContract.issueStatusCategoryDisplayOrder`.
 */
val issueStatusCategoryDisplayOrder: List<IssueStatusCategory> = listOf(
    IssueStatusCategory.Backlog,
    IssueStatusCategory.Unstarted,
    IssueStatusCategory.Started,
    IssueStatusCategory.Completed,
    IssueStatusCategory.Cancelled,
    IssueStatusCategory.Duplicate,
)

/**
 * A status ready to render: either a synced team row or a constructed builtin
 * default. [id] is the GROUP KEY (row id, or `builtin:<key>` for a constructed
 * fallback) used by grouping, collapse state and the status filter.
 */
data class ResolvedIssueStatus(
    val id: String,
    /** Null for constructed fallbacks — those can only be written as an enum. */
    val rowId: String?,
    val name: String,
    val category: IssueStatusCategory,
    val colorHex: String?,
    /** Non-null on builtin rows + constructed defaults; null on custom rows. */
    val builtinKey: IssueStatus?,
    /** Registry icon name — resolve via `ExpIcons.byName`. */
    val iconName: String,
)

object IssueStatusResolver {

    /** Prefix of a constructed fallback's synthetic id. */
    const val BUILTIN_ID_PREFIX = "builtin:"

    // Declared BEFORE builtinDefaults: an `object`'s properties initialize in
    // declaration order, and building the defaults reads these tables.
    internal val CLOCKS_2 = listOf("progress-2-4", "progress-3-4")
    internal val CLOCKS_3 = listOf("progress-1-4", "progress-2-4", "progress-3-4")
    internal val CLOCKS_4 = listOf("progress-1-5", "progress-2-5", "progress-3-5", "progress-4-5")

    /**
     * The builtin statuses as they exist on EVERY team, built from the
     * generated contract defaults (`DomainContract.issueStatusDefault*`). Used
     * when the issue_statuses shape hasn't synced yet — and, because it runs
     * through the same ordering + clock pipeline, its in_progress / in_review
     * glyphs are exactly the ones the app rendered before EXP-314. EXP-685
     * retired the `todo` builtin, so the `unstarted` CATEGORY (which stays, for
     * custom rows) no longer has a builtin default of its own.
     */
    val builtinDefaults: List<ResolvedIssueStatus> = run {
        val keys = DomainContract.issueStatusDefaultKeys
        val rows = keys.indices.map { i ->
            val key = keys[i]
            RawStatus(
                id = BUILTIN_ID_PREFIX + key,
                rowId = null,
                name = DomainContract.issueStatusDefaultNames[i],
                category = IssueStatusCategory.fromWire(DomainContract.issueStatusDefaultCategories[i]),
                colorHex = DomainContract.issueStatusDefaultColors[i],
                builtinKey = IssueStatus.entries.firstOrNull { it.wire == key },
                sortOrder = DomainContract.issueStatusDefaultSortOrders[i].toDouble(),
                createdAt = "",
            )
        }
        order(rows)
    }

    /**
     * The constructed Backlog default — the terminal fallback of [resolve], and
     * the ONE row that can never be missing. Since EXP-685 `backlog` is also the
     * anchor every unknown wire status normalizes to, so this lookup is what
     * keeps rendering total (web/iOS/desktop degrade to backlog identically).
     */
    val backlogDefault: ResolvedIssueStatus =
        builtinDefaults.firstOrNull { it.builtinKey == IssueStatus.Backlog }
            ?: ResolvedIssueStatus(
                id = BUILTIN_ID_PREFIX + IssueStatus.Backlog.wire,
                rowId = null,
                name = IssueStatus.Backlog.label,
                category = IssueStatusCategory.Backlog,
                colorHex = null,
                builtinKey = IssueStatus.Backlog,
                iconName = iconName(IssueStatusCategory.Backlog),
            )

    /** The team's statuses in canonical display order, glyphs assigned. */
    fun teamStatuses(rows: List<IssueStatusEntity>): List<ResolvedIssueStatus> = order(
        rows.map { row ->
            RawStatus(
                id = row.id,
                rowId = row.id,
                name = row.name,
                category = IssueStatusCategory.fromWire(row.category),
                colorHex = row.color,
                builtinKey = IssueStatus.entries.firstOrNull { it.wire == row.builtinKey },
                sortOrder = row.sortOrder,
                createdAt = row.createdAt,
            )
        }
    )

    /** Resolve one issue against its team's statuses (see the header contract). */
    fun resolve(issue: IssueEntity, team: List<ResolvedIssueStatus>): ResolvedIssueStatus =
        resolve(issue.statusId, issue.status, team)

    fun resolve(
        statusId: String?,
        anchor: String?,
        team: List<ResolvedIssueStatus>,
    ): ResolvedIssueStatus {
        if (!statusId.isNullOrBlank()) {
            team.firstOrNull { it.rowId == statusId }?.let { return it }
        }
        // An unknown / missing anchor (a NEWER server) normalizes to backlog
        // BEFORE the team lookup, so such an issue joins the team's REAL
        // Backlog row instead of spawning a second, constructed group. Only a
        // team with no synced rows at all degrades to the constructed default.
        val key = IssueStatus.fromWire(anchor)
        team.firstOrNull { it.builtinKey == key }?.let { return it }
        // NON-TOTAL by construction: a retired builtin (EXP-685's `todo`) has no
        // default row any more, so this lookup must never be a `first { }`.
        return builtinDefaults.firstOrNull { it.builtinKey == key } ?: backlogDefault
    }

    /**
     * Glyph for a category. Started statuses get a pie-clock whose fill grows
     * with their position among the team's started rows.
     */
    fun iconName(
        category: IssueStatusCategory,
        startedIndex: Int = 0,
        startedCount: Int = 1,
    ): String = when (category) {
        IssueStatusCategory.Backlog -> "circle-dashed"
        IssueStatusCategory.Unstarted -> "circle"
        IssueStatusCategory.Started -> startedClockIcon(startedIndex, startedCount)
        IssueStatusCategory.Completed -> "circle-check"
        IssueStatusCategory.Cancelled -> "circle-x"
        IssueStatusCategory.Duplicate -> "copy"
    }

    /**
     * The started-status clock table (identical on all four clients). Indexes
     * are clamped: a racing create can transiently push a team past the
     * started cap (`DomainContract.issueStatusStartedMax`).
     */
    fun startedClockIcon(index0: Int, count: Int): String = when {
        count <= 2 -> CLOCKS_2[index0.coerceIn(0, CLOCKS_2.lastIndex)]
        count == 3 -> CLOCKS_3[index0.coerceIn(0, CLOCKS_3.lastIndex)]
        else -> CLOCKS_4[index0.coerceIn(0, CLOCKS_4.lastIndex)]
    }

    // A pre-glyph status row (synced or constructed).
    private data class RawStatus(
        val id: String,
        val rowId: String?,
        val name: String,
        val category: IssueStatusCategory?,
        val colorHex: String?,
        val builtinKey: IssueStatus?,
        val sortOrder: Double,
        val createdAt: String,
    ) {
        // An unknown category (a NEWER server) degrades to the backlog
        // treatment for RENDERING: neutral dashed glyph, active sort branch —
        // never a crash.
        val effectiveCategory: IssueStatusCategory get() = category ?: IssueStatusCategory.Backlog

        // …but for ORDERING it sorts LAST, after duplicate — a status this
        // client cannot interpret must not wedge itself into the middle of the
        // team's list. (Same rule on web/iOS/desktop.)
        val categoryRank: Int
            get() = category
                ?.let { issueStatusCategoryDisplayOrder.indexOf(it) }
                ?.takeIf { it >= 0 }
                ?: issueStatusCategoryDisplayOrder.size
    }

    private fun order(rows: List<RawStatus>): List<ResolvedIssueStatus> {
        val sorted = rows.sortedWith(
            compareBy<RawStatus> { it.categoryRank }
                .thenBy { it.sortOrder }
                .thenBy { it.createdAt }
                .thenBy { it.id }
        )
        val startedCount = sorted.count { it.effectiveCategory == IssueStatusCategory.Started }
        var startedIndex = 0
        return sorted.map { row ->
            val category = row.effectiveCategory
            val index = if (category == IssueStatusCategory.Started) startedIndex++ else 0
            ResolvedIssueStatus(
                id = row.id,
                rowId = row.rowId,
                name = row.name,
                category = category,
                colorHex = row.colorHex,
                builtinKey = row.builtinKey,
                iconName = iconName(category, startedIndex = index, startedCount = startedCount),
            )
        }
    }
}
