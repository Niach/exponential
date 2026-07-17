package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import java.time.Instant
import java.time.OffsetDateTime
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

// EXP-153: client-side staleness guard for `running` coding_sessions rows. A
// row whose synced `updated_at` (heartbeat-advanced by the desktop) is older
// than the contract stale window renders as ABSENT — mirroring the server
// sweep's DELETE (never as `ended`, that flip is the desktop kill-switch
// signal) — so a crashed desktop can't pin a phantom "coding now" badge when
// the sweep lags or isn't running.
object CodingSessionLiveness {

    // Tolerant ISO-8601 → epoch ms. `Instant.parse` needs the `Z`/offset RFC
    // 3339 form; `OffsetDateTime` additionally accepts `+00:00`-style offsets
    // some serializers emit.
    fun parseEpochMs(iso: String): Long? =
        runCatching { Instant.parse(iso).toEpochMilli() }
            .recoverCatching { OffsetDateTime.parse(iso).toInstant().toEpochMilli() }
            .getOrNull()

    // Unparseable liveness signal ⇒ live (fail-open: never hide a session the
    // server still considers alive; the sweep is the backstop).
    fun isStale(updatedAt: String, nowMs: Long = System.currentTimeMillis()): Boolean {
        val seen = parseEpochMs(updatedAt) ?: return false
        return nowMs - seen >= DomainContract.codingSessionStaleMs
    }

    fun isLive(session: CodingSessionEntity, nowMs: Long = System.currentTimeMillis()): Boolean =
        session.status == DomainContract.codingSessionStatusRunning &&
            !isStale(session.updatedAt, nowMs)

    // Cold minute clock for combine()-based re-evaluation — Room flows only
    // re-emit on writes, so a badge would otherwise outlive its window until
    // the next sync delta.
    fun minuteTicker(): Flow<Long> = flow {
        while (true) {
            emit(System.currentTimeMillis())
            delay(60_000)
        }
    }
}
