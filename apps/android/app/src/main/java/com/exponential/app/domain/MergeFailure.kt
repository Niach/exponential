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
    }
}

/**
 * Whether a failed merge may offer the "Fix merge conflicts" run: a real
 * conflict, on a PR whose branch we recorded (the run rebases it), with remote
 * start available.
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
