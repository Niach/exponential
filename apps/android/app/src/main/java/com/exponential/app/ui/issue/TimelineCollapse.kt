package com.exponential.app.ui.issue

import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueEventEntity

// One timeline entry — the synthesized "created the issue" item, a human
// comment, or a synced activity event. Moved out of CommentThread.kt (EXP-240)
// so the pure collapse function below is unit-testable without Compose.
internal sealed interface TimelineItem {
    val createdAt: String
    val id: String

    /** Synthesized first item — never collapses, always pinned before the merge. */
    data class Created(val issue: IssueEntity) : TimelineItem {
        override val createdAt get() = issue.createdAt
        override val id get() = "created-${issue.id}"
    }

    data class Comment(val comment: CommentEntity) : TimelineItem {
        override val createdAt get() = comment.createdAt
        override val id get() = comment.id
    }

    data class Event(val event: IssueEventEntity) : TimelineItem {
        override val createdAt get() = event.createdAt
        override val id get() = event.id
    }
}

// A renderable timeline row after collapsing: either one item, or a run of
// consecutive events folded behind a "Show N activity items" expander.
internal sealed interface TimelineRow {
    data class Single(val item: TimelineItem) : TimelineRow

    /** [runKey] = the run's FIRST event id — stable across sync re-emits. */
    data class CollapsedRun(val runKey: String, val count: Int) : TimelineRow
}

/**
 * Fold runs of MORE THAN TWO consecutive [TimelineItem.Event]s into a single
 * [TimelineRow.CollapsedRun] unless the run's key is in [expandedRuns]. The
 * created item and comments never collapse (they break runs). Pure — the view
 * layer owns [expandedRuns] as remembered state keyed by issue.
 */
internal fun collapseTimeline(
    items: List<TimelineItem>,
    expandedRuns: Set<String>,
): List<TimelineRow> {
    val out = mutableListOf<TimelineRow>()
    var i = 0
    while (i < items.size) {
        val item = items[i]
        if (item !is TimelineItem.Event) {
            out += TimelineRow.Single(item)
            i++
            continue
        }
        var j = i
        while (j < items.size && items[j] is TimelineItem.Event) j++
        val run = items.subList(i, j)
        val runKey = run.first().id
        if (run.size > 2 && runKey !in expandedRuns) {
            out += TimelineRow.CollapsedRun(runKey, run.size)
        } else {
            run.forEach { out += TimelineRow.Single(it) }
        }
        i = j
    }
    return out
}
