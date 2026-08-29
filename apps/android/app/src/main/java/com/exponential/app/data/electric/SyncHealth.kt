package com.exponential.app.data.electric

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.update

/**
 * EXP-533: per-account aggregate sync health — the pure model behind the
 * offline banner. Direct port of the proven iOS `SyncDebug` / desktop
 * `crates/sync/src/health.rs` machinery, minus the `unauthorized` case: a hard
 * 401 already owns its own surface here (`SessionInvalidator` signs the
 * account out and the app routes to login), so this model only ever answers
 * "can't reach the server" — it must never double-report auth.
 *
 * Times are WALL clock ([System.currentTimeMillis]), NOT
 * `SystemClock.elapsedRealtime`: the 300s staleness window exists exactly for
 * suspend gaps, and a clock that pauses in deep sleep (which elapsedRealtime
 * does on some OEMs, and which a coroutine `delay` always does) would make an
 * hours-old streak look fresh on wake — the precise bug the window prevents.
 * Clock skew saturates to zero.
 *
 * Deliberately NOT part of [SyncStats]: that is per-SHAPE diagnostics measured
 * on elapsedRealtime and rendered by the Sync Diagnostics screen; this is one
 * aggregate per ACCOUNT and drives a banner.
 */
enum class SyncHealth {
    Ok,

    /**
     * The failure streak persisted past [FAILURE_STREAK_GRACE_MS] — the server
     * is unreachable and the boards are showing cached data.
     */
    Offline,
}

/**
 * How long a failure streak must persist (with no intervening success) before
 * the banner may alarm. TIME-based by design (EXP-44): on app wake every shape
 * long-poll fails simultaneously before the first fresh success, so any
 * consecutive-failure COUNT would trip instantly on healthy servers.
 */
const val FAILURE_STREAK_GRACE_MS = 12_000L

/**
 * An error older than this no longer alarms ([AccountHealth.health]'s
 * staleness guard), and a failure GAP this long breaks the streak's
 * continuity. While genuinely failing, the shape loops report at most ~30s
 * apart (their backoff cap) — a far longer gap means they weren't running (the
 * phone slept mid-outage), so the wake burst's first fresh failure must
 * RESTART the debounce instead of inheriting an hours-old streak start (which
 * would flash the banner immediately on resume).
 */
const val ERROR_STALENESS_WINDOW_MS = 300_000L

/** How often [SyncHealthTracker.activeHealth] re-evaluates while a streak is open. */
private const val HEALTH_TICK_MS = 2_000L

/**
 * One account's aggregate poll health. Every successful poll (row batch or
 * idle `up-to-date` heartbeat — both are 2xx) records a success; every failed
 * poll records a failure. Keyed per account by [SyncHealthTracker] — only the
 * ACTIVE account's entry may drive the banner (a background account's outage
 * must never alarm while the active one syncs fine).
 *
 * Immutable: [recordSuccess] / [recordFailure] return the next value, so the
 * whole model is a pure function of the poll timeline.
 */
data class AccountHealth(
    val lastSuccessAtMs: Long? = null,
    val lastErrorAtMs: Long? = null,
    /**
     * Start of the CURRENT uninterrupted failure streak: set on the first
     * failure after a success (or ever), left alone while failures repeat,
     * cleared by ANY success, and RESTARTED when a failure lands after an
     * [ERROR_STALENESS_WINDOW_MS]-sized quiet gap.
     */
    val failureStreakStartedAtMs: Long? = null,
    /** The most recent failure's display string, for diagnostics. */
    val lastError: String? = null,
) {
    fun recordSuccess(nowMs: Long): AccountHealth =
        copy(lastSuccessAtMs = nowMs, failureStreakStartedAtMs = null)

    fun recordFailure(nowMs: Long, error: String): AccountHealth = copy(
        failureStreakStartedAtMs = if (streakBroken(nowMs)) nowMs else failureStreakStartedAtMs,
        lastErrorAtMs = nowMs,
        lastError = error,
    )

    /**
     * PURE READ — mirrors the iOS/desktop rule exactly. All state mutation
     * stays in the `record*` methods; render paths re-evaluate this freely.
     */
    fun health(nowMs: Long): SyncHealth {
        val errorAt = lastErrorAtMs ?: return SyncHealth.Ok
        // ANY success after the last failure clears instantly.
        if (lastSuccessAtMs != null && lastSuccessAtMs > errorAt) return SyncHealth.Ok
        // Staleness guard: an error that stopped repeating long ago (the shape
        // loops died with the phone asleep) mustn't alarm on wake.
        if (elapsed(errorAt, nowMs) >= ERROR_STALENESS_WINDOW_MS) return SyncHealth.Ok
        // Alarm only once the streak persisted through the grace window — the
        // wake-up burst resolves via a 2xx (streak cleared) well inside it,
        // while a genuine outage keeps the streak alive.
        val startedAt = failureStreakStartedAtMs ?: return SyncHealth.Ok
        return if (elapsed(startedAt, nowMs) >= FAILURE_STREAK_GRACE_MS) {
            SyncHealth.Offline
        } else {
            SyncHealth.Ok
        }
    }

    /**
     * Whether a failure streak is currently open — i.e. [health] can change
     * on the clock alone and so has to be re-evaluated on a timer.
     */
    val streakOpen: Boolean get() = failureStreakStartedAtMs != null

    /** Whether a fresh failure starts a NEW streak instead of extending the current one. */
    private fun streakBroken(nowMs: Long): Boolean {
        if (failureStreakStartedAtMs == null) return true
        val previousErrorAt = lastErrorAtMs ?: return true
        return elapsed(previousErrorAt, nowMs) >= ERROR_STALENESS_WINDOW_MS
    }
}

/** `now - t`, saturating to zero on clock skew. */
private fun elapsed(t: Long, nowMs: Long): Long = (nowMs - t).coerceAtLeast(0L)

/**
 * The live [AccountHealth] of every signed-in account, fed by the shape
 * pipelines in [SyncManager] and read by the app shell's offline banner.
 */
@Singleton
class SyncHealthTracker @Inject constructor() {

    private val _state = MutableStateFlow<Map<String, AccountHealth>>(emptyMap())
    val state: StateFlow<Map<String, AccountHealth>> = _state.asStateFlow()

    fun recordSuccess(accountId: String, nowMs: Long = System.currentTimeMillis()) {
        _state.update { all ->
            all + (accountId to (all[accountId] ?: AccountHealth()).recordSuccess(nowMs))
        }
    }

    fun recordFailure(
        accountId: String,
        error: String,
        nowMs: Long = System.currentTimeMillis(),
    ) {
        _state.update { all ->
            all + (accountId to (all[accountId] ?: AccountHealth()).recordFailure(nowMs, error))
        }
    }

    fun clearAccount(accountId: String) {
        _state.update { it - accountId }
    }

    /**
     * The health of whichever account is active right now. Re-evaluated on
     * every recorded poll and, while a streak is open, every
     * [HEALTH_TICK_MS] — the grace and staleness edges are crossings of the
     * clock, not of an event, so nothing else would ever emit them.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    fun activeHealth(activeAccountId: Flow<String?>): Flow<SyncHealth> =
        combine(activeAccountId, _state) { accountId, all ->
            accountId?.let { all[it] } ?: AccountHealth()
        }
            .distinctUntilChanged()
            .flatMapLatest { health ->
                if (!health.streakOpen) {
                    flowOf(health.health(System.currentTimeMillis()))
                } else {
                    flow {
                        while (true) {
                            emit(health.health(System.currentTimeMillis()))
                            delay(HEALTH_TICK_MS)
                        }
                    }
                }
            }
            .distinctUntilChanged()
}
