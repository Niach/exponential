package com.exponential.app.data.api

import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.domain.WorkflowLaunch
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject

// Mirrors apps/web/src/lib/trpc/workflows.ts (EXP-981). The rows themselves
// arrive over the `workflows` + `workflow_nodes` shapes (WorkflowEntity /
// WorkflowNodeEntity) — this API carries only the mutations: create the draft,
// rename it or change how it runs, add/drop issues, set a node's plan, replan,
// delete. Any team MEMBER may call them (a workflow is work, not a team
// setting), and every refusal is a human sentence the caller shows verbatim.
// `list`/`get` exist server-side for MCP; sync is the read path here.

/** `workflows.create`'s / `.update`'s answer (the txId is unused here). */
@Serializable
data class WorkflowMutationResult(
    @SerialName("workflow") val workflow: WorkflowDto,
)

/**
 * One workflow row as the tRPC mutations answer it. Kept separate from the
 * synced [WorkflowEntity] because the wire shape is camelCase and the jsonb
 * columns arrive as objects, not the raw text Room stores. Only [id] is read
 * by the callers (they navigate; Electric delivers the row itself).
 */
@Serializable
data class WorkflowDto(
    @SerialName("id") val id: String,
    @SerialName("teamId") val teamId: String = "",
    @SerialName("name") val name: String = "",
    @SerialName("status") val status: String = "",
    @SerialName("repositoryId") val repositoryId: String? = null,
)

@Serializable
private data class CreateWorkflowInput(
    @SerialName("teamId") val teamId: String,
    @SerialName("issueIds") val issueIds: List<String>,
    @SerialName("name") val name: String? = null,
)

@Serializable
private data class SetIssuesInput(
    @SerialName("id") val id: String,
    @SerialName("addIssueIds") val addIssueIds: List<String>,
    @SerialName("removeIssueIds") val removeIssueIds: List<String>,
)

@Serializable
private data class WorkflowIdInput(@SerialName("id") val id: String)

/**
 * `workflows.update`'s patch, hand-built: the router applies a key only when
 * it is `!== undefined`, so an OMITTED key means "keep" — and the shared Json
 * (`explicitNulls = false`) would drop a null property from a `@Serializable`
 * class outright, which is exactly how [deviceId] clears the runner ([JsonNull]
 * rides as the explicit "unbind"). The same rule the action editor's patch
 * follows.
 */
internal fun updateWorkflowInput(
    id: String,
    name: String?,
    deviceId: String?,
    clearDevice: Boolean,
    launch: WorkflowLaunch?,
    gate: String?,
    startOn: String?,
): JsonObject = buildJsonObject {
    put("id", id)
    name?.let { put("name", it) }
    when {
        clearDevice -> put("deviceId", JsonNull)
        deviceId != null -> put("deviceId", JsonPrimitive(deviceId))
    }
    launch?.let { options ->
        putJsonObject("launch") {
            options.agent.takeIf { it.isNotEmpty() }?.let { put("agent", it) }
            options.model.takeIf { it.isNotEmpty() }?.let { put("model", it) }
            // Claude only, and "" is the CLI's own default — the server
            // refuses an empty string against the closed model vocabulary.
            options.subagentModel.takeIf { it.isNotEmpty() }?.let { put("subagentModel", it) }
            options.effort.takeIf { it.isNotEmpty() }?.let { put("effort", it) }
            options.account.takeIf { it.isNotEmpty() }?.let { put("account", it) }
            put("maxParallel", options.maxParallel)
        }
    }
    gate?.let { put("gate", it) }
    startOn?.let { put("startOn", it) }
}

/**
 * `workflows.updateNode`'s patch — addressed by ISSUE (a member's id resolves
 * to its compound node). Same omitted-key rule as above; `budget` is the one
 * key whose explicit null CLEARS.
 */
internal fun updateWorkflowNodeInput(
    workflowId: String,
    issueId: String,
    kind: String?,
    risk: String?,
    touches: List<String>?,
): JsonObject = buildJsonObject {
    put("workflowId", workflowId)
    put("issueId", issueId)
    kind?.let { put("kind", it) }
    risk?.let { put("risk", it) }
    touches?.let { globs ->
        putJsonArray("touches") { globs.forEach { add(JsonPrimitive(it)) } }
    }
}

/**
 * The workflow WRITE path (EXP-981). Reads stay on the two synced shapes; only
 * the member-gated mutations come through tRPC. Every method throws
 * [TrpcException] with the server's own sentence, which the callers render as
 * the surface's error notice rather than translating.
 */
@Singleton
class WorkflowsApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * `workflows.create` — a draft over [issueIds] (backlog issues of ONE
     * repository, in display order). The server names it after the picks when
     * [name] is absent, stamps the integration branch and lays the graph out.
     */
    suspend fun create(
        accountId: String,
        teamId: String,
        issueIds: List<String>,
        name: String? = null,
    ): WorkflowDto = trpc.mutation(
        accountId,
        path = "workflows.create",
        input = CreateWorkflowInput(teamId = teamId, issueIds = issueIds, name = name),
        inputSerializer = CreateWorkflowInput.serializer(),
        outputSerializer = WorkflowMutationResult.serializer(),
    ).workflow

    /**
     * `workflows.update` — the name (always editable) or how the workflow runs
     * (draft only). [clearDevice] unbinds the runner; everything null is left
     * alone. Electric echoes the written row back, so a success needs no local
     * write.
     */
    suspend fun update(
        accountId: String,
        id: String,
        name: String? = null,
        deviceId: String? = null,
        clearDevice: Boolean = false,
        launch: WorkflowLaunch? = null,
        gate: String? = null,
        startOn: String? = null,
    ): WorkflowDto = trpc.mutation(
        accountId,
        path = "workflows.update",
        input = updateWorkflowInput(
            id = id,
            name = name,
            deviceId = deviceId,
            clearDevice = clearDevice,
            launch = launch,
            gate = gate,
            startOn = startOn,
        ),
        inputSerializer = JsonObject.serializer(),
        outputSerializer = WorkflowMutationResult.serializer(),
    ).workflow

    /** `workflows.setIssues` — add or drop issues of a DRAFT, then replan. */
    suspend fun setIssues(
        accountId: String,
        id: String,
        addIssueIds: List<String> = emptyList(),
        removeIssueIds: List<String> = emptyList(),
    ) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.setIssues",
            input = SetIssuesInput(
                id = id,
                addIssueIds = addIssueIds,
                removeIssueIds = removeIssueIds,
            ),
            inputSerializer = SetIssuesInput.serializer(),
        )
    }

    /** `workflows.updateNode` — what the plan declares for one node. */
    suspend fun updateNode(
        accountId: String,
        workflowId: String,
        issueId: String,
        kind: String? = null,
        risk: String? = null,
        touches: List<String>? = null,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.updateNode",
            input = updateWorkflowNodeInput(
                workflowId = workflowId,
                issueId = issueId,
                kind = kind,
                risk = risk,
                touches = touches,
            ),
            inputSerializer = JsonObject.serializer(),
        )
    }

    /** `workflows.replan` — re-derive the compound nodes, layout and metrics. */
    suspend fun replan(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.replan",
            input = WorkflowIdInput(id = id),
            inputSerializer = WorkflowIdInput.serializer(),
        )
    }

    /** `workflows.delete` — permanent; refused while the workflow is live. */
    suspend fun delete(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.delete",
            input = WorkflowIdInput(id = id),
            inputSerializer = WorkflowIdInput.serializer(),
        )
    }
}
