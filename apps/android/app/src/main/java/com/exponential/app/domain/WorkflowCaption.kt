package com.exponential.app.domain

/**
 * EXP-850 (S7): the ONE sentence a running workflow reads as — the working
 * caption's text while a workflow runs, and what the device writes into
 * `coding_sessions.agent_caption` for every session list.
 *
 * Hand-mirrored ×4 (TS `packages/domain-contract/src/workflow-caption.ts`,
 * Rust `steer::workflow_caption`, iOS `WorkflowCaption.swift`) and byte-locked
 * by `packages/domain-contract/fixtures/workflow-caption.json`, which every
 * client's test replays.
 *
 * Rules:
 * - running with no agents yet → `Workflow {name} · starting`
 * - running → `Workflow {name} · {done}/{total} agents done · {phase}`, where
 *   done counts `done` AND `error` agents, total is every agent, and the phase
 *   is the title of the RUNNING agent with the highest index (else of the
 *   highest-index agent). The ` · {phase}` segment is omitted when that agent
 *   names no phase, or names one no phase row matches.
 * - completed → `Workflow {name} · done · {total} agents` (singular at 1)
 * - failed → `Workflow {name} · failed`; stopped → `Workflow {name} · stopped`
 */
object WorkflowCaption {
    /** The segment separator: space, MIDDLE DOT (U+00B7), space. */
    const val SEPARATOR: String = " · "

    /** The caption's leading word — every segment hangs off it. */
    const val PREFIX: String = "Workflow"

    fun of(workflow: WorkflowState): String {
        val head = "$PREFIX ${workflow.name}"
        return when (workflow.status) {
            WORKFLOW_STATUS_COMPLETED -> {
                val total = workflow.agents.size
                head + SEPARATOR + "done" + SEPARATOR +
                    "$total agent${if (total == 1) "" else "s"}"
            }
            WORKFLOW_STATUS_FAILED -> head + SEPARATOR + "failed"
            WORKFLOW_STATUS_STOPPED -> head + SEPARATOR + "stopped"
            // Running, and anything a newer engine invents: a workflow we
            // cannot name the end of is still one in flight.
            else -> {
                if (workflow.agents.isEmpty()) return head + SEPARATOR + "starting"
                val done = workflow.agents.count { it.isFinished }
                val progress = "$done/${workflow.agents.size} agents done"
                val phase = workflow.phaseTitle(workflow.leadAgent())
                head + SEPARATOR + progress + (phase?.let { SEPARATOR + it } ?: "")
            }
        }
    }
}

/** [WorkflowCaption.of] as an extension, for call sites that read better. */
fun WorkflowState.caption(): String = WorkflowCaption.of(this)

/** The agent whose phase names the caption: the RUNNING one with the highest
 *  index, else the highest-index agent of any state. Null on an agent-less
 *  workflow. */
fun WorkflowState.leadAgent(): WorkflowAgent? =
    agents.filter { it.state == WORKFLOW_AGENT_STATE_RUNNING }.maxByOrNull { it.index }
        ?: agents.maxByOrNull { it.index }

/** The title of [agent]'s phase, or null when it names none (or names one this
 *  workflow has no row for — a phase list that has not caught up yet). */
fun WorkflowState.phaseTitle(agent: WorkflowAgent?): String? {
    val index = agent?.phaseIndex ?: return null
    return phases.firstOrNull { it.index == index }?.title?.takeIf { it.isNotBlank() }
}
