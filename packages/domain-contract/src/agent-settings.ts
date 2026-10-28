// EXP-1291/NEW-2: Team settings → Agents. Pure and byte-locked by
// `fixtures/agent-settings.json` (`summaryCases`, `validationCases`), which
// every mirror replays: desktop `domain::agent_settings`, iOS ExpCore
// `AgentSettings.swift`, Android `domain/AgentSettings.kt`.

import fixture from "../fixtures/agent-settings.json" with { type: "json" }
import contractJson from "../contract.json" with { type: "json" }
import { scheduleSummary } from "./schedule-picker"

export const AGENT_SETTINGS_FIELDS = fixture.fields
export const AGENT_SETTINGS_COPY = fixture.copy

export interface AgentTriggerLike {
  kind: string
  enabled?: boolean
  /** schedule: the 5-field cron and its IANA zone. */
  cron?: string
  tz?: string | null
  /** event: the events row name it listens on. */
  name?: string
}

/** One Triggers row's text. */
export function triggerSummary(trigger: AgentTriggerLike): string {
  const copy = fixture.copy
  let text = ``
  if (trigger.kind === `schedule`) {
    // NEW-2: the schedule picker's sentence (`schedule-picker.json`).
    text = scheduleSummary(trigger.cron ?? ``, trigger.tz)
  } else {
    const name = trigger.name ?? ``
    text = (copy.events as Record<string, string>)[name] ?? copy.event.replace(`{name}`, name)
  }
  return trigger.enabled === false ? `${text} · ${copy.paused}` : text
}

export interface AgentForm {
  name: string
  deviceId: string | null
  instructions: string
}

export interface AgentFormError {
  field: string
  message: string
}

/** Field errors in field order; [] = valid. */
export function validateAgentForm(form: AgentForm): AgentFormError[] {
  const errors: AgentFormError[] = []
  const caps = contractJson.sessionsSend
  const name = form.name.trim()
  if (!name) errors.push({ field: `identity`, message: fixture.copy.errors.nameRequired })
  else if ([...name].length > caps.agentNameMax)
    errors.push({ field: `identity`, message: fixture.copy.errors.nameTooLong })
  if (!form.deviceId) errors.push({ field: `device`, message: fixture.copy.errors.deviceRequired })
  if (new TextEncoder().encode(form.instructions).length > caps.agentInstructionsMaxBytes)
    errors.push({ field: `instructions`, message: fixture.copy.errors.instructionsTooLong })
  return errors
}

export interface AgentAccountOptionLike {
  id: string
  agent: string
  email: string
  isLastUsed: boolean
}

/** EXP-1306: the login the Account row reads (its agent mark + email, the
 *  composer chip's value); null = no login reported (`copy.accountLastUsed`). */
export function agentAccountValue<T extends AgentAccountOptionLike>(input: {
  options: readonly T[]
  account: string | null
  cli: string | null
}): T | null {
  const { options, account, cli } = input
  return (
    (account ? options.find((option) => option.id === account && (!cli || option.agent === cli)) : undefined) ??
    (cli ? options.find((option) => option.agent === cli) : undefined) ??
    options.find((option) => option.isLastUsed) ??
    options[0] ??
    null
  )
}

export interface AgentActorRowLike {
  id: string
  name: string
  icon: string | null
  color: string | null
}

export type IssueActor =
  | { kind: `agent`; id: string; name: string; icon: string | null; color: string | null }
  | { kind: `user`; id: string }

/** #EXP-1383: who an issue event / comment / issue creation reads as
 *  (`actor.actorCases`): a synced agent first, else the member, else null. */
export function issueActor(
  input: { actorAgentId: string | null | undefined; actorUserId: string | null | undefined },
  agents: readonly AgentActorRowLike[]
): IssueActor | null {
  const agent = input.actorAgentId ? agents.find((row) => row.id === input.actorAgentId) : undefined
  if (agent) return { kind: `agent`, id: agent.id, name: agent.name, icon: agent.icon, color: agent.color }
  if (input.actorUserId) return { kind: `user`, id: input.actorUserId }
  return null
}

/** #EXP-1383: the assignee picker's option ids ×4 (`assignee.optionCases`):
 *  unassigned, the members in the given order, then the team's agents by
 *  sortOrder then name. */
export function assigneeOptions(input: {
  members: readonly { id: string }[]
  agents: readonly { id: string; name: string; sortOrder: number }[]
}): string[] {
  const agents = [...input.agents].sort(
    (a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name) || a.id.localeCompare(b.id)
  )
  return [
    `unassigned`,
    ...input.members.map((member) => `user:${member.id}`),
    ...agents.map((agent) => `agent:${agent.id}`),
  ]
}

// S-3 (owner 2026-10-28): THE ASSISTANT (`assistant` in the fixture; contract
// `agentRole`). Replayed by every mirror (`scheduleCases`, `visibleCases`,
// `orderCases`).
export const ASSISTANT = fixture.assistant
export const ASSISTANT_COPY = fixture.assistant.copy
export type AssistantScheduleId = `off` | `15m` | `30m` | `1h` | `daily`
export const ASSISTANT_SCHEDULES = fixture.assistant.schedules as {
  id: AssistantScheduleId
  label: string
  cron: string | null
}[]

/** The preset stored triggers stand for; null = not an Assistant's
 *  (an event, a custom cron, more than one) and `agents.update` refuses it. */
export function assistantScheduleOf(triggers: AgentTriggerLike[]): AssistantScheduleId | null {
  if (triggers.length === 0) return `off`
  if (triggers.length > 1) return null
  const [trigger] = triggers
  if (trigger!.kind !== `schedule`) return null
  const preset = ASSISTANT_SCHEDULES.find((schedule) => schedule.cron !== null && schedule.cron === trigger!.cron)
  if (!preset) return null
  return trigger!.enabled === false ? `off` : preset.id
}

/** The triggers a preset saves as (`off` = none). */
export function assistantScheduleTriggers(
  id: AssistantScheduleId,
  tz?: string | null
): { kind: `schedule`; cron: string; enabled: true; tz?: string }[] {
  const cron = ASSISTANT_SCHEDULES.find((schedule) => schedule.id === id)?.cron
  if (!cron) return []
  return [{ kind: `schedule`, cron, enabled: true, ...(tz ? { tz } : {}) }]
}

/** The Assistant's Model row value (`modelValueCases`): the pinned model,
 *  else the dispatcher root's default for the login's CLI (never `Last
 *  used`). */
export function assistantModelValue(input: { model?: string | null; cli?: string | null }): string {
  const pinned = input.model?.trim()
  if (pinned) return input.model!
  const defaults = contractJson.deviceAgentDefaults
  return input.cli === `codex` ? defaults.codexThreadModel : defaults.threadModel
}

export interface AgentRoleLike {
  role?: string | null
  ownerId: string
}

/** Another member's Assistant is never listed (settings, composer, chats). */
export function visibleAgent(agent: AgentRoleLike, me: string | null | undefined): boolean {
  return agent.role !== `assistant` || agent.ownerId === me
}

export function isAssistant(agent: { role?: string | null }): boolean {
  return agent.role === `assistant`
}

/** The agents a member sees, in order: their Assistant first, then the team
 *  agents by sortOrder, then name. */
export function orderedAgents<T extends AgentRoleLike & { name: string; sortOrder: number }>(
  agents: T[],
  me: string | null | undefined
): T[] {
  const visible = agents.filter((agent) => visibleAgent(agent, me))
  return [...visible].sort((a, b) => {
    const rank = Number(!isAssistant(a)) - Number(!isAssistant(b))
    if (rank !== 0) return rank
    if (a.sortOrder !== b.sortOrder) return a.sortOrder - b.sortOrder
    return a.name.localeCompare(b.name)
  })
}
