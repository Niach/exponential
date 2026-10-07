package com.exponential.app.domain

import com.exponential.app.data.api.isConflictError
import com.exponential.app.data.api.trpcErrorMessage

/**
 * EXP-533: why a squash merge was refused, and whether the builtin "Fix merge
 * conflicts" recovery run could help.
 *
 * A merge fails for several unrelated reasons — a real content conflict,
 * branch protection, a stale base, an unconfigured GitHub App, or simply no
 * network — and only the FIRST is something a rebase-and-force-push run can
 * resolve. Offering the run for the others (the issue's complaint: it showed
 * up after a failed merge with no internet at all) sends the user off to burn
 * an agent session on a problem it cannot see.
 */
data class MergeFailure(
    /** The caption rendered on the failing row — already user-presentable. */
    val message: String,
    /** The server answered a real content conflict (HTTP 409). */
    val isConflict: Boolean,
) {
    companion object {
        fun from(error: Throwable, fallback: String): MergeFailure = MergeFailure(
            message = trpcErrorMessage(error, fallback),
            isConflict = isConflictError(error),
        )

        /**
         * EXP-1145: a failed STACK merge. Its refusal is about whichever pull
         * request stopped the chain, not necessarily this one, so it shows the
         * server's message and never offers the recovery run (web parity).
         */
        fun fromStack(error: Throwable): MergeFailure = MergeFailure(
            message = trpcErrorMessage(error, "The stack could not be merged"),
            isConflict = false,
        )
    }
}

/**
 * EXP-1233: ONE plain merge of [issueId]'s pull request refused by a REAL
 * conflict — the one-shot a ViewModel emits so its screen can open the Fix
 * merge conflicts composer exactly once per refusal (a sticky state would
 * re-navigate on every recomposition). The screen applies
 * [canOfferFixConflicts] with what only it knows (branch, remote start).
 */
data class ConflictRefusal(val issueId: String, val failure: MergeFailure)

/**
 * EXP-917/EXP-1233: the ONE gate on the recovery run — a real conflict, on a
 * PR whose branch we recorded (the run rebases it), with remote start
 * available. When it holds, the refused merge OPENS the Agent composer on the
 * Fix merge conflicts builtin at once (seed `conflict = true`); when it does
 * not, the refusal is a toast / row caption. Callers apply it to ISSUE
 * targets only (the builtin takes a representative issue) and never to a
 * stack merge ([MergeFailure.fromStack] is never a conflict). Mirrors web
 * `canOfferFixConflicts`, iOS `canFixConflicts` and desktop
 * `work_header::conflict_opens_composer`.
 *
 * [steerEnabled] defaults to true for the surfaces that carry no separate
 * remote-start gate of their own (Reviews rows, which report an unreachable
 * device through the start sheet instead).
 */
fun canOfferFixConflicts(
    failure: MergeFailure?,
    branch: String?,
    steerEnabled: Boolean = true,
): Boolean = failure?.isConflict == true && !branch.isNullOrBlank() && steerEnabled
