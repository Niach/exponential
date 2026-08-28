package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.DeviceOwner
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.DeviceWorktreeEntity
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json

// EXP-481: the synced devices/device_worktrees rows → the SteerDevice shape
// every existing surface renders. Online-ness derives CLIENT-side from
// last_seen_at freshness (machines heartbeat ~30s; the relay presence merge
// is the OLD clients' path) — recomputed on a 30s ticker, since Room flows
// only re-emit on writes.

/** Tolerant decode config for the jsonb-as-text columns DeviceEntity holds. */
private val deviceJson = Json { ignoreUnknownKeys = true }

object DeviceLiveness {

    /**
     * Whether [lastSeenAt] is fresh enough to render "online" — within the
     * contract window (`device.onlineWindowSeconds`, three missed 30s beats).
     * FAIL-CLOSED, unlike session liveness: an unparseable timestamp reads
     * OFFLINE — an unstartable claim is the safe direction for a start target.
     * Negative ages (server stamp ahead of this device's clock) clamp online.
     */
    fun isOnline(lastSeenAt: String?, nowMs: Long = System.currentTimeMillis()): Boolean {
        val seen = lastSeenAt?.let(WireTimestamps::parseEpochMs) ?: return false
        return nowMs - seen < DomainContract.deviceOnlineWindowMs
    }

    /**
     * Cold clock for combine()-based online recomputation — the
     * CodingSessionLiveness.minuteTicker idiom at the device window's cadence
     * (30s against the 90s window; a minute tick could lag a state flip by
     * two-thirds of the window).
     */
    fun ticker(intervalMs: Long = 30_000L): Flow<Long> = flow {
        while (true) {
            emit(System.currentTimeMillis())
            delay(intervalMs)
        }
    }
}

/**
 * EXP-656: is what we know about device presence worth rendering?
 *
 * `last_seen_at` only ever moves FORWARD, so a devices cursor we haven't
 * refreshed can produce a false OFFLINE (the machine has been beating the
 * whole time, we just didn't hear it) but never a false online. That is
 * exactly the "my session says Paused for the first 5-10s after I open the
 * app" report — and the same is true of a phone with no network at all.
 * Presence is therefore only trustworthy while our OWN `devices` shape has
 * completed a poll inside the contract window; outside it the answer is
 * UNKNOWN, and unknown must never render as paused.
 */
object DeviceFreshness {

    /**
     * [polledAtMs] and [nowMs] are the same monotonic clock
     * (`SystemClock.elapsedRealtime`); 0 means the shape has never completed a
     * poll on this run, which is never trustworthy.
     */
    fun isTrustworthy(
        polledAtMs: Long,
        nowMs: Long,
        windowMs: Long = DomainContract.deviceOnlineWindowMs,
    ): Boolean = polledAtMs > 0L && nowMs - polledAtMs < windowMs
}

/**
 * EXP-623: stable device-list order — online machines first, sorted by label
 * so heartbeats can't reorder them; offline rows don't beat, so last-seen
 * desc is stable there. Ties break on deviceId.
 */
fun stableDeviceOrder(nowMs: Long): Comparator<DeviceEntity> = Comparator { a, b ->
    val aOnline = DeviceLiveness.isOnline(a.lastSeenAt, nowMs)
    val bOnline = DeviceLiveness.isOnline(b.lastSeenAt, nowMs)
    if (aOnline != bOnline) return@Comparator if (aOnline) -1 else 1
    if (!aOnline) {
        // ISO stamps order lexicographically — desc = most recent first.
        val byLastSeen = (b.lastSeenAt ?: "").compareTo(a.lastSeenAt ?: "")
        if (byLastSeen != 0) return@Comparator byLastSeen
    }
    val byLabel = a.label.lowercase().compareTo(b.label.lowercase())
    if (byLabel != 0) byLabel else a.deviceId.compareTo(b.deviceId)
}

/** A stored jsonb string array (`agents`, `caps`, …) → its list; null/bad = null. */
private fun parseStringList(raw: String?): List<String>? =
    raw?.let {
        runCatching {
            deviceJson.decodeFromString(ListSerializer(String.serializer()), it)
        }.getOrNull()
    }

/** The stored `launch_defaults` jsonb object → the shared DTO; null/bad = null. */
fun parseLaunchDefaults(raw: String?): DeviceLaunchDefaults? =
    raw?.let {
        runCatching {
            deviceJson.decodeFromString(DeviceLaunchDefaults.serializer(), it)
        }.getOrNull()
    }

/**
 * EXP-484: the stored `agent_accounts` jsonb object → per-agent sign-in
 * status; null/bad = null (the Agents section then reads "unknown").
 */
fun parseAgentAccounts(raw: String?): Map<String, AgentAccount>? =
    AgentUsagePresentation.parseAccounts(raw)

/** The stored `agent_usage` jsonb object → per-agent usage; null/bad = null. */
fun parseAgentUsage(raw: String?): Map<String, AgentUsage>? =
    // The tolerant presentation parser: one malformed window drops that
    // window, never the whole map (the session view already parses this way).
    AgentUsagePresentation.parseUsageMap(raw)

/**
 * One synced devices row as the [SteerDevice] every picker/list renders.
 * [ownerName] is the resolved display name for a TEAMMATE's shared row (the
 * owner is always inside the users shape — a sharing owner is a member of the
 * sharing team); own rows pass null and keep `owner == null`, preserving the
 * `isMine` contract. `unauthedAgents` is live-advertisement data the row
 * doesn't carry for OTHER reasons than sign-out — the persisted copy is
 * synced, so it maps through.
 */
fun DeviceEntity.toSteerDevice(
    nowMs: Long,
    currentUserId: String?,
    ownerName: String? = null,
): SteerDevice = SteerDevice(
    deviceId = deviceId,
    deviceLabel = label,
    agents = parseStringList(agents),
    unauthedAgents = parseStringList(unauthedAgents).orEmpty(),
    caps = parseStringList(caps),
    launchDefaults = parseLaunchDefaults(launchDefaults),
    kind = kind,
    platform = platform,
    online = DeviceLiveness.isOnline(lastSeenAt, nowMs),
    lastSeenAt = lastSeenAt,
    registered = true,
    version = version,
    updateRequested = updateRequestedAt != null,
    updateBlocked = updateRequestedAt != null && activeSessions > 0,
    sharedTeamId = sharedTeamId,
    owner = if (userId == currentUserId || currentUserId == null) {
        null
    } else {
        DeviceOwner(id = userId, name = ownerName ?: "")
    },
    // EXP-622: a default belongs to the row's OWNER — never surface a
    // teammate's shared server as the caller's default.
    isDefault = isDefault && userId == currentUserId,
    agentAccounts = parseAgentAccounts(agentAccounts),
    agentUsage = parseAgentUsage(agentUsage),
    agentUsageAt = agentUsageAt,
    rowId = id,
)

/**
 * The synced worktree row that makes "Resume previous session" offerable
 * (EXP-481): same device row, [issueIdentifier] equal case-insensitively, and
 * an agents marker that is NULL (pre-marker worktree — any agent may resume)
 * or contains [agent]. Null when nothing matches — the toggle simply doesn't
 * render, and a stale row degrades device-side (the launcher falls back to a
 * fresh session seeded with a resume prompt).
 */
fun resumeWorktreeFor(
    worktrees: List<DeviceWorktreeEntity>,
    deviceRowId: String?,
    issueIdentifier: String?,
    agent: String,
): DeviceWorktreeEntity? {
    if (deviceRowId == null || issueIdentifier.isNullOrBlank()) return null
    return worktrees.firstOrNull { wt ->
        wt.deviceRowId == deviceRowId &&
            wt.issueIdentifier?.equals(issueIdentifier, ignoreCase = true) == true &&
            (wt.agents == null || parseStringList(wt.agents)?.contains(agent) != false)
    }
}
