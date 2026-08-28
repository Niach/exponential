package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.AgentUsageWindow
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DeviceEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull

/** How loud a usage window reads (EXP-484): ≥75 warns, ≥95 is danger. */
enum class AgentUsageSeverity { Normal, Warning, Danger }

/**
 * EXP-484: how an agent's auth + usage status presents.
 *
 * Hand-mirrored on web (`lib/agent-usage.ts`), iOS
 * (`AgentUsagePresentation.swift`) and the desktop (`ui/src/usage_bar.rs`)
 * against the SAME fixture and the same test names — the strings and the
 * thresholds are the contract, so change all four or none.
 *
 * Everything here is pure and tolerant: the machine's payload is jsonb the
 * server only clamps structurally, so a malformed window is dropped rather
 * than blanking the row, and an unparseable timestamp reads NOT fresh
 * (fail closed — showing a stale percentage as live is the bad direction).
 */
object AgentUsagePresentation {

    /** Usage older than this simply isn't rendered. */
    const val FRESH_WINDOW_MS = 15 * 60_000L

    private val json = Json { ignoreUnknownKeys = true }

    // ── Parsing ──────────────────────────────────────────────────────────────

    /** One agent's `agent_usage` entry as stored/sent; null on anything unusable. */
    fun parseUsage(raw: String?): AgentUsage? =
        usageFrom(parseElement(raw))

    /** The whole `agent_usage` object → per-agent usage; null on anything unusable. */
    fun parseUsageMap(raw: String?): Map<String, AgentUsage>? {
        val root = parseElement(raw) as? JsonObject ?: return null
        return root.mapNotNull { (agent, value) ->
            usageFrom(value)?.let { agent to it }
        }.toMap()
    }

    /** The whole `agent_accounts` object → per-agent sign-in; null on anything unusable. */
    fun parseAccounts(raw: String?): Map<String, AgentAccount>? {
        val root = parseElement(raw) as? JsonObject ?: return null
        return root.mapNotNull { (agent, value) ->
            runCatching { json.decodeFromJsonElement(AgentAccount.serializer(), value) }
                .getOrNull()
                ?.let { agent to it }
        }.toMap()
    }

    private fun parseElement(raw: String?): JsonElement? =
        raw?.takeIf { it.isNotBlank() }?.let {
            runCatching { json.parseToJsonElement(it) }.getOrNull()
        }

    private fun usageFrom(element: JsonElement?): AgentUsage? {
        val obj = element as? JsonObject ?: return null
        return AgentUsage(
            fetchedAt = obj.string("fetchedAt"),
            stale = obj.bool("stale"),
            windows = (obj["windows"] as? JsonArray).orEmpty().mapNotNull(::windowFrom),
        )
    }

    // A window without a key can't be selected or remembered, so it is dropped
    // instead of failing the whole snapshot; the percent is clamped here so
    // every renderer can trust 0-100.
    private fun windowFrom(element: JsonElement): AgentUsageWindow? {
        val obj = element as? JsonObject ?: return null
        val key = obj.string("key")?.takeIf { it.isNotBlank() } ?: return null
        return AgentUsageWindow(
            key = key,
            label = obj.string("label").orEmpty(),
            percent = obj.number("percent").coerceIn(0.0, 100.0),
            resetsAt = obj.string("resetsAt"),
        )
    }

    private fun JsonObject.primitive(name: String): JsonPrimitive? = this[name] as? JsonPrimitive

    private fun JsonObject.string(name: String): String? =
        primitive(name)?.takeIf { it.isString }?.content

    private fun JsonObject.bool(name: String): Boolean =
        primitive(name)?.let { it.booleanOrNull ?: (it.content == "t") } ?: false

    private fun JsonObject.number(name: String): Double =
        primitive(name)?.let { it.doubleOrNull ?: it.content.toDoubleOrNull() } ?: 0.0

    // ── Selection + severity ─────────────────────────────────────────────────

    /**
     * The window a collapsed bar shows: the caller's remembered [preferredKey]
     * when that window is still reported, else the busiest one — which is what
     * the user actually needs to see, and what a machine with only one window
     * shows anyway. Null when there is nothing to show.
     */
    fun selectWindow(usage: AgentUsage?, preferredKey: String? = null): AgentUsageWindow? {
        val windows = usage?.windows.orEmpty()
        if (windows.isEmpty()) return null
        preferredKey?.let { key ->
            windows.firstOrNull { it.key == key }?.let { return it }
        }
        return windows.maxByOrNull { it.percent }
    }

    fun severity(percent: Double): AgentUsageSeverity = when {
        percent >= 95.0 -> AgentUsageSeverity.Danger
        percent >= 75.0 -> AgentUsageSeverity.Warning
        else -> AgentUsageSeverity.Normal
    }

    // ── Freshness + countdown ────────────────────────────────────────────────

    /**
     * Whether [fetchedAt] is recent enough to render. FAIL-CLOSED: an absent or
     * unparseable stamp is NOT fresh. A stamp in the future (the machine's
     * clock runs ahead) counts as fresh.
     */
    fun isFresh(fetchedAt: String?, nowMs: Long): Boolean {
        val at = fetchedAt?.let(WireTimestamps::parseEpochMs) ?: return false
        return nowMs - at < FRESH_WINDOW_MS
    }

    /**
     * `resets in 2h 10m` / `resets in 3d 14h` / `resets in 45m` / `resets soon`
     * (under a minute, or already past). Null when the window never resets or
     * the stamp is unreadable. A zero smaller unit is dropped, so an exact
     * boundary reads `resets in 2h`, never `resets in 2h 0m`.
     */
    fun resetCountdown(resetsAt: String?, nowMs: Long): String? {
        val at = resetsAt?.let(WireTimestamps::parseEpochMs) ?: return null
        val minutes = (at - nowMs) / 60_000L
        if (minutes < 1L) return "resets soon"
        val days = minutes / (60L * 24L)
        val hours = (minutes / 60L) % 24L
        return when {
            days > 0L -> if (hours > 0L) "resets in ${days}d ${hours}h" else "resets in ${days}d"
            hours > 0L -> {
                val rest = minutes % 60L
                if (rest > 0L) "resets in ${hours}h ${rest}m" else "resets in ${hours}h"
            }
            else -> "resets in ${minutes}m"
        }
    }

    // ── Account captions ─────────────────────────────────────────────────────

    /**
     * What one agent's sign-in reads as: `signed in as <email> · <plan>`,
     * `signed in as <email>`, the bare plan for an account with no email (pi's
     * `anthropic (oauth)`), `signed in`, `signed out`, or `unknown` when the
     * machine reported nothing for the agent.
     */
    fun accountCaption(account: AgentAccount?): String {
        if (account == null) return "unknown"
        if (!account.signedIn) return "signed out"
        val email = account.email?.takeIf { it.isNotBlank() }
        val plan = account.plan?.takeIf { it.isNotBlank() }
        return when {
            email != null && plan != null -> "signed in as $email · $plan"
            email != null -> "signed in as $email"
            plan != null -> plan
            else -> "signed in"
        }
    }

    /** The whole row: `claude · signed in as danny@yourev.at · max`. */
    fun accountRow(agent: String, account: AgentAccount?): String =
        "$agent · ${accountCaption(account)}"

    // ── Session join ─────────────────────────────────────────────────────────

    /**
     * The usage bar a LIVE session view renders: the host machine's numbers for
     * the agent that run launched with.
     *
     * Nothing renders unless every piece lines up — the run is still working
     * (`running` / `in_review`), it recorded its agent, its host row is the one
     * [resolveSessionDevice] would pick, that row reports fresh usage for the
     * agent, and there is at least one window. A missing piece is normal (old
     * rows, a machine that never reported), so this stays silent rather than
     * showing a placeholder.
     */
    fun sessionUsage(
        session: CodingSessionEntity,
        devices: List<DeviceEntity>,
        nowMs: Long,
    ): AgentUsage? {
        if (session.status != DomainContract.codingSessionStatusRunning &&
            session.status != DomainContract.codingSessionStatusInReview
        ) {
            return null
        }
        val agent = session.agent?.takeIf { it.isNotBlank() } ?: return null
        val deviceId = session.deviceId ?: return null
        val matches = devices.filter { it.deviceId == deviceId }
        val row = matches.firstOrNull { it.userId == session.userId } ?: matches.firstOrNull() ?: return null
        val usage = parseUsageMap(row.agentUsage)?.get(agent) ?: return null
        if (!isFresh(usage.fetchedAt, nowMs)) return null
        return usage.takeIf { it.windows.isNotEmpty() }
    }
}

/**
 * EXP-484: what a completed `agent_login` device command published — the
 * machine's own sign-in URL, plus the device code codex prints (claude has
 * none). The command result is the JSON
 * `{"agent":"codex","phase":"url","url":"https://…","code":"ABCD-EFGHI"}`,
 * capped at 2000 chars server-side.
 */
data class AgentLoginResult(val url: String, val code: String?)

/**
 * Parse a `done` [AgentLoginResult] out of a device command's result string.
 * Null for anything that isn't a login publication — a worktree command's
 * plain-text summary, a truncated payload, a JSON object without a URL — so
 * the caller falls through to the ordinary command caption instead of
 * rendering an empty link.
 */
fun parseAgentLoginResult(result: String?): AgentLoginResult? {
    val trimmed = result?.trim()?.takeIf { it.startsWith("{") } ?: return null
    val obj = runCatching { Json.parseToJsonElement(trimmed) }.getOrNull() as? JsonObject
        ?: return null
    fun text(name: String): String? =
        (obj[name] as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotBlank() }
    val url = text("url") ?: return null
    return AgentLoginResult(url = url, code = text("code"))
}
