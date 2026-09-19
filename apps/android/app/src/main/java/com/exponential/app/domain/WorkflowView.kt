package com.exponential.app.domain

/**
 * EXP-981: what every client SAYS about a workflow. The graph's geometry is
 * the server's (`wave`/`lane` on the synced nodes); this is the rest — bands,
 * captions, the edges between nodes — mirrored ×4 (web `lib/workflow-view.ts`,
 * iOS `WorkflowView.swift`, desktop `domain::workflow_view`) and locked by the
 * contract fixture `domain-contract/fixtures/workflow-view.json`. All strings
 * byte-identical.
 *
 * NOT to be confused with [WorkflowCaption], which reads the agent feed's
 * Claude Code Workflow TOOL card — an unrelated concept that owns the
 * contract's `workflowStatus` key. A workflow of THIS kind wears `wf*`.
 */
object WorkflowView {

    /** The list's three bands, in order. Flat rows under each, no row buttons. */
    enum class Band(val key: String, val title: String) {
        Running("running", "Running"),
        Draft("draft", "Draft"),
        Done("done", "Done"),
    }

    val BANDS: List<Band> = listOf(Band.Running, Band.Draft, Band.Done)

    const val WORKFLOWS_TITLE = "Workflows"
    const val WORKFLOWS_EMPTY_TITLE = "No workflows yet"
    const val WORKFLOWS_EMPTY_BODY =
        "Select backlog issues on a board and choose Create workflow to plan them as one parallel run."
    const val PLAN_WORKFLOW_LABEL = "Plan"
    const val DELETE_WORKFLOW_LABEL = "Delete workflow"

    /** The bulk bar's play menu (EXP-981), in order. */
    const val START_AS_BATCH_LABEL = "Start as batch"
    const val START_AS_STACK_LABEL = "Start as stack"
    const val CREATE_WORKFLOW_LABEL = "Create workflow…"

    /**
     * `paused` is a running workflow someone held; `cancelled` is over. An
     * unknown status (a newer server) lands in Done rather than vanishing.
     */
    fun band(status: String): Band = when (status) {
        DomainContract.wfStatusRunning, DomainContract.wfStatusPaused -> Band.Running
        DomainContract.wfStatusDraft -> Band.Draft
        else -> Band.Done
    }

    /**
     * `workflows.metrics`: the plan's shape. [edges] and [cycleEdges] ride
     * along because the same jsonb carries them; only [nodes], [depth],
     * [width] and [cycles] are read by the rules below.
     */
    data class Shape(
        val nodes: Int = 0,
        val edges: Int = 0,
        val depth: Int = 0,
        val width: Int = 0,
        /** One entry per blocking cycle: its issue identifiers. Empty = startable. */
        val cycles: List<List<String>> = emptyList(),
        /** `<fromNodeId>\n<toNodeId>` of every edge inside a cycle. */
        val cycleEdges: List<String> = emptyList(),
    )

    /** `12 nodes · depth 3 · width 8`; `1 node · depth 1 · width 1`. */
    fun shapeLine(metrics: Shape): String {
        val nodes = if (metrics.nodes == 1) "1 node" else "${metrics.nodes} nodes"
        return "$nodes · depth ${metrics.depth} · width ${metrics.width}"
    }

    /** Null while the workflow could start; else the cycles spelled out. */
    fun cycleNote(metrics: Shape): String? {
        if (metrics.cycles.isEmpty()) return null
        val spelled = metrics.cycles.joinToString("; ") { it.joinToString(", ") }
        return "These issues block each other in a cycle: $spelled. Remove one relation to start."
    }

    private val STATE_LABELS = mapOf(
        DomainContract.wfNodeStateProposed to "Proposed",
        DomainContract.wfNodeStateBlocked to "Blocked",
        DomainContract.wfNodeStateReady to "Ready",
        DomainContract.wfNodeStateRunning to "Running",
        DomainContract.wfNodeStateWaiting to "Waiting",
        DomainContract.wfNodeStateInReview to "In review",
        DomainContract.wfNodeStateUpdating to "Updating",
        DomainContract.wfNodeStateLanded to "Landed",
        DomainContract.wfNodeStateFailed to "Failed",
        DomainContract.wfNodeStateSkipped to "Skipped",
        DomainContract.wfNodeStatePaused to "Paused",
    )

    private val KIND_LABELS = mapOf(
        DomainContract.wfNodeKindContract to "Contract",
        DomainContract.wfNodeKindLeaf to "Leaf",
        DomainContract.wfNodeKindIntegration to "Integration",
    )

    fun nodeStateLabel(state: String): String = STATE_LABELS[state] ?: state

    fun nodeKindLabel(kind: String): String = KIND_LABELS[kind] ?: kind

    /** The gate's three words, and the start rule's three (EXP-981). */
    fun gateLabel(gate: String): String = when (gate) {
        DomainContract.wfGateNone -> "No gate"
        DomainContract.wfGateAgent -> "Agent review"
        DomainContract.wfGateHuman -> "Human review"
        else -> gate
    }

    fun startOnLabel(startOn: String): String = when (startOn) {
        DomainContract.wfStartOnContract -> "On contract"
        DomainContract.wfStartOnPrOpen -> "On PR open"
        DomainContract.wfStartOnLanded -> "When landed"
        else -> startOn
    }

    fun riskLabel(risk: String): String = risk.replaceFirstChar { it.uppercaseChar() }

    /**
     * The tone a node's state paints in. `waiting` is the ONLY amber one (and
     * the only one that pushes): amber means "a person is needed".
     */
    enum class Tone(val key: String) {
        Muted("muted"),
        Active("active"),
        Amber("amber"),
        Success("success"),
        Danger("danger"),
    }

    fun nodeTone(state: String): Tone = when (state) {
        DomainContract.wfNodeStateWaiting -> Tone.Amber
        DomainContract.wfNodeStateFailed -> Tone.Danger
        DomainContract.wfNodeStateLanded -> Tone.Success
        DomainContract.wfNodeStateRunning,
        DomainContract.wfNodeStateUpdating,
        DomainContract.wfNodeStateInReview,
        -> Tone.Active
        else -> Tone.Muted
    }

    data class CaptionNode(val kind: String, val state: String, val risk: String)

    /**
     * The ONE caption under a node. A draft has no states worth reading yet, so
     * it names the plan (`Contract`, `Leaf · high risk`); a started workflow
     * names the state, prefixed by the kind only for the two special nodes
     * (`Contract · Running`, `In review`).
     */
    fun nodeCaption(node: CaptionNode, workflowStatus: String): String {
        if (workflowStatus == DomainContract.wfStatusDraft) {
            val kind = nodeKindLabel(node.kind)
            return if (node.risk == DomainContract.wfRiskHigh) "$kind · high risk" else kind
        }
        val state = nodeStateLabel(node.state)
        return if (node.kind == DomainContract.wfNodeKindLeaf) {
            state
        } else {
            "${nodeKindLabel(node.kind)} · $state"
        }
    }

    /**
     * `EXP-14 +3` for a compound node (a parent run as one batch with its
     * sub-issues), the bare identifier otherwise.
     */
    fun nodeTitle(identifier: String, memberCount: Int): String =
        if (memberCount > 0) "$identifier +$memberCount" else identifier

    data class EdgeNode(
        val id: String,
        val issueId: String,
        val memberIssueIds: List<String>,
    )

    data class EdgeRelation(
        val type: String,
        val issueId: String,
        val relatedIssueId: String,
    )

    data class Edge(
        val from: String,
        val to: String,
        /** Inside a blocking cycle (`metrics.cycleEdges`): drawn red. */
        val cycle: Boolean,
    )

    /**
     * The edges between a workflow's nodes, from the synced `blocks` relations:
     * a relation between ANY two covered issues of two different nodes (the
     * server's `nodeEdges`). One edge per node pair, ordered by (from, to) node
     * id. [cycleEdges] = the workflow's `metrics.cycleEdges` (`<from>\n<to>`).
     */
    fun edges(
        nodes: List<EdgeNode>,
        relations: List<EdgeRelation>,
        cycleEdges: List<String> = emptyList(),
    ): List<Edge> {
        val nodeOf = HashMap<String, String>()
        for (node in nodes) {
            nodeOf[node.issueId] = node.id
            for (member in node.memberIssueIds) nodeOf[member] = node.id
        }
        val onCycle = cycleEdges.toSet()
        val seen = HashSet<String>()
        val edges = ArrayList<Edge>()
        for (relation in relations) {
            if (relation.type != DomainContract.issueRelationTypeBlocks) continue
            val from = nodeOf[relation.issueId] ?: continue
            val to = nodeOf[relation.relatedIssueId] ?: continue
            if (from == to) continue
            val key = "$from\n$to"
            if (!seen.add(key)) continue
            edges.add(Edge(from = from, to = to, cycle = key in onCycle))
        }
        return edges.sortedWith(compareBy({ it.from }, { it.to }))
    }

    // ── Running a workflow (EXP-982) ────────────────────────────────────────

    const val START_WORKFLOW_LABEL = "Start"
    const val PAUSE_WORKFLOW_LABEL = "Pause"
    const val RESUME_WORKFLOW_LABEL = "Resume"
    const val CANCEL_WORKFLOW_LABEL = "Cancel workflow"
    const val CANCEL_WORKFLOW_CONFIRM =
        "Its live runs end and its branch is deleted. Nothing reached the default branch."
    const val APPROVE_NODE_LABEL = "Approve and land"
    const val WITHDRAW_APPROVAL_LABEL = "Withdraw approval"
    const val MERGE_TRAIN_TITLE = "Merge train"
    const val MERGE_TRAIN_EMPTY = "Nothing is waiting to land."
    const val FINAL_PR_TITLE = "Final pull request"

    /** The four fields Start is judged on, off the synced workflow row. */
    data class Startable(
        val status: String,
        val deviceId: String?,
        val repositoryId: String?,
        val startOn: String,
    )

    /**
     * Why Start is disabled, or null when the draft can start. One reason, the
     * most fundamental first; the server refuses with the same sentences.
     */
    fun startBlocker(workflow: Startable, metrics: Shape): String? {
        if (workflow.status != DomainContract.wfStatusDraft) return "The workflow has already started."
        if (metrics.nodes == 0) return "The workflow has no issues."
        cycleNote(metrics)?.let { return it }
        if (workflow.repositoryId.isNullOrEmpty()) return "The workflow's repository is gone."
        if (workflow.deviceId.isNullOrEmpty()) return "Pick the device that runs this workflow first."
        if (workflow.startOn != DomainContract.wfStartOnLanded) {
            return "Only \"When landed\" starts are available yet."
        }
        return null
    }

    /**
     * A node lands without a person only when the workflow has no gate AND it
     * is not the contract (always human-gated). Mirrors the server.
     */
    fun nodeNeedsApproval(gate: String, kind: String): Boolean =
        kind == DomainContract.wfNodeKindContract || gate != DomainContract.wfGateNone

    /** A node as the merge train reads it. [approvedAt] null = not approved. */
    data class TrainNode(
        val id: String,
        val kind: String,
        val state: String,
        val wave: Int,
        val lane: Int,
        val approvedAt: String?,
    )

    enum class TrainStep(val key: String) {
        Next("next"),
        Queued("queued"),
        NeedsApproval("needs-approval"),
        Updating("updating"),
    }

    data class TrainEntry(val id: String, val step: TrainStep)

    /**
     * The merge train: every node whose PR is up (`in_review`, or `updating`
     * while it merges the trunk in), in landing order (wave, then lane). The
     * FIRST node that is cleared to land is `next`; cleared ones behind it are
     * `queued`; one still waiting for a person says so.
     */
    fun mergeTrain(nodes: List<TrainNode>, gate: String): List<TrainEntry> {
        val waiting = nodes
            .filter {
                it.state == DomainContract.wfNodeStateInReview ||
                    it.state == DomainContract.wfNodeStateUpdating
            }
            .sortedWith(compareBy({ it.wave }, { it.lane }, { it.id }))
        var nextTaken = false
        return waiting.map { node ->
            when {
                node.state == DomainContract.wfNodeStateUpdating ->
                    TrainEntry(node.id, TrainStep.Updating)
                nodeNeedsApproval(gate, node.kind) && node.approvedAt.isNullOrEmpty() ->
                    TrainEntry(node.id, TrainStep.NeedsApproval)
                nextTaken -> TrainEntry(node.id, TrainStep.Queued)
                else -> {
                    nextTaken = true
                    TrainEntry(node.id, TrainStep.Next)
                }
            }
        }
    }

    fun trainStepLabel(step: TrainStep): String = when (step) {
        TrainStep.Next -> "Landing next"
        TrainStep.Queued -> "Queued"
        TrainStep.NeedsApproval -> "Needs approval"
        TrainStep.Updating -> "Merging the trunk in"
    }

    /**
     * The final-PR node's caption, or null while the node is not drawn: it
     * appears once every node landed (or was skipped), after the last wave.
     */
    fun finalPrCaption(
        nodeStates: List<String>,
        finalPrState: String?,
        finalPrNumber: Int?,
    ): String? {
        if (nodeStates.isEmpty()) return null
        val allIn = nodeStates.all {
            it == DomainContract.wfNodeStateLanded || it == DomainContract.wfNodeStateSkipped
        }
        if (!allIn && finalPrNumber == null) return null
        if (finalPrNumber == null) return "Opening the pull request"
        val label = when (finalPrState) {
            DomainContract.prStateMerged -> "Merged"
            DomainContract.prStateClosed -> "Closed"
            else -> "Open"
        }
        return "#$finalPrNumber · $label"
    }

    const val RETRY_NODE_LABEL = "Retry"
    const val SKIP_NODE_LABEL = "Skip"
    const val SKIP_NODE_CONFIRM =
        "Its dependents go on without it. The node's work is not part of the final pull request."

    /**
     * A list row's secondary text: the shape line, led by the status word for
     * the two statuses a band alone does not tell apart.
     */
    fun rowSubtitle(status: String, metrics: Shape): String {
        val shape = shapeLine(metrics)
        return when (status) {
            DomainContract.wfStatusPaused -> "Paused · $shape"
            DomainContract.wfStatusCancelled -> "Cancelled · $shape"
            else -> shape
        }
    }
}
