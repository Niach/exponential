package com.exponential.app.data.api

import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.domain.AutomationTrigger
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

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
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
    @SerialName("sortOrder") val sortOrder: Double = 0.0,
) {
    /** The when-part, tolerantly parsed (unknown kinds read as null). */
    val parsedTrigger: AutomationTrigger?
        get() = AutomationTrigger.parse(trigger?.toString())
}

/** `automations.create`'s / `.update`'s answer (the txid is unused here). */
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
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
)

@Serializable
private data class UpdateAutomationInput(
    @SerialName("id") val id: String,
    @SerialName("actionId") val actionId: String? = null,
    @SerialName("deviceId") val deviceId: String? = null,
    @SerialName("trigger") val trigger: JsonObject? = null,
    @SerialName("enabled") val enabled: Boolean? = null,
    @SerialName("agent") val agent: String? = null,
    @SerialName("model") val model: String? = null,
    @SerialName("effort") val effort: String? = null,
)

@Serializable
private data class AutomationIdInput(@SerialName("id") val id: String)

@Singleton
class AutomationsApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * `automations.create` — bind [actionId] to [deviceId] with [trigger].
     * Null [agent]/[model]/[effort] mean "the device's own launch defaults";
     * the server validates the pins against what that machine advertises.
     */
    suspend fun create(
        accountId: String,
        teamId: String,
        actionId: String,
        deviceId: String,
        trigger: AutomationTrigger,
        agent: String? = null,
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
            model = model?.takeIf { it.isNotEmpty() },
            effort = effort?.takeIf { it.isNotEmpty() },
        ),
        inputSerializer = CreateAutomationInput.serializer(),
        outputSerializer = AutomationMutationResult.serializer(),
    ).automation

    /**
     * `automations.update` — a partial patch; every omitted field keeps its
     * stored value. The Automations tab's enabled Switch sends just
     * `{id, enabled}`.
     */
    suspend fun update(
        accountId: String,
        id: String,
        actionId: String? = null,
        deviceId: String? = null,
        trigger: AutomationTrigger? = null,
        enabled: Boolean? = null,
        agent: String? = null,
        model: String? = null,
        effort: String? = null,
    ): AutomationDto = trpc.mutation(
        accountId,
        path = "automations.update",
        input = UpdateAutomationInput(
            id = id,
            actionId = actionId,
            deviceId = deviceId,
            trigger = trigger?.toWireJson(),
            enabled = enabled,
            agent = agent,
            model = model,
            effort = effort,
        ),
        inputSerializer = UpdateAutomationInput.serializer(),
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
