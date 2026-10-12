package com.exponential.app.domain

import java.util.UUID

/**
 * feedback-states.json `starting` (final review): ONE `startId` per DRAFT
 * START, bound to the target device + the draft fingerprint. Minted on the
 * first Start of a draft and reused by every Retry AND every later Start of
 * the SAME draft on the SAME device until a session lands or the draft or
 * device changes (then a fresh id). The server and the device dedupe the id,
 * so a slow device never ends up with two sessions. Mirrored on iOS
 * (`DraftStart.swift`).
 */
data class DraftStart(
    val startId: String,
    val deviceId: String,
    val fingerprint: Fingerprint,
) {
    /**
     * What a draft start is "the same" by: the text, the subject's issue /
     * action ids and the attachments (stable per-pick ids, never the uploaded
     * ones, which a send stamps).
     */
    data class Fingerprint(
        val text: String,
        val issueIds: List<String>,
        val actionId: String?,
        val attachments: List<String>,
    )

    /**
     * A landing of this start clears the draft and opens the session only
     * when the draft is unchanged since its send; a draft the person changed
     * after the timeout stays exactly as they left it (no navigation).
     */
    fun landsInto(current: Fingerprint): Boolean = fingerprint == current

    companion object {
        /** The start a Start or Retry of [fingerprint] on [deviceId] sends:
         * the pending one while device and draft match, else a fresh one. */
        fun resolve(
            pending: DraftStart?,
            deviceId: String,
            fingerprint: Fingerprint,
            mint: () -> String = { UUID.randomUUID().toString() },
        ): DraftStart =
            if (pending != null && pending.deviceId == deviceId && pending.fingerprint == fingerprint) {
                pending
            } else {
                DraftStart(mint(), deviceId, fingerprint)
            }
    }
}
