import {
  agentAppTriggerKindValues,
  type AgentTrigger,
} from "@exp/db-schema/domain"
import { templatesValid, validateAutomations, type Automation, type AutomationTrigger, type TriggerKind } from "@exp/emcp/automations"
import type { ExposeRun } from "@exp/emcp/exposes"

// #EXP-1829 (ONE trigger system): an app's automations ARE its agent's
// triggers (`agents.triggers`). This module is the bridge between a stored
// trigger and the @exp/emcp automation executor: a trigger carrying `run`
// becomes one `Automation` (its name derived from the trigger id, so its
// audit rows stay `automation:<name>`), validated against the app's
// PUBLISHED package before it runs and against the head package when it is
// written. Pure: no DB.

const APP_KINDS: readonly string[] = agentAppTriggerKindValues

/** The automation name of a trigger (`automation:<name>` audit rows). */
export function triggerAutomationName(triggerId: string): string {
  return `t_${triggerId.replace(/-/g, ``).toLowerCase()}`
}

/** The trigger's when-part as an automation `on`. */
export function triggerOn(trigger: AgentTrigger): AutomationTrigger {
  switch (trigger.kind) {
    case `schedule`:
      return { schedule: { cron: trigger.cron, ...(trigger.tz ? { tz: trigger.tz } : {}) } }
    case `record`:
      return { record: { kind: trigger.record, op: trigger.op } }
    case `webhook`:
      return { webhook: { name: trigger.name } }
    default:
      return { event: trigger.name }
  }
}

/** The template roots a trigger's `prompt` and `run` may use. */
export function triggerTemplateKind(trigger: Pick<AgentTrigger, `kind`>): TriggerKind {
  return trigger.kind as TriggerKind
}

/** A `run` trigger as the automation the executor runs; null = a wake. */
export function triggerAutomation(trigger: AgentTrigger): Automation | null {
  if (!trigger.run) return null
  return { name: triggerAutomationName(trigger.id), on: triggerOn(trigger), run: trigger.run as unknown as ExposeRun }
}

export interface PackageContext {
  kinds: string[]
  upstreams: string[]
}

/** The record kinds and upstreams a package declares. */
export function packageContext(pkg: unknown): PackageContext {
  const records = (pkg as { records?: unknown } | null)?.records
  const upstreams = (pkg as { upstreams?: unknown } | null)?.upstreams
  return {
    kinds: records && typeof records === `object` && !Array.isArray(records) ? Object.keys(records) : [],
    upstreams: Array.isArray(upstreams) ? upstreams.filter((u): u is string => typeof u === `string`) : [],
  }
}

export interface RunTrigger {
  trigger: AgentTrigger
  automation: Automation
}

/** The enabled `run` triggers whose binding the PUBLISHED package can run
 *  (a stale one never runs; its row shows why). */
export function runnableTriggers(triggers: readonly AgentTrigger[] | null | undefined, pkg: unknown): RunTrigger[] {
  const list = (triggers ?? []).filter((t) => t && t.enabled !== false && typeof t.id === `string` && t.run)
  const entries = list.map((trigger) => ({ trigger, automation: triggerAutomation(trigger)! }))
  const errors = validateAutomations(
    entries.map((e) => e.automation),
    packageContext(pkg)
  )
  const broken = new Set(errors.map((e) => e.index))
  if (broken.has(null)) return []
  return entries.filter((_, i) => !broken.has(i))
}

/** The wake triggers (no `run`) of one kind, enabled. */
export function wakeTriggers(triggers: readonly AgentTrigger[] | null | undefined): AgentTrigger[] {
  return (triggers ?? []).filter((t) => t && t.enabled !== false && typeof t.id === `string` && !t.run)
}

/** #EXP-1829: whether a trigger needs an app's agent (an app kind or `run`). */
export function needsApp(trigger: Pick<AgentTrigger, `kind` | `run`>): boolean {
  return APP_KINDS.includes(trigger.kind) || !!trigger.run
}

export interface TriggerIssue {
  index: number
  code: string
}

/** Every write-time problem of a trigger list, in order. `pkg` = the app's
 *  HEAD package (null = the agent belongs to no app). */
export function appTriggerIssues(triggers: readonly AgentTrigger[], pkg: unknown | null): TriggerIssue[] {
  const issues: TriggerIssue[] = []
  const ctx = pkg === null ? null : packageContext(pkg)
  const hooks = new Set<string>()
  triggers.forEach((trigger, index) => {
    const fail = (code: string) => void issues.push({ index, code })
    if (needsApp(trigger) && ctx === null) return fail(`appOnly`)
    const kind = triggerTemplateKind(trigger)
    if (trigger.prompt && !templatesValid(trigger.prompt, kind)) fail(`template`)
    if (trigger.kind === `record` && ctx && !ctx.kinds.includes(trigger.record)) fail(`kind`)
    if (trigger.kind === `webhook`) {
      if (hooks.has(trigger.name)) fail(`webhook`)
      hooks.add(trigger.name)
    }
    if (trigger.run && ctx) {
      for (const error of validateAutomations([triggerAutomation(trigger)], ctx)) fail(error.code)
    }
  })
  return issues
}

/** The words of each write-time code (automations.json `errors` + ours). */
export const TRIGGER_ISSUE_MESSAGES: Record<string, string> = {
  appOnly: `record and webhook triggers and run belong to an app's agent`,
  template: `templates are \${record.<field>} (record triggers), \${event.<field>} (event and webhook triggers) and \${firedAt}`,
  kind: `the record kind is not one the app declares`,
  webhook: `two webhook triggers share a hook name`,
  schedule: `a run's schedule is a 5-field cron, at most every 15 minutes`,
  scheduleTooOften: `a schedule that runs a binding fires at most every 15 minutes`,
  chain: `a run never reaches an app through exponential_apps_*: write this app's records with a records run`,
  run: `run holds exactly one of records, action, upstream, agent (as in exposes)`,
  upstream: `run names none of the package's upstreams`,
  action: `an action run is one of the exponential_<noun>_<op> operations`,
  destructive: `a run is never destructive`,
  ownerOnly: `an owner-only operation is never a run`,
  agent: `agent names a team agent by its slugged name`,
  prompt: `an agent run needs a prompt`,
}

/** #EXP-1829 (domain.ts): a kept trigger (same id) that leaves `prompt` or
 *  `run` out keeps the stored value (an older client re-sends only the
 *  when-part); `null` clears it. Ids are minted for new triggers. */
export function mergeTriggerWrite(
  stored: readonly AgentTrigger[] | null | undefined,
  incoming: ReadonlyArray<Record<string, unknown> & { id?: string }>
): AgentTrigger[] {
  const byId = new Map((stored ?? []).filter((t) => t && typeof t.id === `string`).map((t) => [t.id, t]))
  return incoming.map((raw) => {
    const id = raw.id ?? crypto.randomUUID()
    const before = raw.id ? byId.get(raw.id) : undefined
    const out: Record<string, unknown> = { ...raw, id }
    for (const key of [`prompt`, `run`] as const) {
      if (out[key] === undefined && before?.[key] !== undefined && before[key] !== null) out[key] = before[key]
      if (out[key] === null || out[key] === undefined) delete out[key]
    }
    return out as unknown as AgentTrigger
  })
}

const PLACEHOLDER = /\$\{([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*)\}/g

/** A prompt with its `${…}` filled from the firing's input (values inline:
 *  strings as they are, the rest as JSON; a missing path = empty). */
export function fillPrompt(prompt: string, input: Record<string, unknown>): string {
  return prompt.replace(PLACEHOLDER, (_, path: string) => {
    let value: unknown = input
    for (const part of path.split(`.`)) {
      value = value && typeof value === `object` ? (value as Record<string, unknown>)[part] : undefined
    }
    if (value === undefined || value === null) return ``
    return typeof value === `string` ? value : JSON.stringify(value)
  })
}

/** A short label of a trigger's when-part (notifications, interim rows). */
export function triggerLabel(trigger: AgentTrigger): string {
  switch (trigger.kind) {
    case `schedule`:
      return `Schedule ${trigger.cron}${trigger.tz ? ` (${trigger.tz})` : ``}`
    case `record`:
      return `When a ${trigger.record} is ${trigger.op === `create` ? `created` : trigger.op === `update` ? `updated` : `deleted`}`
    case `webhook`:
      return `When webhook ${trigger.name} is called`
    default:
      return `On ${trigger.name}`
  }
}

/** #EXP-1829: one package automation `{name, on, run, sponsor?}` (the
 *  pre-#EXP-1829 format, an old app file's) as its agent's trigger. A run
 *  handing off to the app's OWN agent (`ownSlug`) becomes a wake with that
 *  prompt; a schedule without a zone keeps the team's (`teamZone`); the
 *  sponsor goes (a run acts as the agent's owner). Null = not convertible. */
export function automationAsTrigger(
  automation: Automation,
  ctx: { teamZone: string | null; ownSlug: string | null; id?: string }
): AgentTrigger | null {
  const on = automation.on as Record<string, unknown>
  const base = { id: ctx.id ?? crypto.randomUUID(), enabled: true }
  let when: Record<string, unknown>
  if (`schedule` in on) {
    const schedule = on.schedule as { cron: string; tz?: string }
    const tz = schedule.tz ?? ctx.teamZone ?? undefined
    when = { kind: `schedule`, cron: schedule.cron, ...(tz ? { tz } : {}) }
  } else if (`record` in on) {
    const record = on.record as { kind: string; op: string }
    when = { kind: `record`, record: record.kind, op: record.op }
  } else if (`webhook` in on) {
    when = { kind: `webhook`, name: (on.webhook as { name: string }).name }
  } else if (typeof on.event === `string`) {
    when = { kind: `event`, name: on.event }
  } else return null
  const run = automation.run as Record<string, unknown>
  const own = typeof run.agent === `string` && ctx.ownSlug !== null && run.agent === ctx.ownSlug
  const prompt = own && typeof run.prompt === `string` && run.prompt.trim() ? run.prompt.trim() : undefined
  return { ...base, ...when, ...(own ? (prompt ? { prompt } : {}) : { run }) } as unknown as AgentTrigger
}
