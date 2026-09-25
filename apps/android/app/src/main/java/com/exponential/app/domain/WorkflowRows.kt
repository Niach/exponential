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

/**
 * `workflows.launch`: what every node's run starts with, as the STORED jsonb
 * carries it — every vintage of it. The strict launch a run actually reads is
 * [NormalizedWorkflowLaunch]; everything marked deprecated below is decoded
 * only so an old row still folds into it.
 */
data class WorkflowLaunch(
    val agent: String = "",
    val model: String = "",
    /**
     * EXP-1029: the STRONG model — contract, integration and `risk: high`
     * nodes, and every agent review. CARRIED: the phone never edits it, and
     * the encoder omits it so the server keeps the stored value. null = not
     * set, and [normalizedLaunch] then folds an old row's pins in.
     */
    val strongModel: String? = null,
    /** Deprecated (EXP-1029): claude's subagents take [model] now. */
    val subagentModel: String = "",
    /** Deprecated (EXP-1029): a workflow run has no effort pick. */
    val effort: String = "",
    /** An agent profile id on the runner device; "" = its active login. */
    val account: String = "",
    /** Deprecated (EXP-1029): the engine owns how many nodes run at once. */
    val maxParallel: Int = DomainContract.workflowMaxParallelDefault,
    /** Deprecated (EXP-1029): every agent review runs on [strongModel]. */
    val reviewModel: String = "",
    /**
     * Deprecated (EXP-1029): the per-PHASE model pins of EXP-1002 fold into
     * [strongModel]. Still decoded (old rows carry them) and still CARRIED on
     * the wire, so a phone write never erases what web/desktop stored.
     */
    val contractModel: String? = null,
    val integrationModel: String? = null,
    val riskModel: String? = null,
)

/**
 * EXP-1029: the STRICT launch every node run reads — two models, no more.
 * [model] is the CHEAP one (leaf nodes, and the subagents inside every node
 * run), [strongModel] the capable one (contract nodes, integration nodes,
 * `risk: high` nodes and EVERY agent review). Mirrors web
 * `lib/workflow-launch.ts` and Rust `coding::workflows::launch`, same rules,
 * same names.
 */
data class NormalizedWorkflowLaunch(
    val agent: String,
    val model: String,
    val strongModel: String,
    /** An agent profile id on the runner; "" = its active login. */
    val account: String = "",
)

/** The agent a launch runs on when the row names none this build knows. */
private const val DEFAULT_WORKFLOW_AGENT = "claude"

/** That agent's two models, from the contract. */
private fun launchDefaults(agent: String): Pair<String, String> = when (agent) {
    "codex" -> DomainContract.workflowLaunchCodexModel to
        DomainContract.workflowLaunchCodexStrongModel
    else -> DomainContract.workflowLaunchClaudeModel to
        DomainContract.workflowLaunchClaudeStrongModel
}

/** A stored string that carries a value: trimmed and non-blank, else null. */
private fun text(value: String?): String? = value?.trim()?.takeIf { it.isNotEmpty() }

/**
 * The stored `workflows.launch` (any vintage) → the launch a run reads:
 * - `agent`: `claude` or `codex`; anything else → `claude`.
 * - `model`: the stored one, else that agent's default.
 * - `strongModel`: the stored one; else the first set of the deprecated pins
 *   (`reviewModel`, `riskModel`, `contractModel`, `integrationModel`, in that
 *   order); else that agent's default strong model.
 * - `subagentModel`, `effort` and `maxParallel` are dropped.
 */
fun normalizedLaunch(launch: WorkflowLaunch): NormalizedWorkflowLaunch {
    val agent = text(launch.agent)
        ?.takeIf { it in DomainContract.workflowLaunchAgents }
        ?: DEFAULT_WORKFLOW_AGENT
    val (defaultModel, defaultStrong) = launchDefaults(agent)
    val legacy = listOf(
        launch.reviewModel,
        launch.riskModel,
        launch.contractModel,
        launch.integrationModel,
    ).firstNotNullOfOrNull { text(it) }
    return NormalizedWorkflowLaunch(
        agent = agent,
        model = text(launch.model) ?: defaultModel,
        strongModel = text(launch.strongModel) ?: legacy ?: defaultStrong,
        account = text(launch.account).orEmpty(),
    )
}

/**
 * The model ONE node's run spawns on: the strong model for a `contract` or an
 * `integration` node and for any `risk: high` node, else the cheap one.
 */
fun modelForNode(launch: NormalizedWorkflowLaunch, kind: String, risk: String): String {
    val strong = kind == DomainContract.wfNodeKindContract ||
        kind == DomainContract.wfNodeKindIntegration ||
        risk == DomainContract.wfRiskHigh
    return if (strong) launch.strongModel else launch.model
}

/** The model EVERY agent review runs on: the strong one, whatever the node. */
fun reviewModelFor(launch: NormalizedWorkflowLaunch): String = launch.strongModel

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
        strongModel = obj.string("strongModel").ifEmpty { null },
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

/** The workflow's shape line + cycle note source, off its synced row. */
val WorkflowEntity.shape: WorkflowView.Shape get() = workflowMetrics(metrics)

/** The launch options off the synced row. */
val WorkflowEntity.launchOptions: WorkflowLaunch get() = workflowLaunch(launch)

/** A node's caption inputs, in the shared rule's shape. */
val WorkflowNodeEntity.captionNode: WorkflowView.CaptionNode
    get() = WorkflowView.CaptionNode(kind = kind, state = state, risk = risk)

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

