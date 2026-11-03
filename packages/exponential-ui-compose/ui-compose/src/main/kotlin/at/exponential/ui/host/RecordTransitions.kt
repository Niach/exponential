package at.exponential.ui.host

import at.exponential.ui.ffi.transitionRefusalJson
import at.exponential.ui.ffi.transitionsFromJson
import at.exponential.ui.ffi.validateRecordTransitionsJson
import at.exponential.ui.json.JsonValue

/**
 * eApps R6 (VAPP-222): a kind's state machine on one enum field, the core's
 * one rule (`@exponential_app/eui/host` record-transitions, eui-core
 * `host::record_transitions`, `fixtures/record-transitions.json`). [kind] =
 * the kind's definition (`records.<kind>`, `transitions` inside).
 */
object RecordTransitions {
    /** Why a write may not move the state (`{code, message}`), null = it may. [before] null = a create. */
    fun refusal(kind: JsonValue, before: JsonValue?, after: JsonValue, viewer: JsonValue, row: JsonValue? = null): JsonValue? {
        val out = runCatching { transitionRefusalJson(kind.json, before?.json, after.json, viewer.json, row?.json) }.getOrNull() ?: return null
        return JsonValue.parse(out)
    }

    /** The next states a record may move to (null record = the initial states), only those [viewer] (`{id, role}`) may take when given. */
    fun from(kind: JsonValue, record: JsonValue?, viewer: JsonValue? = null, row: JsonValue? = null): List<String> {
        val out = runCatching { transitionsFromJson(kind.json, record?.json, viewer?.json, row?.json) }.getOrNull() ?: return emptyList()
        return JsonValue.parse(out).array.orEmpty().mapNotNull { it.string }
    }

    /** The issues of a kind's `transitions` (`[{path, message}]` under `<at>/transitions`). */
    fun validate(kind: JsonValue, at: String, kindName: String): List<JsonValue> {
        val out = runCatching { validateRecordTransitionsJson(kind.json, at, kindName) }.getOrNull() ?: return emptyList()
        return JsonValue.parse(out).array.orEmpty()
    }
}
