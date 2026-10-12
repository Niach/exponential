package com.exponential.app.domain

import java.util.UUID

/**
 * EXP-1259 (start surfaces): every remote start OTHER than the Agent
 * composer's draft start ([DraftStart]) carries ONE `startId` per PRESS: a
 * Resume, an account-switch resume. The press mints a fresh id, that press's
 * Retry (the failure toast's) re-sends the SAME id, so the server and the
 * device start it once (feedback-states.json `starting.dedupeMs`). A new press
 * is a new intent and mints again. Bound to its [target] (what the press
 * starts, on which machine, under which login) so a Retry can never re-send an
 * id minted for something else. Mirrored on iOS (`StartIntent.swift`).
 */
data class StartIntent(val startId: String, val target: String) {
    companion object {
        fun mint(): String = UUID.randomUUID().toString()

        /** A press: a new intent, a fresh id. */
        fun press(target: String, mint: () -> String = ::mint): StartIntent =
            StartIntent(mint(), target)

        /** A Retry of [pending]: the same id while it starts the same
         * [target], else (nothing pending, another target) a press. */
        fun retry(pending: StartIntent?, target: String, mint: () -> String = ::mint): StartIntent =
            if (pending != null && pending.target == target) pending else press(target, mint)

        /** The target of a resume (`steer.startSession({ resumeSessionId })`):
         * the run, its machine and the login it re-enters under (null = the
         * recorded one; a switch names another). */
        fun resumeTarget(sessionId: String, deviceId: String, account: String?): String =
            "resume:$sessionId:$deviceId:${account.orEmpty()}"
    }
}
