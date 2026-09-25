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
    )

    private val KIND_LABELS = mapOf(
        DomainContract.wfNodeKindContract to "Contract",
        DomainContract.wfNodeKindLeaf to "Leaf",
        DomainContract.wfNodeKindIntegration to "Integration",
    )

    fun nodeStateLabel(state: String): String = STATE_LABELS[state] ?: state

    fun nodeKindLabel(kind: String): String = KIND_LABELS[kind] ?: kind

    /** The start rule's three words (EXP-981). */
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
     * The ONE caption under a node: the bare STATE label once the workflow has
     * started (`Running`, `In review`, `Landed`), nothing at all in a draft.
     * The kind and the risk are the node sheet's (EXP-1014: no `Leaf`, no
     * `Contract · high risk` beside the chips — the chip names the issue, the
     * caption says only what is happening to it).
     */
    fun nodeCaption(node: CaptionNode, workflowStatus: String): String {
        if (workflowStatus == DomainContract.wfStatusDraft) return ""
        return nodeStateLabel(node.state)
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
        /** EXP-983: engine-written serialization edges (`after_node_ids`). */
        val afterNodeIds: List<String> = emptyList(),
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
        /**
         * EXP-983: not a `blocks` relation but a SERIALIZATION edge the engine
         * added after two siblings' work collided: `to` merges `from` in first.
         */
        val serial: Boolean = false,
    )

    /**
     * The edges between a workflow's nodes, from the synced `blocks` relations:
     * a relation between ANY two covered issues of two different nodes (the
     * server's `nodeEdges`). One edge per node pair, ordered by (from, to) node
     * id. [cycleEdges] = the workflow's `metrics.cycleEdges` (`<from>\n<to>`).
     * The engine's serialization edges (EXP-983) join them unless the pair
     * already has one.
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
        // Serialization edges, unless a real edge already joins the pair.
        val known = nodes.mapTo(HashSet()) { it.id }
        for (node in nodes) {
            for (from in node.afterNodeIds) {
                if (from !in known || from == node.id) continue
                if (!seen.add("$from\n${node.id}")) continue
                edges.add(Edge(from = from, to = node.id, cycle = false, serial = true))
            }
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

    /**
     * EXP-1033: the ONE human review of the whole run — squash-merging the
     * workflow's final pull request from the workflow screen.
     */
    const val MERGE_FINAL_PR_LABEL = "Merge"
    const val MERGE_FINAL_PR_CONFIRM =
        "The workflow's branch is squash-merged into the default branch and the run is done."

    /** The strip over the graph that lists the runs that are up, one tap away. */
    const val RUNNING_NOW_LABEL = "Running now"

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
        // EXP-983: every `start_on` runs now — speculative starts included.
        return null
    }

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
     * `queued`; one no review approved yet says so (EXP-1010: the agent review
     * is the only gate, a person may approve by hand).
     */
    fun mergeTrain(nodes: List<TrainNode>): List<TrainEntry> {
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
                node.approvedAt.isNullOrEmpty() ->
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
        // EXP-984: a `proposed` node was never admitted; it is not part of the
        // run, so nothing waits for it.
        val real = nodeStates.filter { it != DomainContract.wfNodeStateProposed }
        if (real.isEmpty()) return null
        val allIn = real.all {
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

    // ── Speculative starts (EXP-983) ────────────────────────────────────────

    /** How an edge is drawn. Grey solid is the default; the others say something. */
    enum class EdgeStyle(val key: String) {
        Plain("plain"),
        Cycle("cycle"),
        Stale("stale"),
        Landed("landed"),
        Speculative("speculative"),
    }

    private val STARTED_STATES = setOf(
        DomainContract.wfNodeStateRunning,
        DomainContract.wfNodeStateWaiting,
        DomainContract.wfNodeStateInReview,
        DomainContract.wfNodeStateUpdating,
    )

    /**
     * - [EdgeStyle.Cycle] (red): inside a blocking cycle.
     * - [EdgeStyle.Stale] (red): upstream moved and the dependent is merging it
     *   in (`to` is `updating`).
     * - [EdgeStyle.Landed] (green): the blocker landed.
     * - [EdgeStyle.Speculative] (dashed): the dependent started before its
     *   blocker landed, or the edge is a serialization edge.
     * - [EdgeStyle.Plain] (grey): nothing to say yet.
     */
    fun edgeStyle(edge: Edge, fromState: String, toState: String): EdgeStyle = when {
        edge.cycle -> EdgeStyle.Cycle
        fromState == DomainContract.wfNodeStateLanded -> EdgeStyle.Landed
        toState == DomainContract.wfNodeStateUpdating -> EdgeStyle.Stale
        edge.serial || toState in STARTED_STATES -> EdgeStyle.Speculative
        else -> EdgeStyle.Plain
    }

    /** The node panel's line once a node announced its contract. */
    const val CONTRACT_PUBLISHED_LABEL = "Contract published"

    // ── Review gate, dynamic graphs, budgets, metrics (EXP-984) ─────────────

    const val ADMIT_NODE_LABEL = "Admit"
    const val DISMISS_NODE_LABEL = "Dismiss"
    const val PROPOSED_NODE_NOTE =
        "Filed during the run. Admit it into the workflow or dismiss it."
    const val AGENT_REVIEW_TITLE = "Agent review"
    const val METRICS_TITLE = "Metrics"

    /**
     * EXP-1014: the node sheet's read-only line naming what the node's run
     * spawns on ([modelForNode]). Nothing on a workflow screen CONFIGURES a
     * model any more — the two the launch carries are picked where the
     * workflow is created.
     */
    const val NODE_MODEL_LABEL = "Model"

    /**
     * EXP-1014: the chip of a node whose issue row has not synced yet — the
     * identifier slot shows the first 8 characters of the issue id, the title
     * this line. Byte-identical ×4.
     */
    const val NODE_UNSYNCED_TITLE = "Not synced yet"

    /**
     * The three fields [reviewLine] reads off `workflow_nodes.review`.
     * [oraclePassed] null = the reviewer ran no executable check.
     */
    data class ReviewLine(
        val verdict: String,
        val round: Int,
        val oraclePassed: Boolean? = null,
    )

    /**
     * The node panel's one line about the latest agent review:
     * `Approved · round 1 · checks passed`, `Approved · round 1`,
     * `Changes requested · round 2 · checks failed`,
     * `Changes requested · round 2`.
     * [nodeApproved] = the node's `approvedAt` is set. EXP-1010: an approval
     * with no oracle CLEARS the node, so `advisory` shows only when it did not.
     */
    fun reviewLine(review: ReviewLine, nodeApproved: Boolean): String {
        val verdict = if (review.verdict == DomainContract.wfReviewVerdictApprove) {
            "Approved"
        } else {
            "Changes requested"
        }
        val parts = ArrayList<String>(3)
        parts.add(verdict)
        parts.add("round ${review.round}")
        when {
            review.oraclePassed != null ->
                parts.add(if (review.oraclePassed) "checks passed" else "checks failed")
            review.verdict == DomainContract.wfReviewVerdictApprove && !nodeApproved ->
                parts.add("advisory")
        }
        return parts.joinToString(" · ")
    }

    data class MetricRow(val label: String, val value: String)

    /**
     * The detail's Metrics section for a STARTED workflow, in this order. A row
     * appears only when it has something to say, except the critical path,
     * which always does. [counters] = the numeric keys of `workflows.metrics`
     * ([workflowMetricCounters]); anything the jsonb does not carry as a
     * number simply counts as zero.
     */
    fun metricRows(counters: Map<String, Int>): List<MetricRow> {
        fun count(key: String): Int = counters[key] ?: 0
        val rows = ArrayList<MetricRow>()
        rows.add(
            MetricRow(
                label = "Critical path",
                value = "${count("depth")} waves for ${count("nodes")} nodes",
            ),
        )
        val landed = count("landed")
        if (landed > 0) rows.add(MetricRow("Landed", "$landed"))
        val mergeIns = count("mergeIns")
        val changes = count("contractChanges")
        if (mergeIns > 0) {
            rows.add(
                if (changes > 0) {
                    MetricRow(
                        label = "Merge-ins per contract change",
                        value = ratio(mergeIns, changes),
                    )
                } else {
                    MetricRow("Merge-ins", "$mergeIns")
                },
            )
        }
        val escalations = count("escalations")
        if (escalations > 0) {
            rows.add(
                MetricRow(
                    label = "Escalations",
                    value = "$escalations (${count("duplicateEscalations")} duplicate)",
                ),
            )
        }
        val minutes = count("operatorMinutes")
        if (minutes > 0) rows.add(MetricRow("Operator minutes", "$minutes"))
        val rounds = count("reviewRounds")
        if (rounds > 0) rows.add(MetricRow("Review rounds", "$rounds"))
        val byOracle = count("defectsByOracle")
        val byAgent = count("defectsByAgentReview")
        if (byOracle + byAgent > 0) {
            rows.add(
                MetricRow(
                    label = "Defects found",
                    value = "$byOracle by checks · $byAgent by agent review",
                ),
            )
        }
        return rows
    }

    /** One decimal, dot-separated in every locale (web's `toFixed(1)`). */
    private fun ratio(numerator: Int, denominator: Int): String =
        String.format(java.util.Locale.US, "%.1f", numerator.toDouble() / denominator)
}
