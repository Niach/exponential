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
    /**
     * EXP-984: the model agent reviews run on; "" = the engine picks one (a
     * `risk: high` node is ALWAYS reviewed on a model other than its author's).
     */
    val reviewModel: String = "",
    /**
     * EXP-1002: the per-PHASE model pins (`contract` nodes, `integration`
     * nodes, any `risk: high` node). No picker here — they are CARRIED, so a
     * phone edit of another option (the router replaces the whole `launch`)
     * never erases what web/desktop pinned. null = no pin.
     */
    val contractModel: String? = null,
    val integrationModel: String? = null,
    val riskModel: String? = null,
) {
    /**
     * The pins come out of the AGENT's model vocabulary (the server validates
     * them against it), so an agent switch drops them like it drops `model`.
     */
    fun withoutPhaseModels(): WorkflowLaunch =
        copy(contractModel = null, integrationModel = null, riskModel = null)
}

/**
 * EXP-984: an executable check the reviewer RAN. An agent's opinion is
 * advisory; a passing oracle is what turns its approval into the approval.
 */
data class WorkflowReviewOracle(
    val command: String,
    val passed: Boolean,
)

/** `workflow_nodes.review`: the latest submitted agent verdict. */
data class WorkflowNodeReview(
    val verdict: String,
    val findings: String = "",
    val oracle: WorkflowReviewOracle? = null,
    /** The model that reviewed; "" = the row did not say. */
    val model: String = "",
    val round: Int = 0,
    val at: String = "",
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

private fun JsonObject.boolean(key: String): Boolean =
    (this[key] as? JsonPrimitive)?.content?.trim().equals("true", ignoreCase = true)

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
        reviewModel = obj.string("reviewModel"),
        contractModel = obj.string("contractModel").ifEmpty { null },
        integrationModel = obj.string("integrationModel").ifEmpty { null },
        riskModel = obj.string("riskModel").ifEmpty { null },
    )
}

/**
 * EXP-984: the node's latest agent review, or null while nobody reviewed it.
 * A cell without a verdict is nothing to show — the review block renders off
 * the verdict alone, so a half-written object never paints an empty card.
 */
fun workflowNodeReview(raw: String?): WorkflowNodeReview? {
    val obj = parseObject(raw) ?: return null
    val verdict = obj.string("verdict").takeIf { it.isNotEmpty() } ?: return null
    val oracle = (obj["oracle"] as? JsonObject)?.let { cell ->
        val command = cell.string("command")
        if (command.isEmpty()) null else WorkflowReviewOracle(command, cell.boolean("passed"))
    }
    return WorkflowNodeReview(
        verdict = verdict,
        findings = obj.string("findings"),
        oracle = oracle,
        model = obj.string("model"),
        round = obj.int("round") ?: 0,
        at = obj.string("at"),
    )
}

/** The review line's inputs, in the shared rule's shape. */
val WorkflowNodeReview.line: WorkflowView.ReviewLine
    get() = WorkflowView.ReviewLine(
        verdict = verdict,
        round = round,
        oraclePassed = oracle?.passed,
    )

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

/**
 * EXP-984: every NUMERIC key of `workflows.metrics` — the run's counters live
 * in the same jsonb as the shape keys, so the Metrics section reads them all
 * at once ([WorkflowView.metricRows]). Anything that is not a number (a list,
 * a word a newer server wrote) is simply absent, which counts as zero.
 */
fun workflowMetricCounters(raw: String?): Map<String, Int> {
    val obj = parseObject(raw) ?: return emptyMap()
    val counters = LinkedHashMap<String, Int>()
    for (key in obj.keys) obj.int(key)?.let { counters[key] = it }
    return counters
}

/** The workflow's shape line + cycle note source, off its synced row. */
val WorkflowEntity.shape: WorkflowView.Shape get() = workflowMetrics(metrics)

/** The run's counters off the synced row, as the Metrics section reads them. */
val WorkflowEntity.metricCounters: Map<String, Int> get() = workflowMetricCounters(metrics)

/** The launch options off the synced row. */
val WorkflowEntity.launchOptions: WorkflowLaunch get() = workflowLaunch(launch)

/** A node's caption inputs, in the shared rule's shape. */
val WorkflowNodeEntity.captionNode: WorkflowView.CaptionNode
    get() = WorkflowView.CaptionNode(kind = kind, state = state, risk = risk)

/** A node as the merge train sees it (EXP-982): its landing order + gate stamp. */
val WorkflowNodeEntity.trainNode: WorkflowView.TrainNode
    get() = WorkflowView.TrainNode(
        id = id,
        kind = kind,
        state = state,
        wave = wave ?: 0,
        lane = lane ?: 0,
        approvedAt = approvedAt,
    )

/**
 * A node as the edge rule sees it (its issue plus a compound's members, and
 * the serialization edges the engine wrote onto it, EXP-983).
 */
val WorkflowNodeEntity.edgeNode: WorkflowView.EdgeNode
    get() = WorkflowView.EdgeNode(
        id = id,
        issueId = issueId,
        memberIssueIds = memberIssueIds,
        afterNodeIds = afterNodeIds,
    )

/** Every issue this node covers — its representative first. */
val WorkflowNodeEntity.coveredIssueIds: List<String> get() = listOf(issueId) + memberIssueIds

