package com.exponential.app.domain

/**
 * #EXP-1830: the home screen widgets' folds, the Android mirror of
 * `packages/domain-contract/src/home-widgets.ts` (iOS ExpCore
 * `HomeWidgets.swift`), locked by `fixtures/home-widgets.json`
 * (`HomeWidgetsTest`). The widgets (`com.exponential.app.widget`) draw them.
 */
object HomeWidgets {
    const val MY_ISSUES = "My issues"
    const val NEEDS_YOU = "Needs you"
    const val EMPTY_ISSUES = "No open issues"
    const val EMPTY_NEEDS_YOU = "Nothing needs you"
    const val SIGNED_OUT = "Sign in to Exponential"
    const val SEALED_ROW = "Encrypted chat"
    const val BOARD_GALLERY = "Board"

    const val ICON_MY_ISSUES = "nav-my-issues"
    const val ICON_NEEDS_YOU = "nav-needs-you"
    const val ICON_PLAN = "coding-plan"
    const val ICON_QUESTION = "ui-help"
    const val ICON_PR = "pr-open"

    val OPEN_CATEGORIES: List<String> = listOf("started", "unstarted", "backlog")
    val CATEGORY_ORDER: List<String> = listOf("started", "unstarted", "backlog")
    /** contract `issuePriority.displayOrder`. */
    val PRIORITY_ORDER: List<String> get() = DomainContract.issuePriorityDisplayOrder

    const val CAPACITY_SMALL = 3
    const val CAPACITY_MEDIUM = 3
    const val CAPACITY_LARGE = 8

    const val LOOK_PADDING = 14
    const val LOOK_HEADER_GAP = 8
    const val LOOK_ROW_HEIGHT = 28
    const val LOOK_GLYPH = 16

    data class Status(val id: String, val category: String, val color: String, val sortOrder: Double)

    data class Issue(
        val id: String,
        val identifier: String,
        val title: String,
        val statusId: String,
        val boardId: String,
        val teamId: String,
        val priority: String,
        val sortOrder: Double,
        val assigneeId: String?,
        val updatedAt: String,
    )

    data class IssueRow(
        val id: String,
        val identifier: String,
        val title: String,
        val statusCategory: String,
        val statusColor: String,
    )

    data class WidgetList<T>(val count: Int, val rows: List<T>)

    private data class Open(val issue: Issue, val status: Status)

    private fun openIssues(issues: List<Issue>, statuses: List<Status>): List<Open> {
        val byId = statuses.associateBy { it.id }
        return issues.mapNotNull { issue ->
            byId[issue.statusId]?.takeIf { it.category in OPEN_CATEGORIES }?.let { Open(issue, it) }
        }
    }

    private fun row(open: Open) = IssueRow(
        open.issue.id,
        open.issue.identifier,
        open.issue.title,
        open.status.category,
        open.status.color,
    )

    /** TS `indexOf` (an unknown value ranks first, -1). */
    private fun rank(list: List<String>, value: String): Int = list.indexOf(value)

    /** One board's open issues in its reading order. */
    fun boardWidget(boardId: String, statuses: List<Status>, issues: List<Issue>, max: Int): WidgetList<IssueRow> {
        val rows = openIssues(issues.filter { it.boardId == boardId }, statuses).sortedWith(
            compareBy<Open> { rank(CATEGORY_ORDER, it.status.category) }
                .thenBy { it.status.sortOrder }
                .thenBy { it.issue.sortOrder }
                .thenBy { it.issue.identifier },
        )
        return WidgetList(rows.size, rows.take(max).map(::row))
    }

    /** The viewer's open issues in the picked teams (none = every team). */
    fun myIssuesWidget(
        me: String,
        teamIds: List<String>,
        statuses: List<Status>,
        issues: List<Issue>,
        max: Int,
    ): WidgetList<IssueRow> {
        val mine = issues.filter { it.assigneeId == me && (teamIds.isEmpty() || it.teamId in teamIds) }
        val rows = openIssues(mine, statuses).sortedWith(
            compareBy<Open> { rank(PRIORITY_ORDER, it.issue.priority) }
                .thenByDescending { it.issue.updatedAt }
                .thenBy { it.issue.identifier },
        )
        return WidgetList(rows.size, rows.take(max).map(::row))
    }

    data class NeedsYouInput(
        val messageId: String,
        val sessionId: String,
        val rootId: String,
        val kind: String,
        val opens: String,
        val title: String,
        val age: String,
    )

    data class NeedsYouRow(
        val messageId: String,
        val sessionId: String,
        val rootId: String,
        val opens: String,
        val icon: String,
        val title: String,
        val age: String,
    )

    private val KIND_ICON = mapOf("plan" to ICON_PLAN, "question" to ICON_QUESTION, "pr" to ICON_PR)

    /** THE Needs you rows (needs-you.json order), an encrypted chat's text withheld. */
    fun needsYouWidget(rows: List<NeedsYouInput>, sealedRootIds: Collection<String>, max: Int): WidgetList<NeedsYouRow> {
        val sealed = sealedRootIds.toSet()
        return WidgetList(
            rows.size,
            rows.take(max).map { row ->
                NeedsYouRow(
                    messageId = row.messageId,
                    sessionId = row.sessionId,
                    rootId = row.rootId,
                    opens = row.opens,
                    icon = KIND_ICON[row.kind] ?: ICON_QUESTION,
                    title = if (row.rootId in sealed) SEALED_ROW else row.title,
                    age = row.age,
                )
            },
        )
    }
}
