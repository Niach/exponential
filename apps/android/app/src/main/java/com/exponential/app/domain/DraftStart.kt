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
    val fingerprint: Fingerprint,
) {
    /** The machine the start goes to (part of the fingerprint). */
    val deviceId: String get() = fingerprint.deviceId

    /**
     * What a draft start is "the same" by: every field of
     * `starting.fingerprint.fields` (recheck R2). Lists compare as SETS
     * (order-free), an absent list equals an empty one, text compares
     * verbatim. The attachments are stable per-pick ids, never the uploaded
     * ones, which a send stamps. The launch options are the WIRE form the
     * start sends, so a pick that never reaches the device never mints a new
     * id.
     */
    data class Fingerprint(
        val deviceId: String,
        val text: String,
        val issueIds: Set<String> = emptySet(),
        /** The chat's repository (the hidden Chat builtin's `repo` input). */
        val repositoryId: String? = null,
        val attachments: Set<String> = emptySet(),
        val agent: String? = null,
        val model: String? = null,
        val effort: String? = null,
        val planMode: Boolean? = null,
        val account: String? = null,
        val mcpServerIds: Set<String> = emptySet(),
        val computerUse: Boolean? = null,
        val ultracode: Boolean? = null,
        val subagentModel: String? = null,
        /** The composer's Resume toggle (an absent one = off). */
        val resume: Boolean = false,
    )

    /**
     * A landing of this start clears the draft and opens the session only
     * when the draft is unchanged since its send; a draft the person changed
     * after the timeout stays exactly as they left it (no navigation).
     */
    fun landsInto(current: Fingerprint?): Boolean = fingerprint == current

    companion object {
        /** The start a Start or Retry of [fingerprint] sends: the pending one
         * while the draft (device included) matches, else a fresh one. */
        fun resolve(
            pending: DraftStart?,
            fingerprint: Fingerprint,
            mint: () -> String = { UUID.randomUUID().toString() },
        ): DraftStart =
            if (pending != null && pending.fingerprint == fingerprint) {
                pending
            } else {
                DraftStart(mint(), fingerprint)
            }
    }
}
