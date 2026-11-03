// #VAPP-220 (eApps A1): an app's AUTOMATIONS: one trigger (a schedule, a
// record write, a team event) runs one `exposes`-style binding in the server
// broker, on the published package, as the automation's sponsor. Pure and
// locked by `fixtures/automations.json` (its `$comment` is the spec); the
// server (`apps/web/src/lib/apps/automations.ts`) and the Rust crate `emcp`
// (`contract/automations.rs`) replay the same cases.

import fixture from "../fixtures/automations.json" with { type: "json" }
import { agentAuditTool, resolveArgs, runVia, validateExposes, type AgentRun, type Expose, type ExposeRun } from "./exposes"

export const AUTOMATIONS_LIMITS = fixture.limits
export const AUTOMATION_ERRORS: readonly string[] = fixture.errors

export type AutomationTrigger =
  | { schedule: { cron: string; tz?: string } }
  | { record: { kind: string; op: `create` | `update` | `delete` } }
  | { event: string }
  | { webhook: { name: string } }

export interface Automation {
  name: string
  on: AutomationTrigger
  run: ExposeRun
  /** A member's email; default = the publisher (`resolveSponsor`). */
  sponsor?: string
}

export type TriggerKind = `schedule` | `record` | `event` | `webhook`

export interface AutomationError {
  index: number | null
  code: string
}

export interface AutomationContext {
  kinds: readonly string[]
  upstreams: readonly string[]
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const NAME = new RegExp(fixture.limits.namePattern)
const EVENT = new RegExp(fixture.limits.eventPattern)
const CRON_FIELD = new RegExp(fixture.cronFieldPattern)
const TRIGGER_KINDS: readonly string[] = fixture.triggerKinds
const RECORD_OPS: readonly string[] = fixture.recordTriggerOps
const ROOTS = fixture.templateRoots as Record<TriggerKind, string[]>

/** The kind of an automation's `on` (exactly one key), else null. */
export function triggerKind(on: unknown): TriggerKind | null {
  if (!isObject(on)) return null
  const keys = TRIGGER_KINDS.filter((k) => k in on)
  return keys.length === 1 ? (keys[0] as TriggerKind) : null
}

/** One cron field's values (`*`, lists, ranges, steps; digits only), or null. */
export function cronFieldValues(raw: string, min: number, max: number): number[] | null {
  const out = new Set<number>()
  for (const part of raw.split(`,`)) {
    const [range, step, ...extra] = part.split(`/`)
    if (extra.length > 0 || range === undefined || range === ``) return null
    let by = 1
    if (step !== undefined) {
      if (!/^\d+$/.test(step) || Number(step) < 1) return null
      by = Number(step)
    }
    let lo: number
    let hi: number
    if (range === `*`) {
      lo = min
      hi = max
    } else {
      const bounds = range.split(`-`)
      if (bounds.length > 2 || !bounds.every((b) => /^\d+$/.test(b))) return null
      lo = Number(bounds[0])
      hi = bounds.length === 2 ? Number(bounds[1]) : step !== undefined ? max : lo
    }
    if (lo < min || hi > max || lo > hi) return null
    for (let v = lo; v <= hi; v += by) out.add(v)
  }
  return [...out].sort((a, b) => a - b)
}

/** A schedule's check: null = fine, else `schedule` | `scheduleTooOften`.
 *  The gap = the smallest distance between the minutes of a day it fires
 *  on (minute × hour, circular over the day). */
export function scheduleError(schedule: unknown): `schedule` | `scheduleTooOften` | null {
  if (!isObject(schedule) || typeof schedule.cron !== `string`) return `schedule`
  if (schedule.tz !== undefined && (typeof schedule.tz !== `string` || schedule.tz.trim() === ``)) return `schedule`
  const fields = schedule.cron.trim().split(/\s+/)
  if (fields.length !== 5 || !fields.every((f) => CRON_FIELD.test(f))) return `schedule`
  const minutes = cronFieldValues(fields[0]!, 0, 59)
  const hours = cronFieldValues(fields[1]!, 0, 23)
  if (!minutes || !hours) return `schedule`
  const day = hours.flatMap((h) => minutes.map((m) => h * 60 + m))
  if (day.length < 2) return null
  let gap = 1440 - day[day.length - 1]! + day[0]!
  for (let i = 1; i < day.length; i++) gap = Math.min(gap, day[i]! - day[i - 1]!)
  return gap < fixture.limits.minScheduleGapMinutes ? `scheduleTooOften` : null
}

const PLACEHOLDER = /\$\{([^}]*)\}/g
const ROOT_PATH = /^([A-Za-z_][A-Za-z0-9_]*)((?:\.[A-Za-z_][A-Za-z0-9_]*)*)$/

/** Every `${…}` of a value is a root the trigger carries (`templateRoots`):
 *  `record.<path>`, `event.<path>`, `firedAt`. */
export function templatesValid(value: unknown, kind: TriggerKind): boolean {
  const roots = ROOTS[kind]
  const ok = (text: string) =>
    [...text.matchAll(PLACEHOLDER)].every((m) => {
      const parts = m[1]!.match(ROOT_PATH)
      if (!parts || !roots.includes(parts[1]!)) return false
      return parts[1] === `firedAt` ? parts[2] === `` : parts[2] !== ``
    })
  const walk = (v: unknown): boolean =>
    typeof v === `string` ? ok(v) : Array.isArray(v) ? v.every(walk) : isObject(v) ? Object.values(v).every(walk) : true
  return walk(value)
}

/** Rewrites `${record.…}`, `${event.…}`, `${firedAt}` to exposes'
 *  `${input.<root>…}`, so `resolveArgs` fills them. */
export function toInputTemplates<T>(value: T): T {
  const walk = (v: unknown): unknown => {
    if (typeof v === `string`) return v.replace(/\$\{(record|event|firedAt)(?=[.}])/g, (_, root: string) => `\${input.${root}`)
    if (Array.isArray(v)) return v.map(walk)
    if (isObject(v)) return Object.fromEntries(Object.entries(v).map(([k, item]) => [k, walk(item)]))
    return v
  }
  return walk(value) as T
}

/** The run with its templates filled from the trigger's input
 *  `{record?, event?, firedAt}`. */
export function resolveAutomationRun(run: ExposeRun, input: Record<string, unknown>): ExposeRun {
  return resolveArgs(toInputTemplates(run), input) as ExposeRun
}

/** The run as an exposes binding (the broker runs it like a tool). */
export function automationExpose(automation: Pick<Automation, `name` | `run`>): Expose {
  return {
    name: automation.name,
    title: automation.name,
    description: automation.name,
    input: { type: `object`, properties: { record: {}, event: {}, firedAt: {} } },
    access: `write`,
    run: toInputTemplates(automation.run),
  }
}

/** Every error of an app's `automations`, in order (empty = valid). */
export function validateAutomations(list: unknown, ctx: AutomationContext): AutomationError[] {
  if (!Array.isArray(list)) return [{ index: null, code: `list` }]
  const errors: AutomationError[] = []
  if (list.length > fixture.limits.maxAutomations) errors.push({ index: null, code: `tooMany` })
  const seen = new Set<string>()
  const hooks = new Set<string>()
  list.forEach((item, index) => {
    const fail = (code: string) => void errors.push({ index, code })
    if (!isObject(item)) return fail(`shape`)
    const name = item.name
    if (typeof name !== `string` || !NAME.test(name) || name.length > fixture.limits.nameMax) fail(`name`)
    else if (seen.has(name)) fail(`duplicate`)
    else seen.add(name)
    const kind = triggerKind(item.on)
    if (!kind) return fail(`on`)
    const on = item.on as Record<string, unknown>
    if (kind === `schedule`) {
      const error = scheduleError(on.schedule)
      if (error) fail(error)
    } else if (kind === `record`) {
      const record = on.record
      if (!isObject(record) || typeof record.op !== `string` || !RECORD_OPS.includes(record.op)) return fail(`on`)
      if (typeof record.kind !== `string` || !ctx.kinds.includes(record.kind)) fail(`kind`)
    } else if (kind === `webhook`) {
      // #VAPP-237 (A3): a hook name, unique among the app's webhooks.
      const hook = isObject(on.webhook) ? on.webhook.name : undefined
      if (typeof hook !== `string` || !NAME.test(hook) || hook.length > fixture.limits.nameMax || hooks.has(hook)) fail(`webhook`)
      else hooks.add(hook)
    } else if (typeof on.event !== `string` || !EVENT.test(on.event) || on.event.length > fixture.limits.eventMax) fail(`event`)
    if (item.sponsor !== undefined && (typeof item.sponsor !== `string` || !item.sponsor.includes(`@`))) fail(`sponsor`)
    const run = item.run
    if (!isObject(run)) return fail(`run`)
    if (!templatesValid(run, kind)) fail(`template`)
    if (typeof run.action === `string` && run.action.startsWith(`exponential.apps.`)) return fail(`chain`)
    for (const error of validateExposes([automationExpose({ name: `automation`, run: run as ExposeRun })], ctx)) {
      if (error.code !== `template` && error.code !== `accessTooLow`) fail(error.code)
    }
  })
  return errors
}

/** The trigger a record write, an event or a webhook delivery is, for
 *  `matchAutomations`; `byAgent` = the slug of the agent whose session
 *  tree caused it. */
export type FiredBy = ({ record: { kind: string; op: string } } | { event: string } | { webhook: string }) & { byAgent?: string | null }

/** The automations a record write or event fires, in package order. LOOP
 *  GUARD: a write or event caused by an automation run fires nothing; one
 *  caused from an agent's tree (`byAgent`) fires no hand-off to that agent. */
export function matchAutomations(list: unknown, firedBy: FiredBy, fromAutomation: boolean): string[] {
  if (fromAutomation || !Array.isArray(list)) return []
  const out: string[] = []
  for (const item of list) {
    if (!isObject(item) || typeof item.name !== `string`) continue
    if (firedBy.byAgent && isObject(item.run) && item.run.agent === firedBy.byAgent) continue
    const kind = triggerKind(item.on)
    const on = item.on as Record<string, unknown>
    if (kind === `record` && `record` in firedBy && isObject(on.record) && on.record.kind === firedBy.record.kind && on.record.op === firedBy.record.op) out.push(item.name)
    else if (kind === `event` && `event` in firedBy && on.event === firedBy.event) out.push(item.name)
    else if (kind === `webhook` && `webhook` in firedBy && isObject(on.webhook) && on.webhook.name === firedBy.webhook) out.push(item.name)
  }
  return out
}

export interface SponsorMember {
  id: string
  email: string
  role: string
  /** The member's app access (`none` | `read` | `write`). */
  level: string
}

export type SponsorState = (typeof fixture.sponsorStates)[number]

export type SponsorResult = { userId: string } | { paused: SponsorState }

/** C23: who an automation acts as, or why it is paused. */
export function resolveSponsor(input: { declared: string | null | undefined; publishedBy: string | null | undefined; members: readonly SponsorMember[] }): SponsorResult {
  const publisher = input.publishedBy ? input.members.find((m) => m.id === input.publishedBy) : undefined
  let sponsor: SponsorMember | undefined = publisher
  const declared = input.declared?.trim().toLowerCase()
  if (declared && declared !== publisher?.email.toLowerCase()) {
    if (!publisher) return { paused: `sponsorGone` }
    if (publisher.role !== `owner`) return { paused: `sponsorNotAllowed` }
    sponsor = input.members.find((m) => m.email.toLowerCase() === declared)
  }
  if (!sponsor) return { paused: `sponsorGone` }
  if (sponsor.level !== `write`) return { paused: `sponsorReadOnly` }
  return { userId: sponsor.id }
}

/** Whether another run may start (the app's runs of the last 60 s). */
export function rateAllows(runsLastMinute: number): boolean {
  return runsLastMinute < fixture.limits.runsPerMinute
}

/** The audit row's `client_id`. */
export function automationClientId(name: string): string {
  return `${fixture.limits.clientPrefix}${name}`
}

/** The audit row's `tool`. */
export function automationTool(run: ExposeRun): string {
  const via = runVia(run)
  if (via === `records`) {
    const records = (run as { records: { op: string; kind: string } }).records
    return `records.${records.op}:${records.kind}`
  }
  if (via === `action`) return (run as { action: string }).action
  if (via === `agent`) return agentAuditTool((run as AgentRun).agent)
  const upstream = run as { upstream: string; tool: string }
  return `${upstream.upstream}/${upstream.tool}`
}
