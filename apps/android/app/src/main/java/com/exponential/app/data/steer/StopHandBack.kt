package com.exponential.app.data.steer

/**
 * EXP-1342 ("one message on Stop" ×4): a `sessions.stop` that bypasses the
 * relay (the thread header's Stop, the composer's Stop while disconnected)
 * hands the page's queued messages back to its composer FIRST, exactly as
 * [SteerConnection.interrupt] does — and puts the draft back the way it was
 * if the stop itself fails (web `session-stop.ts`: `noteStopped`, then
 * `setDraftText(draftBefore)` on a throw). A null [connection] = no screen
 * holds the session, so there is no queue bar to hand back.
 */
suspend fun stopHandingBackQueue(connection: SteerConnection?, stop: suspend () -> Unit) {
    val draftBefore = connection?.draft?.value
    val tookQueue = connection?.restoreQueueToDraft() == true
    try {
        stop()
    } catch (t: Throwable) {
        if (tookQueue && draftBefore != null) connection.setDraft(draftBefore)
        throw t
    }
}
