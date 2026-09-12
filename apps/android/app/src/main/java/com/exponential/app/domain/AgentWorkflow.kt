package com.exponential.app.domain

// EXP-850: the two latest-wins slots the engine added beside `config_state`,
// `usage`, `rate_limit` and `turn` — the machine's background tasks, and one
// card per workflow the agent is running — plus the working caption every
// client derives from the turn slot. Compose-free and unit tested on their own;
// mirrored ×4 (web `lib/agent-feed.ts`, iOS `AgentFeed`, desktop
// `steer::frames` + `steer::workflow`).

/** EXP-850 (S2): one thing the CLI is running in the background — a shell
 *  command, a nested workflow, an agent. Latest-wins as a WHOLE list: the
 *  engine republishes everything still running, and an empty list closes the
 *  strip. */
data class BackgroundTask(
    val id: String,
    /** A contract `backgroundTaskKind` value; anything else reads as
     *  [BACKGROUND_TASK_KIND_OTHER] so an older client never chokes. */
    val kind: String,
    val description: String,
    /** The launching tool_use_id, once a `task_started` names one. */
    val toolId: String? = null,
)

const val BACKGROUND_TASK_KIND_OTHER = "other"

/** EXP-850 (S3): one phase of a workflow — latest-wins per [index]. */
data class WorkflowPhase(val index: Int, val title: String)

/** EXP-850 (S3): one agent of a workflow — latest-wins per [index]. Every
 *  telemetry field is optional: the engine fills what the CLI reported. */
data class WorkflowAgent(
    val index: Int,
    val label: String,
    val phaseIndex: Int? = null,
    /** The subagent id its nested events are tagged with, once it starts. */
    val agentId: String? = null,
    val model: String? = null,
    /** A contract `workflowAgentState` value. */
    val state: String,
    val tokens: Long? = null,
    val toolCalls: Int? = null,
    val durationMs: Long? = null,
    val lastTool: String? = null,
    val lastToolSummary: String? = null,
    val resultPreview: String? = null,
    val error: String? = null,
) {
    /** Done or errored — the states [WorkflowCaption] counts as finished. */
    val isFinished: Boolean
        get() = state == WORKFLOW_AGENT_STATE_DONE || state == WORKFLOW_AGENT_STATE_ERROR
}

/** EXP-850 (S3): one workflow card. Latest-wins PER ID — the engine
 *  republishes the WHOLE state on every change, so a newer frame simply
 *  replaces the older one. */
data class WorkflowState(
    val id: String,
    val name: String,
    val description: String? = null,
    /** A contract `workflowStatus` value. */
    val status: String,
    val phases: List<WorkflowPhase> = emptyList(),
    val agents: List<WorkflowAgent> = emptyList(),
    val summary: String? = null,
) {
    val isRunning: Boolean get() = status == WORKFLOW_STATUS_RUNNING
}

const val WORKFLOW_STATUS_RUNNING = "running"
const val WORKFLOW_STATUS_COMPLETED = "completed"
const val WORKFLOW_STATUS_FAILED = "failed"
const val WORKFLOW_STATUS_STOPPED = "stopped"

const val WORKFLOW_AGENT_STATE_QUEUED = "queued"
const val WORKFLOW_AGENT_STATE_RUNNING = "running"
const val WORKFLOW_AGENT_STATE_DONE = "done"
const val WORKFLOW_AGENT_STATE_ERROR = "error"

/** How many workflow cards one session keeps — the journal's and the relay's
 *  cap (`JOURNAL_WORKFLOW_CAP` / `WORKFLOW_SLOT_CAP`), so a client never holds
 *  more than a replay could ever hand it. Oldest id evicted first. */
const val WORKFLOW_SLOT_CAP = 16

/** EXP-850 (S3): one phase of the card's phase strip — its title and how its
 *  agents are doing. The counts are what the strip renders; a phase with no
 *  agents still renders (the workflow declared it). */
data class WorkflowPhaseCounts(
    val index: Int,
    val title: String,
    val queued: Int = 0,
    val running: Int = 0,
    val done: Int = 0,
    val error: Int = 0,
) {
    val total: Int get() = queued + running + done + error
}

/**
 * The phase strip: every declared phase in index order with its agents counted
 * by state, plus — last — a synthetic "no phase" entry when agents name a phase
 * this workflow never declared (or name none at all), so no agent is ever
 * silently missing from the strip.
 */
fun workflowPhaseCounts(workflow: WorkflowState): List<WorkflowPhaseCounts> {
    val declared = workflow.phases.sortedBy { it.index }
    val known = declared.map { it.index }.toSet()
    fun counts(index: Int?, title: String): WorkflowPhaseCounts {
        val agents = workflow.agents.filter {
            if (index == null) it.phaseIndex == null || it.phaseIndex !in known else it.phaseIndex == index
        }
        return WorkflowPhaseCounts(
            index = index ?: -1,
            title = title,
            queued = agents.count { it.state == WORKFLOW_AGENT_STATE_QUEUED },
            running = agents.count { it.state == WORKFLOW_AGENT_STATE_RUNNING },
            done = agents.count { it.state == WORKFLOW_AGENT_STATE_DONE },
            error = agents.count { it.state == WORKFLOW_AGENT_STATE_ERROR },
        )
    }
    val rows = declared.map { counts(it.index, it.title) }
    val loose = counts(null, WORKFLOW_UNPHASED_TITLE)
    return if (loose.total > 0) rows + loose else rows
}

/** What the strip calls the agents that belong to no declared phase. */
const val WORKFLOW_UNPHASED_TITLE = "Other"

/** EXP-850 (S3): one phase's counts as the strip words them — done, running,
 *  queued, failed, in that order, omitting the zeroes. A declared phase with
 *  no agents yet says so rather than rendering an empty column. */
fun workflowPhaseSummary(counts: WorkflowPhaseCounts): String {
    val parts = listOfNotNull(
        counts.done.takeIf { it > 0 }?.let { "$it done" },
        counts.running.takeIf { it > 0 }?.let { "$it running" },
        counts.queued.takeIf { it > 0 }?.let { "$it queued" },
        counts.error.takeIf { it > 0 }?.let { "$it failed" },
    )
    return if (parts.isEmpty()) WORKFLOW_PHASE_EMPTY else parts.joinToString(WorkflowCaption.SEPARATOR)
}

/** A declared phase nothing has been queued into yet. */
const val WORKFLOW_PHASE_EMPTY = "no agents"

// ── EXP-850 (S5): the working caption ────────────────────────────────────────

/** The caption with nothing to say yet — no turn start, so no verb and no
 *  clock. Byte-identical ×4. */
const val WORKING_FALLBACK_CAPTION = "Working…"

/** The verb of the turn that started at [startedAt] — a stable pick out of the
 *  contract's 24, so the same turn reads the same on every client and never
 *  flickers between frames. Null without a start. */
fun workingVerb(startedAt: Long?): String? {
    if (startedAt == null) return null
    val verbs = DomainContract.steerWorkingVerbs
    if (verbs.isEmpty()) return null
    // `startedAt` is an epoch in ms, so this is a plain modulo; the abs keeps
    // a nonsense negative stamp on a real index (web parity).
    return verbs[Math.floorMod(Math.abs(startedAt), verbs.size.toLong()).toInt()]
}

/**
 * How long the turn has been going: `37s`, `2m 04s`, `1h 03m`. Absent input
 * renders nothing; a clock BEHIND the turn's start (skew) reads as `0s`.
 */
fun formatWorkingDuration(startedAt: Long?, nowMs: Long): String? {
    if (startedAt == null || startedAt <= 0L) return null
    return formatDurationMs(nowMs - startedAt)
}

/** The same clock over a plain elapsed span (a workflow agent's `durationMs`):
 *  `37s`, `2m 04s`, `1h 03m`. Sub-second and negative spans read as `0s`
 *  rather than disappearing (web `formatTurnDuration` parity — clock skew is
 *  not a reason for the caption to lose half its words). */
fun formatDurationMs(ms: Long?): String? {
    if (ms == null) return null
    val seconds = maxOf(0L, ms / 1000L)
    return when {
        seconds < 60L -> "${seconds}s"
        seconds < 3600L -> "${seconds / 60L}m ${(seconds % 60L).toString().padStart(2, '0')}s"
        else -> "${seconds / 3600L}h ${((seconds % 3600L) / 60L).toString().padStart(2, '0')}m"
    }
}

/** EXP-850 (S3): what one workflow agent row says about itself under its
 *  label — its model, its tokens, its tool calls and how long it took, in that
 *  order, omitting whatever the engine did not report. Null when it reported
 *  nothing at all. */
fun workflowAgentMetrics(agent: WorkflowAgent): String? {
    val parts = listOfNotNull(
        agent.model?.takeIf { it.isNotBlank() },
        formatWorkingTokens(agent.tokens)?.let { "$it tokens" },
        agent.toolCalls?.takeIf { it > 0 }
            ?.let { "$it tool call${if (it == 1) "" else "s"}" },
        formatDurationMs(agent.durationMs?.takeIf { it > 0L }),
    )
    return parts.takeIf { it.isNotEmpty() }?.joinToString(WorkflowCaption.SEPARATOR)
}

/** EXP-850 (S3): the one line an agent row shows about what it is doing (or
 *  did) — the failure, the result, or the tool it is on. */
fun workflowAgentNote(agent: WorkflowAgent): String? = when (agent.state) {
    WORKFLOW_AGENT_STATE_ERROR -> agent.error ?: agent.resultPreview
    WORKFLOW_AGENT_STATE_DONE -> agent.resultPreview
    WORKFLOW_AGENT_STATE_RUNNING -> agent.lastToolSummary ?: agent.lastTool
    else -> null
}?.takeIf { it.isNotBlank() }

/**
 * The turn's output tokens: `812`, `2.0k`, `1.2M`. TRUNCATED at one decimal
 * rather than rounded, so a count never reads as the next unit before it has
 * reached it (999_999 is `999.9k`, not `1000.0k`).
 */
fun formatWorkingTokens(tokens: Long?): String? {
    if (tokens == null || tokens < 0L) return null
    return when {
        tokens < 1_000L -> tokens.toString()
        tokens < 1_000_000L -> "${tenths(tokens, 1_000L)}k"
        else -> "${tenths(tokens, 1_000_000L)}M"
    }
}

/** `value / unit` to one TRUNCATED decimal, e.g. 1234/1000 → `1.2`. */
private fun tenths(value: Long, unit: Long): String {
    val scaled = value * 10L / unit
    return "${scaled / 10L}.${scaled % 10L}"
}

/**
 * EXP-850 (S5): the ONE caption the working row renders —
 * `{verb}… ({duration} · ↓ {tokens} tokens)`.
 *
 * [workflowCaption] (a running workflow's [WorkflowCaption]) REPLACES the verb
 * when there is one: while a workflow runs, what the agent is doing is the
 * workflow, not a mood word. The bracketed group is omitted while both the
 * duration and the token count are unknown, and each half is dropped on its
 * own — a codex run publishes neither and degrades to plain "Working…".
 */
fun workingCaption(
    startedAt: Long?,
    tokens: Long?,
    nowMs: Long,
    workflowCaption: String? = null,
): String {
    val head = workflowCaption?.takeIf { it.isNotBlank() }
        ?: workingVerb(startedAt)?.let { "$it…" }
        ?: WORKING_FALLBACK_CAPTION
    val parts = listOfNotNull(
        formatWorkingDuration(startedAt, nowMs),
        formatWorkingTokens(tokens)?.let { "↓ $it tokens" },
    )
    if (parts.isEmpty()) return head
    return "$head (${parts.joinToString(WorkflowCaption.SEPARATOR)})"
}

/** EXP-850 (S1): what an unsettled `wait` tool row says in the strip above the
 *  composer. The row itself stays an ordinary tool row in the transcript. */
fun waitingLabel(detail: String): String = "Waiting on $detail"

/** EXP-850 (S2): what one background task says in that same strip. */
fun backgroundTaskLabel(description: String): String = "↻ $description"
