package com.exponential.app.domain

import com.exponential.app.data.db.ReleaseEntity

// Release progress + ordering (EXP-56 §10.2/§10.3) — the cross-platform
// contract, mirroring apps/web/src/lib/releases.ts exactly. Pure functions
// shared by the releases list, the release detail, and the issue picker.

/**
 * Progress over a release's member issues. `cancelled`/`duplicate` issues are
 * *dropped*, not shipped — they leave the denominator instead of counting as
 * progress.
 */
data class ReleaseProgress(
    val total: Int,
    val done: Int,
    val dropped: Int,
    val denominator: Int,
    val fraction: Float,
    /**
     * "Ready to ship": every non-dropped issue is done. Independent of
     * shipped_at — shipping early (or an empty release) is allowed, it just
     * never reads as Ready.
     */
    val isComplete: Boolean,
)

/** Compute progress from the member issues' raw status strings. */
fun releaseProgress(statuses: List<String>): ReleaseProgress {
    val total = statuses.size
    var done = 0
    var dropped = 0
    for (status in statuses) {
        when (status) {
            IssueStatus.Done.wire -> done += 1
            IssueStatus.Cancelled.wire, IssueStatus.Duplicate.wire -> dropped += 1
        }
    }
    val denominator = total - dropped
    return ReleaseProgress(
        total = total,
        done = done,
        dropped = dropped,
        denominator = denominator,
        fraction = if (denominator > 0) done.toFloat() / denominator else 0f,
        isComplete = denominator > 0 && done == denominator,
    )
}

/** "N of M done" — denominator excludes cancelled + duplicate. */
fun releaseProgressText(progress: ReleaseProgress): String =
    if (progress.total == 0) "No issues" else "${progress.done} of ${progress.denominator} done"

/**
 * Canonical release ordering: unshipped before shipped; unshipped by
 * targetDate asc with nulls LAST (a dated release is more urgent than an
 * undated one) then createdAt desc; shipped by shippedAt desc (most recently
 * shipped first). String comparisons are safe — targetDate is a plain DATE and
 * the timestamps arrive as sortable ISO-8601 text.
 */
val releaseComparator: Comparator<ReleaseEntity> = Comparator { a, b ->
    val aShipped = a.shippedAt != null
    val bShipped = b.shippedAt != null
    when {
        aShipped != bShipped -> if (aShipped) 1 else -1
        aShipped && bShipped -> {
            val byShipped = (b.shippedAt ?: "").compareTo(a.shippedAt ?: "")
            if (byShipped != 0) byShipped else b.createdAt.compareTo(a.createdAt)
        }
        else -> {
            val aDate = a.targetDate
            val bDate = b.targetDate
            when {
                aDate != null && bDate != null && aDate != bDate -> aDate.compareTo(bDate)
                aDate != null && bDate == null -> -1
                aDate == null && bDate != null -> 1
                else -> b.createdAt.compareTo(a.createdAt)
            }
        }
    }
}
