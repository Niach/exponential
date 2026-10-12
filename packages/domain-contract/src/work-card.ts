// EXP-1240 (EXP-1259 D12): THE Work card — ONE card for a claude workflow, a
// task list and a subagent, replacing WorkflowCard, TaskListBlock and
// SubagentGroupRow on every client. This is the pure MODEL the card draws;
// each client paints it with its own primitives. Byte-locked by
// `fixtures/work-card.json`, replayed by every mirror: desktop
// `steer::work_card`, web `@exp/domain-contract/work-card` (this file), iOS
// `ExpCore/Sources/Domain/WorkCard.swift`, Android `domain/WorkCard.kt`.
//
// The card: a HEADER (kind icon, name, kind word, progress, elapsed, a spinner
// while running), a SEGMENT BAR (one segment per phase or task, never for a
// subagent) and ROWS. Finished leading phases FOLD into one row; the running
// (or failed) phase lists its lanes one line each; a done phase after it is
// one collapsed row, a pending phase a bare title. No report text, no token
// counts, no JSON. A lane row opens that lane's transcript (the client's
// existing subagent/workflow-agent view).
//
// The session-list caption (`workflow-caption.ts`) is a different surface
// and keeps its own fixture.

import fixture from "../fixtures/work-card.json" with { type: "json" }

export const WORK_CARD_SEPARATOR = ` · `
export const WORK_CARD_COPY = fixture.copy
export const WORK_CARD_ICONS = fixture.icons
export const WORK_CARD_GEOMETRY = fixture.geometry

export interface WorkCardWorkflowAgent {
  index: number
  label?: string | null
  phaseIndex?: number | null
  model?: string | null
  /** contract `workflowAgentState`: queued | running | done | error */
  state: string
  durationMs?: number | null
}

export interface WorkCardTask {
  content: string
  /** pending | in_progress | completed */
  status: string
}

export type WorkCardInput =
  | {
      kind: `workflow`
      name: string
      /** contract `workflowStatus` */
      status: string
      phases?: readonly { index: number; title: string }[] | null
      agents?: readonly WorkCardWorkflowAgent[] | null
      elapsedMs?: number | null
    }
  | { kind: `tasks`; tasks: readonly WorkCardTask[] }
  | {
      kind: `subagent`
      title?: string | null
      /** running | done | error */
      state: string
      toolCalls?: number | null
      elapsedMs?: number | null
    }

export type WorkState = `done` | `running` | `pending` | `failed`

export interface WorkCardSegment {
  state: WorkState
  /** 0..1, how much of the segment is filled. */
  fill: number
}

export interface WorkCardLane {
  label: string
  model: string | null
  /** queued | running | done | error */
  state: string
  duration: string | null
}

export type WorkCardRow =
  | { type: `folded`; label: string; lanes: string; duration: string | null }
  | {
      type: `phase`
      title: string
      state: WorkState
      detail: string | null
      duration: string | null
      lanes: WorkCardLane[]
    }
  | { type: `task`; label: string; state: WorkState }
  | { type: `status`; label: string }

export interface WorkCard {
  kind: `workflow` | `tasks` | `subagent`
  /** A concept icon (packages/icons/icons.json). */
  icon: string
  name: string
  kindWord: string | null
  progress: string | null
  elapsed: string | null
  running: boolean
  segments: WorkCardSegment[]
  rows: WorkCardRow[]
}

/** Coarse: `42s`, `48m`, `1h 12m`, `2h`. Header + folded rows. */
export function workDurationShort(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000))
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`
}

/** Fine: `42s`, `8m 29s`, `1h 12m`. Lane rows. */
export function workDurationLong(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000))
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`
}

/** `claude-opus-5-5` → `opus`; anything else as given; blank → null. */
export function workModelLabel(model: string | null | undefined): string | null {
  const trimmed = (model ?? ``).trim()
  if (!trimmed) return null
  const match = /^claude-([a-z]+)/.exec(trimmed)
  return match ? match[1] : trimmed
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`
}

const SETTLED = new Set([`done`, `error`])

interface Phase {
  title: string
  agents: WorkCardWorkflowAgent[]
}

function phaseState(phase: Phase): WorkState {
  if (phase.agents.length === 0) return `pending`
  if (phase.agents.every((agent) => SETTLED.has(agent.state))) {
    return phase.agents.some((agent) => agent.state === `error`) ? `failed` : `done`
  }
  return `running`
}

function phaseSpanMs(phase: Phase): number | null {
  let span: number | null = null
  for (const agent of phase.agents) {
    if (typeof agent.durationMs === `number`) {
      span = Math.max(span ?? 0, agent.durationMs)
    }
  }
  return span
}

function phaseDetail(phase: Phase): string {
  const count = (state: string) => phase.agents.filter((agent) => agent.state === state).length
  const parts: string[] = []
  const done = count(`done`)
  const failed = count(`error`)
  const running = count(`running`)
  const queued = count(`queued`)
  if (done > 0) parts.push(`${done} done`)
  if (failed > 0) parts.push(`${failed} failed`)
  if (running > 0) parts.push(`${running} running`)
  if (queued > 0) parts.push(`${queued} queued`)
  return parts.join(WORK_CARD_SEPARATOR)
}

function lane(agent: WorkCardWorkflowAgent): WorkCardLane {
  const label = (agent.label ?? ``).trim()
  return {
    label: label || `${fixture.copy.agent} ${agent.index}`,
    model: workModelLabel(agent.model),
    state: agent.state,
    duration:
      SETTLED.has(agent.state) && typeof agent.durationMs === `number`
        ? workDurationLong(agent.durationMs)
        : null,
  }
}

function workflowCard(input: Extract<WorkCardInput, { kind: `workflow` }>): WorkCard {
  const agents = [...(input.agents ?? [])].sort((a, b) => a.index - b.index)
  const declared = [...(input.phases ?? [])].sort((a, b) => a.index - b.index)
  const phases: Phase[] = declared.map((phase) => ({
    title: phase.title.trim() || `${fixture.copy.phase} ${phase.index}`,
    agents: agents.filter((agent) => agent.phaseIndex === phase.index),
  }))
  const known = new Set(declared.map((phase) => phase.index))
  const loose = agents.filter(
    (agent) => agent.phaseIndex === undefined || agent.phaseIndex === null || !known.has(agent.phaseIndex)
  )
  if (loose.length > 0) phases.push({ title: fixture.copy.loosePhase, agents: loose })

  const states = phases.map(phaseState)
  const segments = phases.map((phase, i): WorkCardSegment => {
    const settled = phase.agents.filter((agent) => SETTLED.has(agent.state)).length
    return {
      state: states[i],
      fill: phase.agents.length === 0 ? 0 : settled / phase.agents.length,
    }
  })

  const rows: WorkCardRow[] = []
  let cursor = 0
  while (cursor < phases.length && states[cursor] === `done`) cursor++
  if (cursor > 0) {
    const folded = phases.slice(0, cursor)
    const spans = folded.map(phaseSpanMs).filter((span): span is number => span !== null)
    rows.push({
      type: `folded`,
      label: folded.map((phase) => phase.title).join(WORK_CARD_SEPARATOR),
      lanes: plural(
        folded.reduce((sum, phase) => sum + phase.agents.length, 0),
        fixture.copy.lane,
        fixture.copy.lanes
      ),
      duration: spans.length > 0 ? workDurationShort(spans.reduce((a, b) => a + b, 0)) : null,
    })
  }
  for (let i = cursor; i < phases.length; i++) {
    const phase = phases[i]
    const state = states[i]
    const span = phaseSpanMs(phase)
    if (state === `pending`) {
      rows.push({ type: `phase`, title: phase.title, state, detail: null, duration: null, lanes: [] })
    } else if (state === `done`) {
      rows.push({
        type: `phase`,
        title: phase.title,
        state,
        detail: plural(phase.agents.length, fixture.copy.lane, fixture.copy.lanes),
        duration: span === null ? null : workDurationShort(span),
        lanes: [],
      })
    } else {
      rows.push({
        type: `phase`,
        title: phase.title,
        state,
        detail: phaseDetail(phase),
        duration: state === `failed` && span !== null ? workDurationShort(span) : null,
        lanes: phase.agents.map(lane),
      })
    }
  }

  const settled = agents.filter((agent) => SETTLED.has(agent.state)).length
  return {
    kind: `workflow`,
    icon: fixture.icons.workflow,
    name: input.name.trim(),
    kindWord: fixture.copy.workflow,
    progress: agents.length === 0 ? null : `${settled} / ${agents.length}`,
    elapsed: typeof input.elapsedMs === `number` ? workDurationShort(input.elapsedMs) : null,
    running: input.status === `running`,
    segments,
    rows,
  }
}

function tasksCard(input: Extract<WorkCardInput, { kind: `tasks` }>): WorkCard {
  const tasks = input.tasks
  const stateOf = (task: WorkCardTask): WorkState =>
    task.status === `completed` ? `done` : task.status === `in_progress` ? `running` : `pending`
  const active = tasks.filter((task) => task.status === `in_progress`)
  const shown = active.length > 0 ? active : tasks.filter((task) => stateOf(task) === `pending`).slice(0, 1)
  const completed = tasks.filter((task) => task.status === `completed`).length
  return {
    kind: `tasks`,
    icon: fixture.icons.tasks,
    name: fixture.copy.tasks,
    kindWord: null,
    progress: tasks.length === 0 ? null : `${completed} / ${tasks.length}`,
    elapsed: null,
    running: active.length > 0,
    segments: tasks.map((task) => {
      const state = stateOf(task)
      return { state, fill: state === `done` ? 1 : state === `running` ? 0.5 : 0 }
    }),
    rows: shown.map((task) => ({ type: `task` as const, label: task.content.trim(), state: stateOf(task) })),
  }
}

function subagentCard(input: Extract<WorkCardInput, { kind: `subagent` }>): WorkCard {
  const calls = input.toolCalls ?? 0
  const word =
    input.state === `running`
      ? fixture.copy.running
      : input.state === `error`
        ? fixture.copy.failed
        : fixture.copy.done
  return {
    kind: `subagent`,
    icon: fixture.icons.subagent,
    name: (input.title ?? ``).trim() || fixture.copy.subagent,
    kindWord: fixture.copy.subagent,
    progress: plural(calls, fixture.copy.call, fixture.copy.calls),
    elapsed: typeof input.elapsedMs === `number` ? workDurationShort(input.elapsedMs) : null,
    running: input.state === `running`,
    segments: [],
    rows: [{ type: `status`, label: `${fixture.copy.subagent}${WORK_CARD_SEPARATOR}${word}` }],
  }
}

export function workCard(input: WorkCardInput): WorkCard {
  if (input.kind === `workflow`) return workflowCard(input)
  if (input.kind === `tasks`) return tasksCard(input)
  return subagentCard(input)
}
