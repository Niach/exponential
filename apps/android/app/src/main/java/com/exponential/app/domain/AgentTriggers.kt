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

    /** #EXP-1850 `kind: "record"`: an app agent's trigger on a [record]
     *  kind's [op] (create | update | delete). No editor here. */
    data class Record(val record: String, val op: String) : TriggerWhen

    /** #EXP-1850 `kind: "webhook"`: an app agent's trigger on hook [name].
     *  No editor here. */
    data class Webhook(val name: String) : TriggerWhen

    /** True for the kinds the trigger editor edits (schedule, event). */
    val editable: Boolean get() = this is Schedule || this is Event

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
        is Record -> buildJsonObject {
            put("kind", "record")
            put("record", record)
            put("op", op)
        }
        is Webhook -> buildJsonObject {
            put("kind", "webhook")
            put("name", name)
        }
    }

    /** Compact wire JSON string (kotlinx renders JsonObject compactly). */
    fun toWireJsonString(): String = toWireJson().toString()

    companion object {
        private val json = Json { ignoreUnknownKeys = true }

        /** A record trigger's ops (`agentTriggerSchema`). */
        val RECORD_OPS = setOf("create", "update", "delete")

        /** #EXP-1850: the kinds this client reads and writes: every write
         *  names them all (`agents.update` `triggerKinds`). */
        val KINDS: List<String> get() = DomainContract.agentTriggerKindValues

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
            "record" -> {
                val record = obj.stringValue("record")?.takeIf { it.isNotEmpty() }
                val op = obj.stringValue("op")?.takeIf { it in RECORD_OPS }
                if (record != null && op != null) Record(record, op) else null
            }
            "webhook" -> obj.stringValue("name")?.takeIf { it.isNotEmpty() }?.let { Webhook(it) }
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
    /** #EXP-1846 (agent-settings.json `checkIn`): the Check in preset this
     *  schedule is (an `assistant.schedules` id other than `off`); null = a
     *  listed trigger. Kept as stored on every whole-array write. */
    val preset: String? = null,
    /** #EXP-1850: every other stored key (`prompt`, `run`, …), kept as
     *  stored and written back on every whole-array write. */
    val extras: JsonObject = JsonObject(emptyMap()),
) {
    /** This trigger as an `agents.create/update({triggers})` array element. */
    fun toWireJson(): JsonObject = agentTriggerWireJson(id, enabled, whenPart, preset, extras)

    /** #EXP-1850: the app binding this trigger runs instead of a wake. */
    val run: JsonObject? get() = extras["run"] as? JsonObject
}

/** The keys a stored trigger's typed fields own (the rest are [AgentTrigger.extras]). */
private val TYPED_TRIGGER_KEYS = setOf("id", "enabled", "kind", "cron", "tz", "name", "record", "op", "preset")

/**
 * One element of the `triggers` array (`agentTriggerSchema`, strict): `id`
 * (absent = new), `enabled`, then the when-part's keys
 * ([TriggerWhen.toWireJson]), then a schedule's `preset` (#EXP-1846), then
 * the stored [extras] (#EXP-1850: `prompt`, `run`) unchanged.
 */
fun agentTriggerWireJson(
    id: String?,
    enabled: Boolean,
    whenPart: TriggerWhen,
    preset: String? = null,
    extras: JsonObject = JsonObject(emptyMap()),
): JsonObject =
    buildJsonObject {
        id?.let { put("id", it) }
        put("enabled", enabled)
        whenPart.toWireJson().forEach { (key, value) -> put(key, value) }
        // #EXP-1846: only a schedule carries the Check in tag.
        if (whenPart is TriggerWhen.Schedule) preset?.takeIf { it.isNotEmpty() }?.let { put("preset", it) }
        extras.forEach { (key, value) -> if (key !in TYPED_TRIGGER_KEYS) put(key, value) }
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
    val preset = (obj["preset"] as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotEmpty() }
        ?.takeIf { whenPart is TriggerWhen.Schedule }
    // #EXP-1850: prompt, run and any key this build has no field for ride along.
    val extras = JsonObject(obj.filterKeys { it !in TYPED_TRIGGER_KEYS })
    return AgentTrigger(id = id, enabled = enabled, whenPart = whenPart, preset = preset, extras = extras)
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
 * these back unchanged, appended: an edit here never erases them. A
 * readable trigger keeps its other keys itself ([AgentTrigger.extras]).
 */
fun unreadableAgentTriggers(raw: String?): List<JsonObject> {
    if (raw.isNullOrBlank()) return emptyList()
    val array = runCatching { triggersJson.parseToJsonElement(raw) }.getOrNull() as? JsonArray
        ?: return emptyList()
    return array.filterIsInstance<JsonObject>().filter { parseAgentTrigger(it) == null }
}
