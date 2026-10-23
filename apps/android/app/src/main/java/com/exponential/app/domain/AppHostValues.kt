package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.ui.components.deviceIconName
import java.text.Collator

// VAPP-110: the app host's `exp:` binding values, derived exactly as the web
// host derives them (apps/web/src/lib/exponential-ui-host.tsx `devicesValue`
// / `issuesSource`) and the desktop host's twin: `exp:devices` →
// `{rows, count, online}`, `exp:issues?board=&limit=` → `{rows, count}`.
object AppHostValues {
    /** Online-ness ages out on a clock: `exp:devices` re-derives on this beat. */
    const val LIVENESS_TICK_MS = 30_000L

    /** The `exp:issues` default (and fallback) `limit`. */
    const val ISSUES_LIMIT = 50

    private val collator: Collator by lazy { Collator.getInstance() }

    /** `exp:devices` at [nowMs]: `Online` within the contract's window, else `Last seen <ago>`. */
    fun devices(devices: List<DeviceEntity>, nowMs: Long): JsonValue {
        val rows = devices.sortedWith { a, b -> collator.compare(a.label, b.label) }.map { d ->
            val seen = d.lastSeenAt?.let(WireTimestamps::parseEpochMs)
            val online = seen != null && DeviceLiveness.isOnline(d.lastSeenAt, nowMs)
            val detail = listOfNotNull(
                if (d.kind == "server") "Server" else "Desktop",
                d.platform?.takeIf { it.isNotEmpty() },
                d.version?.takeIf { it.isNotEmpty() }?.let { "v$it" },
            ).joinToString(" · ")
            val status = when {
                online -> "Online"
                seen != null -> "Last seen ${CodingReadiness.ago(nowMs, seen)}"
                else -> "Offline"
            }
            obj(
                "id" to JsonValue.Str(d.id),
                "label" to JsonValue.Str(d.label),
                "detail" to JsonValue.Str(detail),
                "icon" to JsonValue.Str(deviceIconName(d.icon, d.kind == "server")),
                "discTone" to JsonValue.Str(if (online) "success" else "muted"),
                "tone" to JsonValue.Str(if (online) "live" else "idle"),
                "status" to JsonValue.Str(status),
            ) to online
        }
        return obj(
            "rows" to JsonValue.Arr(rows.map { it.first }),
            "count" to JsonValue.Num(rows.size.toDouble()),
            "online" to JsonValue.Num(rows.count { it.second }.toDouble()),
        )
    }

    /** `exp:issues?board=<id>&limit=<n>`: the board's issues in board order, capped. */
    fun issues(issues: List<IssueEntity>, params: Map<String, String>): JsonValue {
        val board = params["board"]?.takeIf { it.isNotEmpty() }
        val limit = params["limit"]?.toDoubleOrNull()?.takeIf { it.isFinite() }?.toInt() ?: ISSUES_LIMIT
        val rows = issues
            .filter { board == null || it.boardId == board }
            .sortedBy { it.sortOrder }
            .take(limit.coerceAtLeast(0))
            .map {
                obj(
                    "id" to JsonValue.Str(it.id),
                    "identifier" to JsonValue.Str(it.identifier),
                    "title" to JsonValue.Str(it.title),
                    "status" to JsonValue.Str(it.status),
                )
            }
        return obj("rows" to JsonValue.Arr(rows), "count" to JsonValue.Num(rows.size.toDouble()))
    }

    private fun obj(vararg entries: Pair<String, JsonValue>) = JsonValue.Obj(linkedMapOf(*entries))
}
