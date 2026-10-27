package com.exponential.app.domain

import com.exponential.app.data.api.trpcErrorMessage

/**
 * EXP-1391: what a refused squash merge means for the UI.
 *
 * The refusal is a toast (`Couldn't merge the PR` / `the stack`, the server's
 * message verbatim, Retry) and the server posts it into the thread's root
 * session, whose agent takes it over (rebase, fix, wait). Nothing reads a
 * conflict any more: the Fix conflicts run is gone ×4. Mirrors web
 * `mergeFailure` (feedback-states.json `errors.mergeRefusal`).
 */
data class MergeFailure(
    /** The caption rendered on the failing row — already user-presentable. */
    val message: String,
) {
    companion object {
        fun from(error: Throwable, fallback: String): MergeFailure = MergeFailure(
            message = trpcErrorMessage(error, fallback),
        )

        /**
         * EXP-1145: a failed STACK merge. Its refusal is about whichever pull
         * request stopped the chain, not necessarily this one, so it shows the
         * server's message.
         */
        fun fromStack(error: Throwable): MergeFailure = from(error, "The stack could not be merged")
    }
}
