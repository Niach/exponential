package com.exponential.app.ui.threads

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

/** A selection action that failed for [failed] of its [total] chats. */
internal class PartialSelectionFailure(val failed: Int, val total: Int, val first: Throwable) : Exception(first)

/** No signed-in account to act as: the call toasts its refusal, never a silent no-op. */
internal class NoActiveAccount : IllegalStateException("no active account")

/**
 * The selection bar's per-chat calls (`sessions.delete`): one chat at a time,
 * every one tried, then the failures counted ([PartialSelectionFailure]).
 * Once started the batch runs to its end even if the screen goes away (the
 * deletes are already underway on the server), so a selection is never half
 * deleted without a word.
 */
internal suspend fun eachChat(ids: List<String>, block: suspend (String) -> Unit) = withContext(NonCancellable) {
    var first: Throwable? = null
    var failed = 0
    for (id in ids) {
        try {
            block(id)
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            first = first ?: t
            failed += 1
        }
    }
    first?.let { throw PartialSelectionFailure(failed, ids.size, it) }
}
