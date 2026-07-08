package com.exponential.app.data.electric

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * Live per-account / per-shape sync telemetry powering the Sync Diagnostics
 * screen (parity with iOS `SyncDebug`). Updated by [ShapeClient] on every poll;
 * read reactively by the diagnostics UI.
 */
@Singleton
class SyncStats @Inject constructor() {
    data class ShapeStatus(
        val shape: String,
        val phase: String = "idle", // "initial" | "live" | "idle" | "unauthorized"
        val rowsApplied: Int = 0,
        // Lifetime error tally (kept for logs / historical context).
        val errorCount: Int = 0,
        // Current-health signal: how many polls in a row have failed. Reset to 0
        // on any successful poll, so the diagnostics UI reflects the shape's
        // *current* state rather than a long-gone transient blip.
        val consecutiveErrors: Int = 0,
        // Message from the most recent failing poll (null once a poll succeeds).
        val lastError: String? = null,
        // The last failure was a "no such column/table" class SQLite error.
        val schemaError: Boolean = false,
        // An auto-reset is repopulating this shape from a fresh snapshot.
        val recovering: Boolean = false,
        // Wire columns tolerant-apply dropped (older client vs newer server) —
        // benign, shown as a note, cumulative for the run.
        val droppedColumns: Set<String> = emptySet(),
        // Rows dropped because a full-row payload failed to decode — benign, the
        // row re-syncs on the next refetch.
        val decodeDrops: Int = 0,
    )

    // Mark a shape "unauthorized" once a requireAuth shape has failed auth this
    // many times in a row: enough to be confident it's a real, persistent 401/403
    // rather than a transient hiccup, without hammering or misleading.
    companion object {
        const val UNAUTHORIZED_THRESHOLD = 3
    }

    // accountId -> (shape -> status)
    private val _state = MutableStateFlow<Map<String, Map<String, ShapeStatus>>>(emptyMap())
    val state: StateFlow<Map<String, Map<String, ShapeStatus>>> = _state.asStateFlow()

    private fun mutate(accountId: String, shape: String, fn: (ShapeStatus) -> ShapeStatus) {
        _state.update { all ->
            val account = all[accountId].orEmpty()
            val current = account[shape] ?: ShapeStatus(shape)
            all + (accountId to (account + (shape to fn(current))))
        }
    }

    fun setPhase(accountId: String, shape: String, phase: String) =
        mutate(accountId, shape) {
            // Don't let a routine pre-poll "initial"/"live" phase update clobber a
            // sticky "unauthorized" state; only a successful poll (clearError)
            // clears that. Prevents the diagnostics row from flickering on every
            // retry of a persistently-401 shape.
            if (it.phase == "unauthorized" && phase != "unauthorized") it
            else it.copy(phase = phase)
        }

    fun addRows(accountId: String, shape: String, count: Int) {
        if (count <= 0) return
        mutate(accountId, shape) { it.copy(rowsApplied = it.rowsApplied + count) }
    }

    /**
     * Record a failed poll. [authFailure] is true for HTTP 401/403; once a shape
     * accumulates [UNAUTHORIZED_THRESHOLD] consecutive auth failures it flips to
     * the terminal-looking "unauthorized" phase so it stops reading as a generic
     * (recoverable) error and so the UI can explain it instead of showing a
     * forever-climbing count on a stuck "initial" shape.
     */
    fun incError(
        accountId: String,
        shape: String,
        authFailure: Boolean = false,
        message: String? = null,
        schema: Boolean = false,
    ) =
        mutate(accountId, shape) {
            val consecutive = it.consecutiveErrors + 1
            val phase = if (authFailure && consecutive >= UNAUTHORIZED_THRESHOLD) {
                "unauthorized"
            } else {
                it.phase
            }
            it.copy(
                errorCount = it.errorCount + 1,
                consecutiveErrors = consecutive,
                phase = phase,
                lastError = message ?: it.lastError,
                schemaError = schema,
            )
        }

    /**
     * Clear the *current* error state after a successful poll. The lifetime
     * [errorCount] is intentionally left intact; only the live-health signals
     * ([consecutiveErrors], [lastError], [recovering], and the "unauthorized"
     * phase) are reset.
     */
    fun clearError(accountId: String, shape: String) =
        mutate(accountId, shape) {
            if (it.consecutiveErrors == 0 && it.phase != "unauthorized" &&
                it.lastError == null && !it.recovering
            ) {
                return@mutate it
            }
            it.copy(
                consecutiveErrors = 0,
                phase = if (it.phase == "unauthorized") "live" else it.phase,
                lastError = null,
                schemaError = false,
                recovering = false,
            )
        }

    /** Mark a shape as auto-recovering (offset + rows wiped, awaiting snapshot). */
    fun setRecovering(accountId: String, shape: String) =
        mutate(accountId, shape) { it.copy(recovering = true) }

    /** Record wire columns dropped by tolerant-apply (union, benign). */
    fun reportDropped(accountId: String, shape: String, columns: Set<String>) {
        if (columns.isEmpty()) return
        mutate(accountId, shape) { it.copy(droppedColumns = it.droppedColumns + columns) }
    }

    /** Record a full-row insert dropped because its payload failed to decode. */
    fun reportDecodeDrop(accountId: String, shape: String) =
        mutate(accountId, shape) { it.copy(decodeDrops = it.decodeDrops + 1) }

    fun clearAccount(accountId: String) {
        _state.update { it - accountId }
    }
}
