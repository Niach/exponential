package com.exponential.app.domain

import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull

/**
 * EXP-981: the jsonb columns of the two workflow shapes, read TOLERANTLY off
 * the raw text Room stores (the `AutomationTrigger.parse` precedent): unknown
 * keys are ignored, a missing or malformed key falls back to its default, and
 * nothing here ever throws — a workflow row must render even when a newer
 * server put something in `launch` this build has no name for.
 */

/** `workflows.launch`: what every node's run starts with. */
data class WorkflowLaunch(
    val agent: String = "",
    val model: String = "",
    /** Claude only: the model its subagents run on. */
    val subagentModel: String = "",
    val effort: String = "",
    /** An agent profile id on the runner device; "" = its active login. */
    val account: String = "",
    val maxParallel: Int = DomainContract.workflowMaxParallelDefault,
)

/** `workflow_nodes.budget`: crossing either pauses the node and notifies. */
data class WorkflowNodeBudget(
    val tokens: Int? = null,
    val minutes: Int? = null,
)

private fun parseObject(raw: String?): JsonObject? {
    val text = raw?.trim().orEmpty()
    if (text.isEmpty() || !text.startsWith("{")) return null
    return runCatching { Json.parseToJsonElement(text) as? JsonObject }.getOrNull()
}

private fun JsonObject.string(key: String): String =
    (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content.orEmpty()

private fun JsonObject.int(key: String): Int? = (this[key] as? JsonPrimitive)?.let {
    it.intOrNull ?: it.content.trim().toIntOrNull()
}

/** The workflow's launch options, every field defaulted. */
fun workflowLaunch(raw: String?): WorkflowLaunch {
    val obj = parseObject(raw) ?: return WorkflowLaunch()
    return WorkflowLaunch(
        agent = obj.string("agent"),
        model = obj.string("model"),
        subagentModel = obj.string("subagentModel"),
        effort = obj.string("effort"),
        account = obj.string("account"),
        maxParallel = obj.int("maxParallel")?.takeIf { it >= 1 }
            ?: DomainContract.workflowMaxParallelDefault,
    )
}

/** A node's token/minute budget, or null when it carries none. */
fun workflowNodeBudget(raw: String?): WorkflowNodeBudget? {
    val obj = parseObject(raw) ?: return null
    val budget = WorkflowNodeBudget(tokens = obj.int("tokens"), minutes = obj.int("minutes"))
    return budget.takeIf { it.tokens != null || it.minutes != null }
}

/** `workflows.metrics`: the plan's shape, as [WorkflowView] reads it. */
fun workflowMetrics(raw: String?): WorkflowView.Shape {
    val obj = parseObject(raw) ?: return WorkflowView.Shape()
    return WorkflowView.Shape(
        nodes = obj.int("nodes") ?: 0,
        edges = obj.int("edges") ?: 0,
        depth = obj.int("depth") ?: 0,
        width = obj.int("width") ?: 0,
        cycles = (obj["cycles"] as? JsonArray)
            ?.mapNotNull { cycle ->
                (cycle as? JsonArray)
                    ?.mapNotNull { (it as? JsonPrimitive)?.content }
                    ?.takeIf { it.isNotEmpty() }
            }
            .orEmpty(),
        cycleEdges = (obj["cycleEdges"] as? JsonArray)
            ?.mapNotNull { (it as? JsonPrimitive)?.content }
            .orEmpty(),
    )
}

/** The workflow's shape line + cycle note source, off its synced row. */
val WorkflowEntity.shape: WorkflowView.Shape get() = workflowMetrics(metrics)

/** The launch options off the synced row. */
val WorkflowEntity.launchOptions: WorkflowLaunch get() = workflowLaunch(launch)

/** A node's caption inputs, in the shared rule's shape. */
val WorkflowNodeEntity.captionNode: WorkflowView.CaptionNode
    get() = WorkflowView.CaptionNode(kind = kind, state = state, risk = risk)

/** A node as the edge rule sees it (its issue plus a compound's members). */
val WorkflowNodeEntity.edgeNode: WorkflowView.EdgeNode
    get() = WorkflowView.EdgeNode(id = id, issueId = issueId, memberIssueIds = memberIssueIds)

/** Every issue this node covers — its representative first. */
val WorkflowNodeEntity.coveredIssueIds: List<String> get() = listOf(issueId) + memberIssueIds

