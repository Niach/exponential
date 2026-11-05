package at.exponential.ui.host

import at.exponential.ui.ffi.HostZone
import at.exponential.ui.json.JsonValue

/**
 * VAPP-206 (eApps, spec `docs/spec/apps.md` §4.1): the HOST-PROVIDED binding
 * params (`@exponential_app/eui` `src/host/host-params.ts`, eui-core
 * `host::host_params`, `fixtures/host-router.json` `hostParams`). `${'$'}{today}`,
 * `${'$'}{now}` and `${'$'}{month}` in the team zone, read at each bind; the pure
 * router fills them from the `hostParams` of an `applyTemplate` or `bindDataModel`.
 */
object HostParams {
    /** The params at [nowMs] (null = the wall clock) in [zone] (null = UTC). */
    fun values(nowMs: Double?, zone: HostZone?): JsonValue {
        fun read(expr: String): String = (Expression.evaluate(expr, emptyMap(), nowMs, zone) as? Expression.Outcome.Value)?.value?.string.orEmpty()
        val today = read("today()")
        return JsonValue.Obj(mapOf("today" to JsonValue.Str(today), "now" to JsonValue.Str(read("now()")), "month" to JsonValue.Str(today.take(7))))
    }

    /** The message with `hostParams` in its `applyTemplate` or `bindDataModel` body; any other message as it is. */
    internal fun adding(json: String, nowMs: Double?, zone: HostZone?): String {
        if (!json.contains("applyTemplate") && !json.contains("bindDataModel")) return json
        val message = JsonValue.parse(json).obj ?: return json
        for (kind in listOf("applyTemplate", "bindDataModel")) {
            val body = message[kind]?.obj ?: continue
            return JsonValue.Obj(message + (kind to JsonValue.Obj(body + ("hostParams" to values(nowMs, zone))))).json
        }
        return json
    }
}
