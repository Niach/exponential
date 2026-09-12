// EXP-850 §7: the ONE caption a running (or finished) claude workflow renders
// — the session-list second line (`coding_sessions.agent_caption`) and the
// working caption inside the steer view. Hand-mirrored ×4 (desktop
// `steer::workflow_caption`, web `lib/agent-feed.ts`, iOS
// `ExpCore/Sources/Domain/WorkflowCaption.swift`, Android
// `domain/WorkflowCaption.kt`) and byte-locked by
// `fixtures/workflow-caption.json`, which every client's test runs.
//
// Rules (STEER-WIRE-EXP-850.md §7):
// - `running` with no agents yet    → `Workflow {name} · starting`
// - `running`                       → `Workflow {name} · {done}/{total} agents done · {phase}`
//   with done = agents in `done` or `error`, total = agents.length, and phase
//   the title of the RUNNING agent with the highest index (else of the
//   highest-index agent); the ` · {phase}` segment is omitted when that agent
//   names no phase title.
// - `completed`                     → `Workflow {name} · done · {total} agents`
// - `failed`                        → `Workflow {name} · failed`
// - `stopped` (and any unknown)     → `Workflow {name} · stopped`

/** The segment separator: space, MIDDLE DOT (U+00B7), space. */
export const WORKFLOW_CAPTION_SEPARATOR = ` · `

export interface WorkflowCaptionPhase {
  index: number
  title: string
}

export interface WorkflowCaptionAgent {
  index: number
  /** A contract `workflowAgentState` value (`queued`/`running`/`done`/`error`). */
  state: string
  phaseIndex?: number | null
}

export interface WorkflowCaptionInput {
  name: string
  /** A contract `workflowStatus` value. */
  status: string
  phases?: readonly WorkflowCaptionPhase[] | null
  agents?: readonly WorkflowCaptionAgent[] | null
}

function phaseTitle(
  agent: WorkflowCaptionAgent | undefined,
  phases: readonly WorkflowCaptionPhase[]
): string | undefined {
  if (!agent || agent.phaseIndex === undefined || agent.phaseIndex === null) {
    return undefined
  }
  const phase = phases.find((entry) => entry.index === agent.phaseIndex)
  const title = phase?.title?.trim()
  return title ? title : undefined
}

/** The agent whose phase names the caption: the running one with the highest
 *  index, else the highest-index agent of any state. */
function leadAgent(
  agents: readonly WorkflowCaptionAgent[]
): WorkflowCaptionAgent | undefined {
  let running: WorkflowCaptionAgent | undefined
  let latest: WorkflowCaptionAgent | undefined
  for (const agent of agents) {
    if (!latest || agent.index >= latest.index) latest = agent
    if (agent.state === `running` && (!running || agent.index >= running.index)) {
      running = agent
    }
  }
  return running ?? latest
}

export function workflowCaption(workflow: WorkflowCaptionInput): string {
  const name = workflow.name.trim()
  const head = `Workflow ${name}`
  const agents = workflow.agents ?? []
  const phases = workflow.phases ?? []
  const total = agents.length
  if (workflow.status === `completed`) {
    return `${head}${WORKFLOW_CAPTION_SEPARATOR}done${WORKFLOW_CAPTION_SEPARATOR}${total} ${
      total === 1 ? `agent` : `agents`
    }`
  }
  if (workflow.status === `failed`) return `${head}${WORKFLOW_CAPTION_SEPARATOR}failed`
  if (workflow.status !== `running`) {
    // `stopped`, and anything a newer contract adds: a run that is over and
    // did not say it succeeded reads as stopped rather than as still running.
    return `${head}${WORKFLOW_CAPTION_SEPARATOR}stopped`
  }
  if (total === 0) return `${head}${WORKFLOW_CAPTION_SEPARATOR}starting`
  const done = agents.filter(
    (agent) => agent.state === `done` || agent.state === `error`
  ).length
  const caption = `${head}${WORKFLOW_CAPTION_SEPARATOR}${done}/${total} agents done`
  const phase = phaseTitle(leadAgent(agents), phases)
  return phase ? `${caption}${WORKFLOW_CAPTION_SEPARATOR}${phase}` : caption
}
