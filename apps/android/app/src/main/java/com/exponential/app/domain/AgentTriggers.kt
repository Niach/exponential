package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// NEW-2 agent triggers. One stored trigger ([AgentTrigger]) = its id +
// enabled flag + the WHEN-part ([TriggerWhen]: a schedule = a 5-field cron
// (`m h dom mon dow`) in an optional IANA zone, or an event = the NAME of an
// `events` row, a builtin (contract `eventBuiltinNames`) or any custom name).
// They ride the synced `agents.triggers` jsonb array; the SERVER fires them
// (`lib/agents/triggers.ts`) into the agent owner's conversation session, the
// device arms nothing and a phone never runs one.
// Parsing is deliberately TOLERANT: an unknown kind or malformed JSON reads as
// null ("no trigger"), never a crash, so a future trigger shape can't brick
// this client.

sealed interface TriggerWhen {

    /** `kind: "schedule"`: a 5-field cron in [tz] (null = UTC). */
    data class Schedule(val cron: String, val tz: String? = null) : TriggerWhen

    /** `kind: "event"`: fires when an `events` row named [name] lands. */
    data class Event(val name: String) : TriggerWhen

    /** The wire JSON object with the server's field names, keys in the
     *  canonical order (kind, cron, tz | kind, name); an empty tz is omitted. */
    fun toWireJson(): JsonObject = when (this) {
        is Schedule -> buildJsonObject {
            put("kind", "schedule")
            put("cron", cron)
            tz?.takeIf { it.isNotBlank() }?.let { put("tz", it) }
        }
        is Event -> buildJsonObject {
            put("kind", "event")
            put("name", name)
        }
    }

    /** Compact wire JSON string (kotlinx renders JsonObject compactly). */
    fun toWireJsonString(): String = toWireJson().toString()

    companion object {
        private val json = Json { ignoreUnknownKeys = true }

        /** Tolerant parse of a when-part JSON string; any malformation = null. */
        fun parse(raw: String?): TriggerWhen? {
            if (raw.isNullOrBlank()) return null
            val obj = runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonObject
                ?: return null
            return parse(obj)
        }

        /** The same tolerant read off an already-decoded object (a stored
         *  trigger's id / enabled beside it are ignored). */
        fun parse(obj: JsonObject): TriggerWhen? = when (obj.stringValue("kind")) {
            "schedule" -> obj.stringValue("cron")?.trim()?.takeIf { it.isNotEmpty() }?.let { cron ->
                Schedule(cron = cron, tz = obj.stringValue("tz")?.trim()?.takeIf { it.isNotEmpty() })
            }
            "event" -> obj.stringValue("name")?.trim()?.takeIf { it.isNotEmpty() }?.let { Event(it) }
            else -> null
        }

        private fun JsonObject.stringValue(key: String): String? =
            (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
    }
}

/**
 * ONE stored trigger of an agent: its [id] (null = new, the server mints it),
 * the [enabled] flag and the [whenPart]. Its row text is
 * [AgentSettings.triggerSummary].
 */
data class AgentTrigger(
    val id: String?,
    val enabled: Boolean = true,
    val whenPart: TriggerWhen,
) {
    /** This trigger as an `agents.create/update({triggers})` array element. */
    fun toWireJson(): JsonObject = agentTriggerWireJson(id, enabled, whenPart)
}

/**
 * One element of the `triggers` array (`agentTriggerSchema`, strict): `id`
 * (absent = new), `enabled`, then the when-part's keys
 * ([TriggerWhen.toWireJson]). No other key travels.
 */
fun agentTriggerWireJson(id: String?, enabled: Boolean, whenPart: TriggerWhen): JsonObject =
    buildJsonObject {
        id?.let { put("id", it) }
        put("enabled", enabled)
        whenPart.toWireJson().forEach { (key, value) -> put(key, value) }
    }

private val triggersJson = Json { ignoreUnknownKeys = true }

/**
 * Tolerant read of ONE stored agent trigger: an unreadable when-part is null.
 * Only an explicit `false` pauses: a missing `enabled` is an enabled trigger.
 */
fun parseAgentTrigger(value: JsonElement?): AgentTrigger? {
    val obj = value as? JsonObject ?: return null
    val whenPart = TriggerWhen.parse(obj) ?: return null
    val id = (obj["id"] as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotEmpty() }
    val enabled = (obj["enabled"] as? JsonPrimitive)
        ?.takeIf { !it.isString }?.content != "false"
    return AgentTrigger(id = id, enabled = enabled, whenPart = whenPart)
}

/** Tolerant read of an agent's `triggers` (the synced column's raw JSON
 *  text): the readable ones, in array order; anything else = none. */
fun parseAgentTriggers(raw: String?): List<AgentTrigger> {
    if (raw.isNullOrBlank()) return emptyList()
    val array = runCatching { triggersJson.parseToJsonElement(raw) }.getOrNull() as? JsonArray
        ?: return emptyList()
    return array.mapNotNull(::parseAgentTrigger)
}

/**
 * The stored trigger objects this build CANNOT read (a kind from a newer
 * client or MCP). `triggers` is a whole-array replace, so every write sends
 * these back unchanged, appended: an edit here never erases them.
 */
fun unreadableAgentTriggers(raw: String?): List<JsonObject> {
    if (raw.isNullOrBlank()) return emptyList()
    val array = runCatching { triggersJson.parseToJsonElement(raw) }.getOrNull() as? JsonArray
        ?: return emptyList()
    return array.filterIsInstance<JsonObject>().filter { parseAgentTrigger(it) == null }
}
