package com.exponential.app.domain

// EXP-1240 (EXP-1259 D12): THE Work card, ONE card for a claude workflow, a
// task list and a subagent, replacing the workflow card, the task list block
// and the subagent group rows. This is the pure MODEL the card draws; the
// Compose half is `ui/session/WorkCard.kt`. Hand-ported from the TS reference
// `packages/domain-contract/src/work-card.ts` and locked to
// `fixtures/work-card.json` by WorkCardTest (whole objects, `fill` within
// 1e-9). The session-list caption keeps [WorkflowCaption].

object WorkCardSpec {
    const val SEPARATOR = " · "

    // fixture `copy`
    const val WORKFLOW = "Workflow"
    const val TASKS = "Tasks"
    const val SUBAGENT = "Subagent"
    const val PHASE = "Phase"
    const val LOOSE_PHASE = "Agents"
    const val AGENT = "Agent"
    const val LANE = "lane"
    const val LANES = "lanes"
    const val CALL = "call"
    const val CALLS = "calls"
    const val RUNNING = "running"
    const val DONE = "done"
    const val FAILED = "failed"

    // fixture `icons` (concept names, packages/icons/icons.json)
    const val ICON_WORKFLOW = "session-tree"
    const val ICON_TASKS = "ui-checklist"
    const val ICON_SUBAGENT = "coding-subagent"

    // fixture `testIds`
    const val TEST_ID_CARD = "work-card"
    const val TEST_ID_LANE = "work-card-lane"
    const val TEST_ID_FOLDED = "work-card-folded"

    // fixture `geometry` (dp)
    const val HEADER = 40
    const val ROW = 32
    const val LANE_ROW = 28
    const val LANE_INDENT = 28
    const val SEGMENT_HEIGHT = 3
    const val SEGMENT_GAP = 4
}

data class WorkCardWorkflowAgent(
    val index: Int,
    val label: String? = null,
    val phaseIndex: Int? = null,
    val model: String? = null,
    /** contract `workflowAgentState`: queued | running | done | error */
    val state: String,
    val durationMs: Long? = null,
)

data class WorkCardTask(
    val content: String,
    /** pending | in_progress | completed */
    val status: String,
)

data class WorkCardPhaseInput(val index: Int, val title: String)

sealed interface WorkCardInput {
    data class Workflow(
        val name: String,
        /** contract `workflowStatus` */
        val status: String,
        val phases: List<WorkCardPhaseInput> = emptyList(),
        val agents: List<WorkCardWorkflowAgent> = emptyList(),
        val elapsedMs: Long? = null,
    ) : WorkCardInput

    data class Tasks(val tasks: List<WorkCardTask>) : WorkCardInput

    data class Subagent(
        val title: String? = null,
        /** running | done | error */
        val state: String,
        val toolCalls: Int? = null,
        val elapsedMs: Long? = null,
    ) : WorkCardInput
}

enum class WorkState(val wire: String) {
    Done("done"),
    Running("running"),
    Pending("pending"),
    Failed("failed"),
    ;

    companion object {
        fun fromWire(value: String): WorkState = entries.first { it.wire == value }
    }
}

data class WorkCardSegment(
    val state: WorkState,
    /** 0..1, how much of the segment is filled. */
    val fill: Double,
)

data class WorkCardLane(
    val label: String,
    val model: String?,
    /** queued | running | done | error */
    val state: String,
    val duration: String?,
)

sealed interface WorkCardRow {
    data class Folded(val label: String, val lanes: String, val duration: String?) : WorkCardRow

    data class Phase(
        val title: String,
        val state: WorkState,
        val detail: String?,
        val duration: String?,
        val lanes: List<WorkCardLane>,
    ) : WorkCardRow

    data class Task(val label: String, val state: WorkState) : WorkCardRow

    data class Status(val label: String) : WorkCardRow
}

enum class WorkCardKind(val wire: String) { Workflow("workflow"), Tasks("tasks"), Subagent("subagent") }

data class WorkCard(
    val kind: WorkCardKind,
    /** A concept icon (packages/icons/icons.json). */
    val icon: String,
    val name: String,
    val kindWord: String?,
    val progress: String?,
    val elapsed: String?,
    val running: Boolean,
    val segments: List<WorkCardSegment>,
    val rows: List<WorkCardRow>,
)

object WorkCards {

    /** Coarse: `42s`, `48m`, `1h 12m`, `2h`. Header + folded rows. */
    fun durationShort(ms: Long): String {
        val seconds = maxOf(0L, ms / 1000)
        if (seconds < 60) return "${seconds}s"
        val minutes = seconds / 60
        if (minutes < 60) return "${minutes}m"
        val hours = minutes / 60
        val rest = minutes % 60
        return if (rest == 0L) "${hours}h" else "${hours}h ${rest}m"
    }

    /** Fine: `42s`, `8m 29s`, `1h 12m`. Lane rows. */
    fun durationLong(ms: Long): String {
        val seconds = maxOf(0L, ms / 1000)
        if (seconds < 60) return "${seconds}s"
        val minutes = seconds / 60
        if (minutes < 60) return "${minutes}m ${seconds % 60}s"
        return "${minutes / 60}h ${minutes % 60}m"
    }

    private val CLAUDE_MODEL = Regex("^claude-([a-z]+)")

    /** `claude-opus-5-5` → `opus`; anything else as given; blank → null. */
    fun modelLabel(model: String?): String? {
        val trimmed = model?.trim().orEmpty()
        if (trimmed.isEmpty()) return null
        return CLAUDE_MODEL.find(trimmed)?.groupValues?.get(1) ?: trimmed
    }

    private fun plural(count: Int, one: String, many: String) = "$count ${if (count == 1) one else many}"

    private val SETTLED = setOf("done", "error")

    /** One phase as the card groups it: a declared phase's agents, or the
     *  trailing loose `Agents` phase. Position N here is segment N. */
    data class PhaseGroup<T>(val index: Int?, val title: String, val agents: List<T>)

    /** The workflow's phases in card order (the declared phases by index, then
     *  the agents outside every declared phase as a trailing `Agents` phase).
     *  Generic over the agent type so the client's own wire agents group by
     *  the SAME rule the model does (web `workflowPhaseGroups`). */
    fun <T> phaseGroups(
        phases: List<WorkCardPhaseInput>,
        agents: List<T>,
        index: (T) -> Int,
        phaseIndex: (T) -> Int?,
    ): List<PhaseGroup<T>> {
        val sorted = agents.sortedBy(index)
        val declared = phases.sortedBy { it.index }
        val groups = declared.map { phase ->
            PhaseGroup(
                index = phase.index,
                title = phase.title.trim().ifEmpty { "${WorkCardSpec.PHASE} ${phase.index}" },
                agents = sorted.filter { phaseIndex(it) == phase.index },
            )
        }.toMutableList()
        val known = declared.mapTo(mutableSetOf()) { it.index }
        val loose = sorted.filter { phaseIndex(it).let { at -> at == null || at !in known } }
        if (loose.isNotEmpty()) groups += PhaseGroup(null, WorkCardSpec.LOOSE_PHASE, loose)
        return groups
    }

    private fun phaseState(phase: PhaseGroup<WorkCardWorkflowAgent>): WorkState {
        if (phase.agents.isEmpty()) return WorkState.Pending
        if (phase.agents.all { it.state in SETTLED }) {
            return if (phase.agents.any { it.state == "error" }) WorkState.Failed else WorkState.Done
        }
        return WorkState.Running
    }

    /** The longest settled lane of a phase: its wall time. */
    fun phaseSpanMs(agents: List<WorkCardWorkflowAgent>): Long? {
        var span: Long? = null
        for (agent in agents) {
            val ms = agent.durationMs ?: continue
            span = maxOf(span ?: 0L, ms)
        }
        return span
    }

    private fun phaseDetail(phase: PhaseGroup<WorkCardWorkflowAgent>): String {
        fun count(state: String) = phase.agents.count { it.state == state }
        val parts = mutableListOf<String>()
        val done = count("done")
        val failed = count("error")
        val running = count("running")
        val queued = count("queued")
        if (done > 0) parts += "$done done"
        if (failed > 0) parts += "$failed failed"
        if (running > 0) parts += "$running running"
        if (queued > 0) parts += "$queued queued"
        return parts.joinToString(WorkCardSpec.SEPARATOR)
    }

    fun lane(agent: WorkCardWorkflowAgent): WorkCardLane {
        val label = agent.label?.trim().orEmpty()
        return WorkCardLane(
            label = label.ifEmpty { "${WorkCardSpec.AGENT} ${agent.index}" },
            model = modelLabel(agent.model),
            state = agent.state,
            duration = agent.durationMs?.takeIf { agent.state in SETTLED }?.let(::durationLong),
        )
    }

    private fun workflowCard(input: WorkCardInput.Workflow): WorkCard {
        val agents = input.agents.sortedBy { it.index }
        val phases = phaseGroups(input.phases, input.agents, { it.index }, { it.phaseIndex })
        val states = phases.map(::phaseState)
        val segments = phases.mapIndexed { i, phase ->
            val settled = phase.agents.count { it.state in SETTLED }
            WorkCardSegment(
                state = states[i],
                fill = if (phase.agents.isEmpty()) 0.0 else settled.toDouble() / phase.agents.size,
            )
        }

        val rows = mutableListOf<WorkCardRow>()
        var cursor = 0
        while (cursor < phases.size && states[cursor] == WorkState.Done) cursor++
        if (cursor > 0) {
            val folded = phases.subList(0, cursor)
            val spans = folded.mapNotNull { phaseSpanMs(it.agents) }
            rows += WorkCardRow.Folded(
                label = folded.joinToString(WorkCardSpec.SEPARATOR) { it.title },
                lanes = plural(folded.sumOf { it.agents.size }, WorkCardSpec.LANE, WorkCardSpec.LANES),
                duration = if (spans.isEmpty()) null else durationShort(spans.sum()),
            )
        }
        for (i in cursor until phases.size) {
            val phase = phases[i]
            val state = states[i]
            val span = phaseSpanMs(phase.agents)
            rows += when (state) {
                WorkState.Pending -> WorkCardRow.Phase(phase.title, state, null, null, emptyList())
                WorkState.Done -> WorkCardRow.Phase(
                    title = phase.title,
                    state = state,
                    detail = plural(phase.agents.size, WorkCardSpec.LANE, WorkCardSpec.LANES),
                    duration = span?.let(::durationShort),
                    lanes = emptyList(),
                )
                else -> WorkCardRow.Phase(
                    title = phase.title,
                    state = state,
                    detail = phaseDetail(phase),
                    duration = if (state == WorkState.Failed && span != null) durationShort(span) else null,
                    lanes = phase.agents.map(::lane),
                )
            }
        }

        val settled = agents.count { it.state in SETTLED }
        return WorkCard(
            kind = WorkCardKind.Workflow,
            icon = WorkCardSpec.ICON_WORKFLOW,
            name = input.name.trim(),
            kindWord = WorkCardSpec.WORKFLOW,
            progress = if (agents.isEmpty()) null else "$settled / ${agents.size}",
            elapsed = input.elapsedMs?.let(::durationShort),
            running = input.status == "running",
            segments = segments,
            rows = rows,
        )
    }

    private fun taskState(task: WorkCardTask): WorkState = when (task.status) {
        "completed" -> WorkState.Done
        "in_progress" -> WorkState.Running
        else -> WorkState.Pending
    }

    private fun tasksCard(input: WorkCardInput.Tasks): WorkCard {
        val tasks = input.tasks
        val active = tasks.filter { it.status == "in_progress" }
        val shown = active.ifEmpty { tasks.filter { taskState(it) == WorkState.Pending }.take(1) }
        val completed = tasks.count { it.status == "completed" }
        return WorkCard(
            kind = WorkCardKind.Tasks,
            icon = WorkCardSpec.ICON_TASKS,
            name = WorkCardSpec.TASKS,
            kindWord = null,
            progress = if (tasks.isEmpty()) null else "$completed / ${tasks.size}",
            elapsed = null,
            running = active.isNotEmpty(),
            segments = tasks.map { task ->
                val state = taskState(task)
                WorkCardSegment(
                    state = state,
                    fill = when (state) {
                        WorkState.Done -> 1.0
                        WorkState.Running -> 0.5
                        else -> 0.0
                    },
                )
            },
            rows = shown.map { WorkCardRow.Task(it.content.trim(), taskState(it)) },
        )
    }

    private fun subagentCard(input: WorkCardInput.Subagent): WorkCard {
        val calls = input.toolCalls ?: 0
        val word = when (input.state) {
            "running" -> WorkCardSpec.RUNNING
            "error" -> WorkCardSpec.FAILED
            else -> WorkCardSpec.DONE
        }
        return WorkCard(
            kind = WorkCardKind.Subagent,
            icon = WorkCardSpec.ICON_SUBAGENT,
            name = input.title?.trim().orEmpty().ifEmpty { WorkCardSpec.SUBAGENT },
            kindWord = WorkCardSpec.SUBAGENT,
            progress = plural(calls, WorkCardSpec.CALL, WorkCardSpec.CALLS),
            elapsed = input.elapsedMs?.let(::durationShort),
            running = input.state == "running",
            segments = emptyList(),
            rows = listOf(WorkCardRow.Status("${WorkCardSpec.SUBAGENT}${WorkCardSpec.SEPARATOR}$word")),
        )
    }

    fun of(input: WorkCardInput): WorkCard = when (input) {
        is WorkCardInput.Workflow -> workflowCard(input)
        is WorkCardInput.Tasks -> tasksCard(input)
        is WorkCardInput.Subagent -> subagentCard(input)
    }
}

// ── The Android feed's inputs (web `lib/agent-feed.ts` workflowWorkCard & co.)

/** One folded phase spelled out for the folded row's in-place expansion (the
 *  model folds them into one row and drops their lanes). */
data class WorkCardFoldedPhase(val title: String, val duration: String?, val lanes: List<WorkCardLane>)

/** Which lane a row names: [phase] = the phase POSITION in card order (the
 *  segments' order), [lane] = the lane's position inside that phase. */
data class WorkCardLaneTarget(val phase: Int, val lane: Int)

private fun WorkflowAgent.cardAgent() = WorkCardWorkflowAgent(
    index = index,
    label = label,
    phaseIndex = phaseIndex,
    model = model,
    state = state,
    durationMs = durationMs,
)

private fun WorkflowState.groups(): List<WorkCards.PhaseGroup<WorkflowAgent>> = WorkCards.phaseGroups(
    phases.map { WorkCardPhaseInput(it.index, it.title) },
    agents,
    { it.index },
    { it.phaseIndex },
)

/** The workflow's elapsed time. The wire carries no start, so it is the sum of
 *  its phases' spans (they run in sequence); null before any lane settled. */
fun WorkflowState.elapsedMs(): Long? {
    var total: Long? = null
    for (group in groups()) {
        val span = WorkCards.phaseSpanMs(group.agents.map { it.cardAgent() }) ?: continue
        total = (total ?: 0L) + span
    }
    return total
}

/** A workflow's Work card. */
fun WorkflowState.workCard(): WorkCard = WorkCards.of(
    WorkCardInput.Workflow(
        name = name,
        status = status,
        phases = phases.map { WorkCardPhaseInput(it.index, it.title) },
        agents = agents.map { it.cardAgent() },
        elapsedMs = elapsedMs(),
    ),
)

/** The phases a card's leading folded row stands for; empty when nothing folded. */
fun WorkflowState.foldedPhases(card: WorkCard): List<WorkCardFoldedPhase> {
    if (card.rows.firstOrNull() !is WorkCardRow.Folded) return emptyList()
    val count = card.segments.size - (card.rows.size - 1)
    return groups().take(count).map { group ->
        val agents = group.agents.map { it.cardAgent() }
        WorkCardFoldedPhase(
            title = group.title,
            duration = WorkCards.phaseSpanMs(agents)?.let(WorkCards::durationShort),
            lanes = agents.map(WorkCards::lane),
        )
    }
}

/** The agent behind a lane the card named, or null. */
fun WorkflowState.laneAgent(target: WorkCardLaneTarget): WorkflowAgent? =
    groups().getOrNull(target.phase)?.agents?.getOrNull(target.lane)

/** The agent's own task list as a Work card. */
fun taskListWorkCard(entries: List<TaskListEntry>): WorkCard =
    WorkCards.of(WorkCardInput.Tasks(entries.map { WorkCardTask(it.content, it.status) }))

/** A subagent (its group of feed items) as a Work card: its name, the calls it
 *  made, done or running. EXP-748: the count is the publisher's. */
fun AgentFeedRow.SubagentRun.workCard(): WorkCard = WorkCards.of(
    WorkCardInput.Subagent(
        title = label,
        state = if (completed) "done" else "running",
        toolCalls = toolCount,
    ),
)
