// #NEW-145 / #NEW-143 D4: MCP tasks (`io.modelcontextprotocol/tasks`) as the
// second kind of watcher on `session_watches` (packages/eagent fixtures/
// rules.json `watcher`): who gets a task, which watch it holds, how the
// watched session's transitions, cards and report and the event firings map
// to task statuses, and tasks/get, tasks/update, tasks/cancel. Pure and
// locked by `fixtures/tasks.json` (its `$comment` is the spec).

import fixture from "../fixtures/tasks.json" with { type: "json" }
import { burstAdmit, type BurstState } from "./deletes"
import type { ElicitRequest, ElicitResult, FormSchema } from "./elicitation"

export const TASKS_EXTENSION: string = fixture.extension
export const TASK_TTL_MS: number = fixture.ttlMs
export const TASK_POLL_INTERVAL_MS: number = fixture.pollIntervalMs
export const TASK_WATCHER_KIND: string = fixture.watcherKind

export type TaskStatus = `working` | `input_required` | `completed` | `cancelled` | `failed`
const TERMINAL: readonly TaskStatus[] = [`completed`, `cancelled`, `failed`]
export const isTerminal = (status: TaskStatus) => TERMINAL.includes(status)

// ── Who gets a task, watching what ────────────────────────────────────

export interface TaskCall {
  tool: string
  args: Record<string, unknown>
  /** The caller runs inside a session of its own (our agents): never a task. */
  inSession: boolean
  /** The request's client capabilities declared the tasks extension. */
  declared: boolean
  /** The caller's live (non-terminal, unexpired) tasks. */
  liveTasks: number
}

export type TaskWatch =
  | { kind: `session`; target: `started` | `sessionId` | `agentId` }
  | { kind: `events`; specs: string[] }
  /** #FEED-295: the transfer the call started. */
  | { kind: `transfer` }

const EVENT_SPEC = new RegExp(fixture.eventSpecPattern)

/** The watch a call's task holds, or null = no task (the ordinary result). */
export function taskWatchFor(call: TaskCall): TaskWatch | null {
  if (call.inSession || !call.declared) return null
  if (call.liveTasks >= fixture.maxLivePerPrincipal) return null
  const a = call.args
  // #FEED-295: a transfer's send watches that transfer; its other ops never make a task.
  if (call.tool === fixture.transferTool.tool) return a.op === fixture.transferTool.op ? { kind: `transfer` } : null
  if (call.tool !== fixture.tool) return null
  if (a.stop === true || a.answer !== undefined) return null
  const has = (key: string) => typeof a[key] === `string` && a[key] !== ``
  if (has(`sessionId`)) return { kind: `session`, target: `sessionId` }
  if (has(`agentId`)) return { kind: `session`, target: `agentId` }
  if (has(`text`) || has(`promptId`) || Array.isArray(a.issueIds)) return { kind: `session`, target: `started` }
  const wakeOn = Array.isArray(a.wakeOn) ? (a.wakeOn as unknown[]) : []
  if (wakeOn.length > 0 && wakeOn.every((spec) => typeof spec === `string` && EVENT_SPEC.test(spec))) return { kind: `events`, specs: wakeOn as string[] }
  return null
}

// ── The task and what drives it ──────────────────────────────────────

/** A posted surface card as the genUI view draws it (packages/mcp-apps genui/result.ts `InlineCard`). */
export interface SurfaceCard {
  kind: `surface`
  surface: { root: Record<string, unknown>; data?: Record<string, unknown> | null }
  title?: string | null
}

export const GENUI_RESOURCE_URI: string = fixture.genuiResourceUri

export type Card =
  | { kind: `ask`; question: string; options: string[]; multi?: boolean; custom?: boolean; sealed?: boolean }
  | { kind: `plan`; text: string; questions: { id: string; prompt: string; options: string[] }[]; sealed?: boolean }

export interface TaskState {
  taskId: string
  principal: string
  watch: `session` | `events` | `transfer`
  /** The watched session (a session watch). */
  sessionId: string | null
  /** #FEED-295: the watched transfer (a transfer watch). */
  transferId?: string | null
  status: TaskStatus
  statusMessage: string | null
  createdAt: number
  lastUpdatedAt: number
  /** Open cards by message id (insertion order). */
  cards: Record<string, Card>
  result: unknown
  error: { code: number; message: string } | null
}

export type TaskEvent =
  | { kind: `session.transition`; at: number; digest: string }
  | { kind: `card.open`; at: number; messageId: string; card: Card }
  | { kind: `card.answered`; at: number; messageId: string }
  /** #NEW-149: `surface` = the last surface card the session posted (its genUI view). */
  | { kind: `session.end`; at: number; summary: string; reason?: string | null; surface?: SurfaceCard | null }
  | { kind: `watch.fire`; at: number; text: string }
  /** #FEED-295: the destination's counters. */
  | { kind: `transfer.progress`; at: number; files: number; filesDone: number; bytes: number; bytesDone: number }
  /** #FEED-295: the transfer ended; `text` = its end line. */
  | { kind: `transfer.end`; at: number; status: `done` | `failed` | `cancelled`; text: string }

const clip = (text: string, max: number) => (text.length > max ? `${text.slice(0, max - 1).trimEnd()}…` : text)

/** The digest's last non-blank line, clipped (`statusMessageMax`). */
export function lastLine(digest: string): string | null {
  const lines = digest.split(`\n`).map((l) => l.trim()).filter(Boolean)
  return lines.length ? clip(lines.at(-1)!, fixture.statusMessageMax) : null
}

/** Decimal units, one decimal under 100 (`5.9 GB`, `164 MB`, `999 B`). */
export function formatBytes(bytes: number): string {
  const units = [`B`, `KB`, `MB`, `GB`, `TB`]
  let value = bytes
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit += 1
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(value >= 100 ? 0 : 1)} ${units[unit]}`
}

/** A transfer's progress line (`copy.transferProgress`). */
export function transferProgressLine(p: { files: number; filesDone: number; bytes: number; bytesDone: number }): string {
  return fixture.copy.transferProgress
    .replace(`{bytesDone}`, formatBytes(p.bytesDone))
    .replace(`{bytes}`, formatBytes(p.bytes))
    .replace(`{filesDone}`, String(p.filesDone))
    .replace(`{files}`, String(p.files))
}

/** Status of a live task: input_required while a card it can render is open. */
function liveStatus(cards: Record<string, Card>): TaskStatus {
  return Object.values(cards).some((c) => !c.sealed) ? `input_required` : `working`
}

/** Apply one bus event to a task (terminal tasks ignore everything). */
export function taskReduce(task: TaskState, event: TaskEvent): TaskState {
  if (isTerminal(task.status)) return task
  const next: TaskState = { ...task, cards: { ...task.cards }, lastUpdatedAt: event.at }
  switch (event.kind) {
    case `session.transition`:
      next.statusMessage = lastLine(event.digest) ?? next.statusMessage
      break
    case `card.open`:
      next.cards[event.messageId] = event.card
      next.statusMessage = event.card.sealed
        ? fixture.copy.sealedCard
        : clip(event.card.kind === `ask` ? event.card.question : fixture.copy.plan, fixture.statusMessageMax)
      break
    case `card.answered`:
      delete next.cards[event.messageId]
      break
    case `session.end`: {
      next.cards = {}
      const reason = event.reason ? (fixture.failures as Record<string, string>)[event.reason] ?? fixture.failures.failed : null
      if (reason) {
        next.status = `failed`
        next.error = { code: fixture.failureCode, message: reason }
        next.statusMessage = reason
      } else {
        const text = event.summary.trim() || fixture.copy.noSummary
        next.status = `completed`
        next.result = event.surface
          ? { content: [{ type: `text`, text }], structuredContent: { shown: `inline`, card: event.surface }, _meta: { ui: { resourceUri: GENUI_RESOURCE_URI } }, isError: false }
          : { content: [{ type: `text`, text }], isError: false }
        next.statusMessage = clip(lastLine(text) ?? text, fixture.statusMessageMax)
      }
      return next
    }
    case `watch.fire`:
      next.cards = {}
      next.status = `completed`
      next.result = { content: [{ type: `text`, text: event.text }], isError: false }
      next.statusMessage = clip(event.text, fixture.statusMessageMax)
      return next
    case `transfer.progress`:
      next.statusMessage = clip(transferProgressLine(event), fixture.statusMessageMax)
      break
    case `transfer.end`:
      next.cards = {}
      next.statusMessage = clip(event.text, fixture.statusMessageMax)
      if (event.status === `done`) {
        next.status = `completed`
        next.result = { content: [{ type: `text`, text: event.text }], isError: false }
      } else if (event.status === `failed`) {
        next.status = `failed`
        next.error = { code: fixture.failureCode, message: event.text }
      } else {
        next.status = `cancelled`
      }
      return next
  }
  next.status = liveStatus(next.cards)
  return next
}

// ── Cards as elicitations and back ────────────────────────────────────

const titled = (labels: readonly string[]) => labels.map((l) => ({ const: l, title: l }))

/** The `elicitation/create` (form) a card asks through; null for a sealed card (answered in the app). */
export function cardRequest(card: Card): ElicitRequest | null {
  if (card.sealed) return null
  const f = fixture.form
  const properties: FormSchema[`properties`] = {}
  let message: string
  const required: string[] = []
  if (card.kind === `ask`) {
    message = card.question
    if (card.options.length > 0) {
      if (card.multi) properties.answers = { type: `array`, title: f.answer, items: { anyOf: titled(card.options) } }
      else properties.answer = { type: `string`, title: f.answer, oneOf: titled(card.options) }
      if (card.custom !== false) properties.text = { type: `string`, title: f.orText }
    } else {
      properties.text = { type: `string`, title: f.answer }
      required.push(`text`)
    }
  } else {
    message = clip(card.text, fixture.planMax)
    properties.decision = { type: `string`, title: f.decision, oneOf: [{ const: `approved`, title: f.approve }, { const: `rejected`, title: f.reject }] }
    required.push(`decision`)
    for (const q of card.questions) properties[`q_${q.id}`] = { type: `string`, title: q.prompt, oneOf: titled(q.options) }
  }
  return { method: `elicitation/create`, params: { mode: `form`, message, requestedSchema: { type: `object`, properties, ...(required.length ? { required } : {}) } } }
}

/** The card-answer an ElicitResult is (sessions.send `answer`), or null = nothing happens (the card stays open). */
export function cardAnswer(messageId: string, card: Card, result: unknown): Record<string, unknown> | null {
  const r = result as ElicitResult | null
  if (!r || typeof r !== `object` || card.sealed) return null
  if (r.action === `decline`) return { messageId, dismissed: true }
  if (r.action !== `accept`) return null
  const c = r.content ?? {}
  const text = typeof c.text === `string` ? c.text.trim() : ``
  if (card.kind === `ask`) {
    const picked = card.multi ? c.answers : typeof c.answer === `string` ? [c.answer] : undefined
    if (Array.isArray(picked) && picked.length > 0) {
      if (!picked.every((p) => typeof p === `string` && card.options.includes(p))) return null
      return { messageId, answers: picked }
    }
    return text && (card.options.length === 0 || card.custom !== false) ? { messageId, answer: text } : null
  }
  if (c.decision !== `approved` && c.decision !== `rejected`) return null
  const answers: Record<string, string[]> = {}
  for (const q of card.questions) {
    const v = c[`q_${q.id}`]
    if (v === undefined) continue
    if (typeof v !== `string` || !q.options.includes(v)) return null
    answers[q.id] = [v]
  }
  return { messageId, decision: c.decision, ...(Object.keys(answers).length ? { answers } : {}) }
}

// ── The wire and the methods ──────────────────────────────────────────

const iso = (ms: number) => new Date(ms).toISOString()

/** The Task object (a CreateTaskResult adds resultType "task", tasks/get "complete"). */
export function taskWire(task: TaskState): Record<string, unknown> {
  const out: Record<string, unknown> = {
    taskId: task.taskId,
    status: task.status,
    ...(task.statusMessage ? { statusMessage: task.statusMessage } : {}),
    createdAt: iso(task.createdAt),
    lastUpdatedAt: iso(task.lastUpdatedAt),
    ttlMs: TASK_TTL_MS,
    pollIntervalMs: TASK_POLL_INTERVAL_MS,
  }
  if (task.status === `input_required`) {
    const requests: Record<string, ElicitRequest> = {}
    for (const [id, card] of Object.entries(task.cards)) {
      const request = cardRequest(card)
      if (request) requests[id] = request
    }
    out.inputRequests = requests
  }
  if (task.status === `completed`) out.result = task.result
  if (task.status === `failed`) out.error = task.error
  return out
}

export interface TaskRequest {
  method: `tasks/get` | `tasks/update` | `tasks/cancel`
  params: Record<string, unknown> | null
  principal: string
  declared: boolean
  now: number
}

export type TaskEffect =
  | { kind: `answer`; sessionId: string; answer: Record<string, unknown> }
  | { kind: `stop`; sessionId: string }
  /** #FEED-295: cancelling a transfer task cancels the transfer. */
  | { kind: `cancel_transfer`; transferId: string }
  | { kind: `unwatch`; taskId: string }

export type TaskOutcome =
  | { ok: true; result: Record<string, unknown>; task: TaskState | null; effects: TaskEffect[] }
  | { ok: false; error: { code: number; message: string; data?: unknown } }

const fail = (key: keyof typeof fixture.errors, data?: unknown): TaskOutcome => ({
  ok: false,
  error: { code: (fixture.errorCodes as Record<string, number>)[key] ?? fixture.errorCodes.default, message: fixture.errors[key], ...(data !== undefined ? { data } : {}) },
})

/** One tasks/* request against the task it names (`lookup` = the principal-free store; `rate` = the principal's window). */
export function taskHandle(req: TaskRequest, lookup: (taskId: string) => TaskState | null, rate: BurstState): TaskOutcome {
  if (!req.declared) return fail(`missingCapability`, { requiredCapabilities: { extensions: { [TASKS_EXTENSION]: {} } } })
  if (burstAdmit(rate, req.now, fixture.rateLimit) !== `ok`) return fail(`rateLimited`)
  const taskId = req.params?.taskId
  if (typeof taskId !== `string` || !taskId) return fail(`missingTaskId`)
  const task = lookup(taskId)
  if (!task || task.principal !== req.principal) return fail(`notFound`)
  if (req.now >= task.createdAt + TASK_TTL_MS) return fail(`expired`)
  if (req.method === `tasks/get`) return { ok: true, result: { ...taskWire(task), resultType: `complete` }, task, effects: [] }
  if (req.method === `tasks/cancel`) {
    if (isTerminal(task.status)) return { ok: true, result: { resultType: `complete` }, task, effects: [] }
    const transfer = task.watch === `transfer` && task.transferId ? task.transferId : null
    const statusMessage = transfer ? fixture.copy.transferCancelled : fixture.copy.cancelled
    const cancelled: TaskState = { ...task, status: `cancelled`, cards: {}, statusMessage, lastUpdatedAt: req.now }
    const effects: TaskEffect[] = task.watch === `session` && task.sessionId ? [{ kind: `stop`, sessionId: task.sessionId }] : []
    if (transfer) effects.push({ kind: `cancel_transfer`, transferId: transfer })
    effects.push({ kind: `unwatch`, taskId: task.taskId })
    return { ok: true, result: { resultType: `complete` }, task: cancelled, effects }
  }
  const responses = req.params?.inputResponses
  if (!responses || typeof responses !== `object` || Array.isArray(responses)) return fail(`badResponses`)
  const effects: TaskEffect[] = []
  if (task.status === `input_required` && task.sessionId) {
    for (const [key, value] of Object.entries(responses as Record<string, unknown>)) {
      const card = task.cards[key]
      if (!card) continue
      const answer = cardAnswer(key, card, value)
      if (answer) effects.push({ kind: `answer`, sessionId: task.sessionId, answer })
    }
  }
  return { ok: true, result: { resultType: `complete` }, task, effects }
}
