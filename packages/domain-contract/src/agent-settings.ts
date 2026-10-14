// EXP-1291 Threads v2: Team settings → Agents. Pure and byte-locked by
// `fixtures/agent-settings.json` (`summaryCases`, `validationCases`), which
// every mirror replays: desktop `domain::agent_settings`, iOS ExpCore
// `AgentSettings.swift`, Android `domain/AgentSettings.kt`.

import fixture from "../fixtures/agent-settings.json" with { type: "json" }
import contractJson from "../contract.json" with { type: "json" }

export const AGENT_SETTINGS_FIELDS = fixture.fields
export const AGENT_SETTINGS_COPY = fixture.copy

export interface AgentTriggerLike {
  kind: string
  enabled?: boolean
  interval?: string
  minuteOfDay?: number
  weekday?: number
  dayOfMonth?: number
  event?: string
}

function clock(minuteOfDay: number | undefined): string {
  const minutes = Math.max(0, Math.min(1439, Math.floor(minuteOfDay ?? 0)))
  const pad = (value: number) => String(value).padStart(2, `0`)
  return `${pad(Math.floor(minutes / 60))}:${pad(minutes % 60)}`
}

/** One Schedule row's text. */
export function triggerSummary(trigger: AgentTriggerLike): string {
  const copy = fixture.copy
  let text = ``
  if (trigger.kind === `schedule`) {
    const time = clock(trigger.minuteOfDay)
    if (trigger.interval === `weekly`) {
      const weekday = copy.weekdays[((trigger.weekday ?? 1) - 1 + 7) % 7]
      text = copy.schedule.weekly.replace(`{weekday}`, weekday).replace(`{time}`, time)
    } else if (trigger.interval === `monthly`) {
      text = copy.schedule.monthly
        .replace(`{day}`, String(trigger.dayOfMonth ?? 1))
        .replace(`{time}`, time)
    } else {
      text = copy.schedule.daily.replace(`{time}`, time)
    }
  } else {
    text = (copy.events as Record<string, string>)[trigger.event ?? ``] ?? trigger.event ?? ``
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
