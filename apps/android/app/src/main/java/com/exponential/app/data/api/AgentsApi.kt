package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// NEW-2: team AGENTS = definition rows (apps/web/src/lib/trpc/agents.ts).
// Owner writes, members read the synced `agents` shape; the server-only
// `instructions` come through `agents.get`. An agent runs as conversation
// sessions (one per member): message one with `sessions.send({agentId})`.
// `triggers` = `agentTriggerWireJson` elements (`{id?, enabled, kind:
// schedule, cron, tz?}` | `{id?, enabled, kind: event, name}`); an element
// without `id` is new (the server mints it), the WHOLE array is replaced.

/** `agents.create`'s input (`agentFacetsSchema.extend({teamId})`). */
@Serializable
data class AgentCreateInput(
    val teamId: String,
    val name: String,
    val icon: String? = null,
    val color: String? = null,
    val instructions: String = "",
    val triggers: JsonArray = JsonArray(emptyList()),
    val deviceId: String,
    val cli: String? = null,
    val account: String? = null,
    val model: String? = null,
)

/** One agent row as `agents.get` / the write echoes answer it. */
@Serializable
data class AgentDto(
    val id: String,
    val teamId: String = "",
    val name: String = "",
    val icon: String? = null,
    val color: String? = null,
    /** Server-only (`agents.get`). */
    val instructions: String = "",
    val triggers: JsonArray = JsonArray(emptyList()),
    val deviceId: String = "",
    val cli: String? = null,
    val account: String? = null,
    val model: String? = null,
    val ownerId: String = "",
    val sortOrder: Double = 0.0,
)

@Serializable
data class AgentResult(val agent: AgentDto, val txid: Long? = null)

@Serializable
private data class AgentIdInput(val id: String)

/**
 * `agents.update` patch. An absent field keeps its value; [account] and
 * [model] are NULLABLE server-side: [clearAccount] / [clearModel] send an
 * explicit JSON null (the shared Json would otherwise drop it).
 */
data class UpdateAgentPatch(
    val name: String? = null,
    val icon: String? = null,
    val color: String? = null,
    val instructions: String? = null,
    /** `agentTriggerWireJson` elements; the WHOLE array is replaced. */
    val triggers: List<JsonObject>? = null,
    val deviceId: String? = null,
    val cli: String? = null,
    val account: String? = null,
    val clearAccount: Boolean = false,
    val model: String? = null,
    val clearModel: Boolean = false,
    val sortOrder: Double? = null,
)

internal fun updateAgentInput(agentId: String, patch: UpdateAgentPatch): JsonObject =
    buildJsonObject {
        put("id", agentId)
        patch.name?.let { put("name", it) }
        patch.icon?.let { put("icon", it) }
        patch.color?.let { put("color", it) }
        patch.instructions?.let { put("instructions", it) }
        patch.triggers?.let { put("triggers", JsonArray(it)) }
        patch.deviceId?.let { put("deviceId", it) }
        patch.cli?.let { put("cli", it) }
        when {
            patch.account != null -> put("account", patch.account)
            patch.clearAccount -> put("account", JsonNull)
        }
        when {
            patch.model != null -> put("model", patch.model)
            patch.clearModel -> put("model", JsonNull)
        }
        patch.sortOrder?.let { put("sortOrder", it) }
    }

@Singleton
class AgentsApi @Inject constructor(private val trpc: TrpcClient) {

    /** Member read: the row with its server-only instructions. */
    suspend fun get(accountId: String, agentId: String): AgentDto =
        trpc.query(
            accountId,
            path = "agents.get",
            input = AgentIdInput(agentId),
            inputSerializer = AgentIdInput.serializer(),
            outputSerializer = AgentResult.serializer(),
        ).agent

    /** Owner-only: a new agent definition (Settings → Agents → New agent). */
    suspend fun create(accountId: String, input: AgentCreateInput): AgentDto =
        trpc.mutation(
            accountId,
            path = "agents.create",
            input = input,
            inputSerializer = AgentCreateInput.serializer(),
            outputSerializer = AgentResult.serializer(),
        ).agent

    /** Owner-only: patch an agent's facets / runner / triggers. */
    suspend fun update(accountId: String, agentId: String, patch: UpdateAgentPatch) {
        trpc.mutationUnit(
            accountId,
            path = "agents.update",
            input = updateAgentInput(agentId, patch),
            inputSerializer = JsonObject.serializer(),
        )
    }

    /** Owner-only: delete an agent (its conversations go with it). */
    suspend fun delete(accountId: String, agentId: String) {
        trpc.mutationUnit(
            accountId,
            path = "agents.delete",
            input = AgentIdInput(agentId),
            inputSerializer = AgentIdInput.serializer(),
        )
    }

    /**
     * S-3: the caller's Assistant in [teamId], created on first use on their
     * newest own desktop (null = no device yet). The composer home, the
     * chats list and the settings Agents group call it on mount; the row
     * arrives through the agents shape.
     */
    suspend fun ensureAssistant(accountId: String, teamId: String) {
        trpc.mutationUnit(
            accountId,
            path = "agents.ensureAssistant",
            input = kotlinx.serialization.json.buildJsonObject { put("teamId", kotlinx.serialization.json.JsonPrimitive(teamId)) },
            inputSerializer = JsonObject.serializer(),
        )
    }
}

/**
 * S-3: `agents.ensureAssistant` once per (account, team) per app run — the
 * composer home, the chats list and the settings Agents group all ask; a
 * failure (no device yet, offline) is retried on the next mount.
 */
object AssistantEnsurer {
    private val done: MutableSet<String> = java.util.concurrent.ConcurrentHashMap.newKeySet()

    suspend fun ensure(api: AgentsApi, accountId: String?, teamId: String?) {
        if (accountId.isNullOrEmpty() || teamId.isNullOrEmpty()) return
        val key = "$accountId:$teamId"
        if (!done.add(key)) return
        try {
            api.ensureAssistant(accountId, teamId)
        } catch (t: Throwable) {
            done.remove(key)
            if (t is kotlinx.coroutines.CancellationException) throw t
        }
    }
}
