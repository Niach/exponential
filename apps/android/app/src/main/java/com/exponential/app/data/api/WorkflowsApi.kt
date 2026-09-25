package com.exponential.app.data.api

import com.exponential.app.data.db.WorkflowEntity
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

// Mirrors apps/web/src/lib/trpc/workflows.ts (EXP-981). The rows themselves
// arrive over the `workflows` + `workflow_nodes` shapes (WorkflowEntity /
// WorkflowNodeEntity) — this API carries only the mutations: create the draft,
// rename it or change how it runs, add/drop issues, set a node's plan, replan,
// delete — and, from EXP-982, RUNNING one: start/pause/resume/cancel plus the
// two node verdicts a person gives (approve, retry/skip). Any team MEMBER may
// call them (a workflow is work, not a team setting), and every refusal is a
// human sentence the caller shows verbatim. The engine-only procedures
// (`reportNode`/`landNode`/`openFinalPr`) belong to the runner DEVICE and are
// never called from a phone. `list`/`get` exist server-side for MCP; sync is
// the read path here.

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
internal data class WorkflowIdInput(@SerialName("id") val id: String)

/**
 * `workflows.mergeFinalPr`'s answer (EXP-1033). The merge's RESULT arrives
 * over the synced row (`final_pr_state`, the workflow's own status); this flag
 * only says the server accepted it — it is `true` for an already merged PR too.
 */
@Serializable
internal data class WorkflowMergeResult(@SerialName("merged") val merged: Boolean = false)

/** `workflows.approveNode` — the gate, taken back with `approved = false`. */
@Serializable
internal data class ApproveNodeInput(
    @SerialName("nodeId") val nodeId: String,
    @SerialName("approved") val approved: Boolean,
)

/** `workflows.resolveNode` — `retry` or `skip`, nothing else. */
@Serializable
internal data class ResolveNodeInput(
    @SerialName("nodeId") val nodeId: String,
    @SerialName("action") val action: String,
)

/**
 * `workflows.admitNode` (EXP-984) — a follow-up filed mid-run arrived as a
 * `proposed` node; a member takes it into the graph or throws it away.
 */
@Serializable
internal data class AdmitNodeInput(
    @SerialName("nodeId") val nodeId: String,
    @SerialName("admit") val admit: Boolean,
)

/**
 * `workflows.update`'s patch, hand-built: the router applies a key only when
 * it is `!== undefined`, so an OMITTED key means "keep" — and the shared Json
 * (`explicitNulls = false`) would drop a null property from a `@Serializable`
 * class outright, which is exactly how [deviceId] clears the runner ([JsonNull]
 * rides as the explicit "unbind"). The same rule the action editor's patch
 * follows. The phone writes only the name and the runner (EXP-1014): the
 * launch options are picked where the workflow is CREATED and `startOn` is
 * fixed to `contract`, so neither has an encoder here.
 */
internal fun updateWorkflowInput(
    id: String,
    name: String?,
    deviceId: String?,
    clearDevice: Boolean,
): JsonObject = buildJsonObject {
    put("id", id)
    name?.let { put("name", it) }
    when {
        clearDevice -> put("deviceId", JsonNull)
        deviceId != null -> put("deviceId", JsonPrimitive(deviceId))
    }
}

/**
 * `workflows.updateNode`'s patch — addressed by ISSUE (a member's id resolves
 * to its compound node). Same omitted-key rule as above.
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
     *
     * EXP-1014: the phone writes only [name] and the runner — a workflow
     * screen configures nothing: its two models are picked where the workflow
     * is created and `startOn` is fixed to `contract`. The router's `launch` /
     * `startOn` keys stay other clients' business.
     */
    suspend fun update(
        accountId: String,
        id: String,
        name: String? = null,
        deviceId: String? = null,
        clearDevice: Boolean = false,
    ): WorkflowDto = trpc.mutation(
        accountId,
        path = "workflows.update",
        input = updateWorkflowInput(
            id = id,
            name = name,
            deviceId = deviceId,
            clearDevice = clearDevice,
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

    /**
     * `workflows.updateNode` — what the plan declares for one node. Kind and
     * touches are draft-only (the server refuses them later); risk stays
     * adjustable at any status.
     */
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

    // ── Running a workflow (EXP-982) ────────────────────────────────────────
    // The server only flips INTENT; the deterministic engine on the runner
    // device does the work off the synced rows. Every refusal below is a human
    // sentence ([WorkflowView.startBlocker] says the same ones up front).

    /** `workflows.start` — the draft becomes `running`; the engine takes over. */
    suspend fun start(accountId: String, id: String): WorkflowDto = trpc.mutation(
        accountId,
        path = "workflows.start",
        input = WorkflowIdInput(id = id),
        inputSerializer = WorkflowIdInput.serializer(),
        outputSerializer = WorkflowMutationResult.serializer(),
    ).workflow

    /** `workflows.pause` — nothing new starts or lands; live runs finish. */
    suspend fun pause(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.pause",
            input = WorkflowIdInput(id = id),
            inputSerializer = WorkflowIdInput.serializer(),
        )
    }

    suspend fun resume(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.resume",
            input = WorkflowIdInput(id = id),
            inputSerializer = WorkflowIdInput.serializer(),
        )
    }

    /** `workflows.cancel` — live runs end and the branch goes. */
    suspend fun cancel(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.cancel",
            input = WorkflowIdInput(id = id),
            inputSerializer = WorkflowIdInput.serializer(),
        )
    }

    /**
     * `workflows.mergeFinalPr` (EXP-1033) — squash-merge the workflow's ONE
     * final pull request (its branch → the default branch), the one human
     * review of the whole run. The server completes the workflow itself, so
     * nothing is echoed locally: Electric carries the merged row back.
     * Idempotent for a PR that already landed.
     */
    suspend fun mergeFinalPr(accountId: String, id: String): Boolean = trpc.mutation(
        accountId,
        path = "workflows.mergeFinalPr",
        input = WorkflowIdInput(id = id),
        inputSerializer = WorkflowIdInput.serializer(),
        outputSerializer = WorkflowMergeResult.serializer(),
    ).merged

    /** `workflows.approveNode` — clear a node's PR for the merge train. */
    suspend fun approveNode(accountId: String, nodeId: String, approved: Boolean = true) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.approveNode",
            input = ApproveNodeInput(nodeId = nodeId, approved = approved),
            inputSerializer = ApproveNodeInput.serializer(),
        )
    }

    /**
     * `workflows.resolveNode` — a person unsticks a failed node: [NODE_RETRY]
     * gives it a fresh attempt, [NODE_SKIP] takes it out so its dependents can
     * go on without it.
     */
    suspend fun resolveNode(accountId: String, nodeId: String, action: String) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.resolveNode",
            input = ResolveNodeInput(nodeId = nodeId, action = action),
            inputSerializer = ResolveNodeInput.serializer(),
        )
    }

    /**
     * `workflows.admitNode` (EXP-984) — a `proposed` node is a follow-up issue
     * somebody filed mid-run: admitting it makes it part of the workflow,
     * dismissing it deletes the node (never the issue). Either way the server
     * replans.
     */
    suspend fun admitNode(accountId: String, nodeId: String, admit: Boolean) {
        trpc.mutationUnit(
            accountId,
            path = "workflows.admitNode",
            input = AdmitNodeInput(nodeId = nodeId, admit = admit),
            inputSerializer = AdmitNodeInput.serializer(),
        )
    }

    companion object {
        const val NODE_RETRY = "retry"
        const val NODE_SKIP = "skip"
    }
}
