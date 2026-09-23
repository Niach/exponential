package com.exponential.app.data.api

import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.domain.AutomationTrigger
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// Mirrors apps/web/src/lib/trpc/automations.ts (EXP-583). The rows themselves
// arrive over the `automations` shape (AutomationEntity) — this API carries
// only the mutations: create, the enabled toggle / edit, and delete. All three
// are owner-gated server-side, and Electric echoes the written row back into
// the shape, so a success needs no local write. `list` exists server-side but
// mobile never calls it; sync is the read path.

/**
 * One automations row as the tRPC mutations answer it. Kept separate from the
 * synced [AutomationEntity] because the wire shape is camelCase and the
 * trigger arrives as a JSON object, not the raw text Room stores.
 */
@Serializable
data class AutomationDto(
    @SerialName("id") val id: String,
    @SerialName("teamId") val teamId: String = "",
    @SerialName("actionId") val actionId: String = "",
    @SerialName("deviceId") val deviceId: String = "",
    @SerialName("enabled") val enabled: Boolean = true,
    @SerialName("trigger") val trigger: JsonObject? = null,
    @SerialName("agent") val agent: String? = null,
    /** EXP-995: the agent profile id on the bound device (belongs to `agent`). */
    @SerialName("account") val account: String? = null,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("sortOrder") val sortOrder: Double = 0.0,
) {
    /** The when-part, tolerantly parsed (unknown kinds read as null). */
    val parsedTrigger: AutomationTrigger?
        get() = AutomationTrigger.parse(trigger?.toString())
}

/** `automations.create`'s / `.update`'s answer (the txId is unused here). */
@Serializable
data class AutomationMutationResult(
    @SerialName("automation") val automation: AutomationDto,
)

@Serializable
private data class CreateAutomationInput(
    @SerialName("teamId") val teamId: String,
    @SerialName("actionId") val actionId: String,
    @SerialName("deviceId") val deviceId: String,
    @SerialName("trigger") val trigger: JsonObject,
    @SerialName("agent") val agent: String? = null,
    @SerialName("account") val account: String? = null,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
)

/**
 * EXP-995: the four launch pins the edit form writes as ONE unit (iOS
 * `AutomationLaunchPatch` parity). Each null travels as an EXPLICIT JSON null
 * — "back to the device's launch defaults" — which the server reads as a
 * clear, where an ABSENT key would mean "keep the stored value".
 */
data class AutomationLaunchPatch(
    val agent: String? = null,
    /** The agent profile id on the bound device (belongs to [agent]). */
    val account: String? = null,
    val model: String? = null,
    val effort: String? = null,
)

/**
 * `automations.update`'s TRI-STATE input (iOS `UpdateInput` parity): `id`
 * always; every other key OMITTED when the caller did not touch it — the
 * server reads an absent key as "keep", so the enable toggle really sends
 * only `{id, enabled}` (a wider class would send `"trigger": null`, which the
 * server's optional-not-nullable schema refuses outright); and the launch
 * pins as literal nulls when [launch] clears them. A [JsonObject] rather
 * than a `@Serializable` class on purpose: the shared Json drops null
 * properties (`explicitNulls = false`), which collapsed "clear the account
 * pin" into "keep the old profile" (the `devices.setIcon` reset story).
 */
internal fun updateAutomationInput(
    id: String,
    actionId: String? = null,
    deviceId: String? = null,
    trigger: JsonObject? = null,
    enabled: Boolean? = null,
    launch: AutomationLaunchPatch? = null,
): JsonObject = buildJsonObject {
    put("id", id)
    actionId?.let { put("actionId", it) }
    deviceId?.let { put("deviceId", it) }
    trigger?.let { put("trigger", it) }
    enabled?.let { put("enabled", it) }
    if (launch != null) {
        put("agent", nullableString(launch.agent))
        put("account", nullableString(launch.account))
        put("model", nullableString(launch.model))
        put("effort", nullableString(launch.effort))
    }
}

/** A pin value, or the literal `null` that clears it (blank clears too). */
private fun nullableString(value: String?): JsonElement =
    value?.takeIf { it.isNotEmpty() }?.let(::JsonPrimitive) ?: JsonNull

@Serializable
private data class AutomationIdInput(@SerialName("id") val id: String)

@Singleton
class AutomationsApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * `automations.create` — bind [actionId] to [deviceId] with [trigger].
     * Null [agent]/[account]/[model]/[effort] mean "the device's own launch
     * defaults"; the server validates the pins against what that machine
     * advertises (EXP-995: an [account] is a profile of [agent], so it needs
     * the agent pinned beside it).
     */
    suspend fun create(
        accountId: String,
        teamId: String,
        actionId: String,
        deviceId: String,
        trigger: AutomationTrigger,
        agent: String? = null,
        account: String? = null,
        model: String? = null,
        effort: String? = null,
    ): AutomationDto = trpc.mutation(
        accountId,
        path = "automations.create",
        input = CreateAutomationInput(
            teamId = teamId,
            actionId = actionId,
            deviceId = deviceId,
            trigger = trigger.toWireJson(),
            agent = agent?.takeIf { it.isNotEmpty() },
            account = account?.takeIf { it.isNotEmpty() },
            model = model?.takeIf { it.isNotEmpty() },
            effort = effort?.takeIf { it.isNotEmpty() },
        ),
        inputSerializer = CreateAutomationInput.serializer(),
        outputSerializer = AutomationMutationResult.serializer(),
    ).automation

    /**
     * `automations.update` with just the paused flag — the Automations tab's
     * enabled Switch. Everything else keeps its stored value.
     */
    suspend fun setEnabled(
        accountId: String,
        id: String,
        enabled: Boolean,
    ): AutomationDto = trpc.mutation(
        accountId,
        path = "automations.update",
        input = updateAutomationInput(id = id, enabled = enabled),
        inputSerializer = JsonObject.serializer(),
        outputSerializer = AutomationMutationResult.serializer(),
    ).automation

    /**
     * `automations.update` from the edit form (EXP-615): the target action,
     * the bound machine, the when-part and the launch pins. Every null (or
     * blank) in [launch] travels as an explicit null — "back to the device's
     * own launch defaults" — never as an omitted key, which would keep the
     * stored value (EXP-995: the account pin could not be cleared).
     */
    suspend fun update(
        accountId: String,
        id: String,
        actionId: String,
        deviceId: String,
        trigger: AutomationTrigger,
        launch: AutomationLaunchPatch,
    ): AutomationDto = trpc.mutation(
        accountId,
        path = "automations.update",
        input = updateAutomationInput(
            id = id,
            actionId = actionId,
            deviceId = deviceId,
            trigger = trigger.toWireJson(),
            launch = launch,
        ),
        inputSerializer = JsonObject.serializer(),
        outputSerializer = AutomationMutationResult.serializer(),
    ).automation

    /** `automations.delete` — owner-only, permanent. */
    suspend fun delete(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "automations.delete",
            input = AutomationIdInput(id = id),
            inputSerializer = AutomationIdInput.serializer(),
        )
    }
}
