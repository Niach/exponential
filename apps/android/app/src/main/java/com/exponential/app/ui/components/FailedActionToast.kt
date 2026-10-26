package com.exponential.app.ui.components

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.ToastAction

// FEED-79 (EXP-1259 D10): Android's ONE failed-action surface (web
// `lib/action-error.ts` `failedAction`). A failed save/delete/start/connect/
// merge/send/invite is a toast titled `Couldn't <verb> <thing>` with the
// server's message as its description and Retry only when re-running the same
// call is safe. Invalid input stays inline under its field; nothing else
// renders an error inline.

/**
 * Toast a failed action: `Couldn't <verb> <thing>` + [message], Retry when
 * [retry] is given ([retryLabel] renames it: the merge check gate's `Merge
 * anyway`, EXP-1378).
 */
fun Toaster.failedAction(
    verb: String,
    thing: String,
    message: String? = null,
    retryLabel: String = FeedbackStates.RETRY,
    retry: (() -> Unit)? = null,
): String = this.error(
    FeedbackStates.errorTitle(verb, thing),
    description = message?.trim()?.takeIf { it.isNotEmpty() },
    action = retry?.let { ToastAction(retryLabel, it) },
)

/**
 * A failed action a ViewModel hands its screen (it has no [Toaster]): the
 * screen raises it with [FailedActionToast] and clears it.
 */
data class FailedAction(
    val verb: String,
    val thing: String,
    val message: String? = null,
    /** The action's label: Retry, or the merge check gate's `Merge anyway`. */
    val retryLabel: String = FeedbackStates.RETRY,
    val retry: (() -> Unit)? = null,
)

/** The one-shot bridge for a [FailedAction] slot: toast once, then [onShown] clears it. */
@Composable
fun FailedActionToast(failure: FailedAction?, onShown: () -> Unit) {
    val toaster = LocalToaster.current
    val currentOnShown by rememberUpdatedState(onShown)
    LaunchedEffect(failure) {
        val f = failure ?: return@LaunchedEffect
        toaster.failedAction(f.verb, f.thing, f.message, f.retryLabel, f.retry)
        currentOnShown()
    }
}

/**
 * [FailedActionToast] for a ViewModel several composables watch at once: only
 * the caller whose [claim] (the VM's atomic compare-and-clear) wins toasts.
 */
@Composable
fun ClaimedFailedActionToast(failure: FailedAction?, claim: (FailedAction) -> Boolean) {
    val toaster = LocalToaster.current
    val currentClaim by rememberUpdatedState(claim)
    LaunchedEffect(failure) {
        val f = failure ?: return@LaunchedEffect
        if (currentClaim(f)) toaster.failedAction(f.verb, f.thing, f.message, f.retryLabel, f.retry)
    }
}

/**
 * The one-shot bridge for a ViewModel's `error: String?`: when [error] turns
 * non-null it raises [Toaster.failedAction] once and calls [onShown] (the VM's
 * clear), so the toast never repeats on recomposition.
 */
@Composable
fun FailedActionToast(
    error: String?,
    verb: String,
    thing: String,
    onShown: () -> Unit,
    retry: (() -> Unit)? = null,
) {
    val toaster = LocalToaster.current
    val currentRetry by rememberUpdatedState(retry)
    val currentOnShown by rememberUpdatedState(onShown)
    LaunchedEffect(error) {
        val message = error ?: return@LaunchedEffect
        toaster.failedAction(verb, thing, message, retry = currentRetry)
        currentOnShown()
    }
}

/**
 * INVALID INPUT (FEED-79): the one line under its field, the rose foreground
 * at `errors.inline.size` (13sp); nothing when [message] is null. The only
 * error that renders inline.
 */
@Composable
fun FieldError(message: String?, modifier: Modifier = Modifier) {
    if (message.isNullOrBlank()) return
    Text(
        message,
        style = MaterialTheme.typography.bodySmall.copy(fontSize = 13.sp, lineHeight = 18.sp),
        color = MaterialTheme.colorScheme.error,
        modifier = modifier,
    )
}
