package at.exponential.ui.host

import at.exponential.ui.ffi.HostZone
import at.exponential.ui.json.JsonValue
import java.util.TimeZone

/**
 * VAPP-194 (eApps E1, spec `docs/spec/apps.md` §4.5): typed effects, the
 * core's one action runner (`@exponential_app/eui` `src/host/actions.ts`,
 * eui-core `host::actions`, `fixtures/actions.json`). Expressions are PURE;
 * a package's `actions: {<name>: {params?, steps}}` are the only place it
 * writes. A Button's `on.press` names one as the function call
 * `actions.<name>`; [ExponentialHost.callFunction] runs it: the PLAN ([run],
 * one snapshot), every host call's gate, the record writes as ONE
 * `records.batch`, the data sets, then the calls. This is the direct entry
 * for a host (and the tests).
 */
object Actions {
    /** What calling a function does ([effect]). */
    enum class Effect(val wire: String) {
        /** Every expression and bind-time function. */
        Pure("pure"),

        /** `set`, `records.upsert` / `.delete` / `.batch`. */
        Write("write"),

        /** `openUrl`, `bind`, every `actions.` `harness.` `exponential.` `tools.` `upstream.` name and every Engine capability's (`photos.`, `files.`, `print.`, `share.`, …; VAPP-271). */
        Host("host"),
        ;

        companion object {
            fun of(wire: String): Effect = entries.firstOrNull { it.wire == wire } ?: Pure
        }
    }

    /** The function-call prefix that runs a package action. */
    const val CALL_PREFIX = "actions."

    /** The host function a host commits an action's record writes through (args `{writes}`; answers `{ids}`). */
    const val BATCH_FUNCTION = "records.batch"

    /** The action a call name runs (`actions.log` → `log`), else null. */
    fun actionName(call: String): String? = if (call.startsWith(CALL_PREFIX)) call.removePrefix(CALL_PREFIX) else null

    /** What one run plans, as the core spells it: `{writes, sets, calls}` ([json]). */
    data class Plan(val json: JsonValue) {
        /** One data-model write (a JSON pointer, the value). */
        data class Assignment(val path: String, val value: JsonValue)

        /** One host function call (after the commit). */
        data class Call(val name: String, val args: Map<String, JsonValue>)

        /** The record writes (`{op: "upsert", kind, data, id?}` | `{op: "delete", id}`), passed to `records.batch` unchanged. */
        val writes: List<JsonValue> get() = json["writes"]?.array.orEmpty()

        val sets: List<Assignment> get() = json["sets"]?.array.orEmpty().map { Assignment(it["path"]?.string.orEmpty(), it["value"] ?: JsonValue.Null) }

        val calls: List<Call> get() = json["calls"]?.array.orEmpty().map { Call(it["name"]?.string.orEmpty(), it["args"]?.obj.orEmpty()) }
    }

    /**
     * VAPP-271: the first `call` step with a `result` the run reached
     * unanswered: the host gates and runs it (before any commit), then runs
     * the action again over the SAME snapshot with its answer under
     * `results[result]`.
     */
    data class Pending(
        /** The step's index in the action. */
        val step: Int,
        /** The host function to call. */
        val name: String,
        val args: Map<String, JsonValue>,
        /** The result name later steps read it as (`results.<result>`). */
        val result: String,
    )

    /** The plan, a result call still to answer (VAPP-271), or the one error that discards the whole action. */
    sealed interface Outcome {
        data class Planned(val plan: Plan) : Outcome

        data class Waiting(val pending: Pending) : Outcome

        data class Error(val message: String) : Outcome
    }

    /** The batch applied (every row after it, each write's id), or the first refusal (`write <i>: …`, nothing applied). */
    sealed interface BatchOutcome {
        data class Applied(val rows: List<JsonValue>, val ids: List<String>) : BatchOutcome

        data class Error(val message: String) : BatchOutcome
    }

    /**
     * Runs action [name] of [pkg] with [params] against one snapshot: [data]
     * (the surface data model), [me] (the member; null = unknown), [nowMs]
     * (epoch ms; null = the wall clock), [zone] (the team zone; null = UTC),
     * [maxSteps] (the run's budget; null = the contract's 100 000),
     * [results] (VAPP-271: the answers of the result calls so far, by
     * result name). Pure: nothing is written.
     */
    fun run(
        pkg: JsonValue,
        name: String,
        params: JsonValue? = null,
        data: JsonValue? = null,
        me: String? = null,
        nowMs: Double? = null,
        zone: HostZone? = null,
        maxSteps: UInt? = null,
        results: Map<String, JsonValue> = emptyMap(),
    ): Outcome {
        val out = runCatching {
            at.exponential.ui.ffi.runPackageActionJson(
                pkg.json,
                name,
                (params ?: JsonValue.Null).json,
                (data ?: JsonValue.Null).json,
                me,
                nowMs,
                zone,
                maxSteps,
                if (results.isEmpty()) null else JsonValue.Obj(results).json,
            )
        }.getOrNull() ?: return Outcome.Error("the package is JSON")
        val v = JsonValue.parse(out)
        v["error"]?.string?.let { return Outcome.Error(it) }
        v["pending"]?.let { p ->
            return Outcome.Waiting(Pending(p["step"]?.number?.toInt() ?: 0, p["name"]?.string.orEmpty(), p["args"]?.obj.orEmpty(), p["result"]?.string.orEmpty()))
        }
        return Outcome.Planned(Plan(v["plan"] ?: JsonValue.Null))
    }

    /** [run] in a `java.util.TimeZone`. */
    fun run(pkg: JsonValue, name: String, params: JsonValue?, data: JsonValue?, me: String?, nowMs: Double?, timeZone: TimeZone, results: Map<String, JsonValue> = emptyMap()): Outcome =
        run(pkg, name, params, data, me, nowMs, TimeZoneOffset(timeZone), results = results)

    /**
     * Applies [writes] to [rows] (record rows as JSON: `{id, kind, data,
     * createdBy, createdAt, updatedAt}`) all or nothing, checked against
     * [records] (the package's kinds): a create is stamped [me] / [now] and
     * takes the next of [newIds]. What a host's in-memory store does for
     * `records.batch`.
     */
    fun applyBatch(rows: List<JsonValue>, writes: List<JsonValue>, records: JsonValue, me: String?, now: String, newIds: List<String>): BatchOutcome {
        val out = runCatching {
            at.exponential.ui.ffi.applyRecordBatchJson(
                JsonValue.Arr(rows).json,
                JsonValue.Arr(writes).json,
                records.json,
                me,
                now,
                JsonValue.Arr(newIds.map { JsonValue.Str(it) }).json,
            )
        }.getOrNull() ?: return BatchOutcome.Error("the batch is JSON")
        val v = JsonValue.parse(out)
        v["error"]?.string?.let { return BatchOutcome.Error(it) }
        return BatchOutcome.Applied(v["rows"]?.array.orEmpty(), v["ids"]?.array.orEmpty().mapNotNull { it.string })
    }

    /** What calling [name] does. */
    fun effect(name: String): Effect = Effect.of(at.exponential.ui.ffi.functionEffect(name))
}

/** A package action that landed nothing (a false `require`'s message, a refused write or host call): the host's failed-action toast. */
data class ActionFailure(
    val surfaceId: String,
    /** The action's name (`log` for `actions.log`). */
    val action: String,
    val message: String,
)
