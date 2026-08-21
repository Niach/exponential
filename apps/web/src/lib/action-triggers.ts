import {
  actionScheduleIntervalValues,
  actionTriggerEventValues,
  issuePriorityValues,
  type AutomationEventTriggerFilters,
  type AutomationScheduleTrigger,
  type AutomationTrigger,
  type ActionTriggerEvent,
  type IssuePriority,
} from "@exp/db-schema/domain"

// Client-side trigger helpers (EXP-530; automations split out of actions in
// EXP-583 — the trigger is the when-part only, device/enabled/agent are
// columns on the automations row). Deliberately DB-free (the
// lib/action-inputs.ts precedent) so dialogs and tests import it without
// pulling server code. Reads are TOLERANT — the strict write union lives in
// domain.ts; here anything unparseable (a future trigger kind, a future event
// value, a corrupted payload) degrades to `null` = "no automation", never a
// throw. Old clients must keep working when the vocabulary grows.

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === `object` && value !== null && !Array.isArray(value)
}

function isIntInRange(value: unknown, min: number, max: number): value is number {
  return typeof value === `number` && Number.isInteger(value) && value >= min && value <= max
}

function includesValue<T extends string>(
  values: readonly T[],
  value: unknown
): value is T {
  return typeof value === `string` && (values as readonly string[]).includes(value)
}

function idList(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined
  const ids = value.filter(
    (entry): entry is string => typeof entry === `string` && entry.length > 0
  )
  return ids.length > 0 ? ids : undefined
}

function priorityList(value: unknown): IssuePriority[] | undefined {
  if (!Array.isArray(value)) return undefined
  const priorities = value.filter((entry): entry is IssuePriority =>
    includesValue(issuePriorityValues, entry)
  )
  return priorities.length > 0 ? priorities : undefined
}

/** Tolerant read of an automations row's `trigger` jsonb. Never throws. */
export function parseAutomationTrigger(value: unknown): AutomationTrigger | null {
  if (!isRecord(value)) return null

  if (value.kind === `schedule`) {
    if (!includesValue(actionScheduleIntervalValues, value.interval)) return null
    if (!isIntInRange(value.minuteOfDay, 0, 1439)) return null
    const trigger: AutomationScheduleTrigger = {
      kind: `schedule`,
      interval: value.interval,
      minuteOfDay: value.minuteOfDay,
    }
    if (value.interval === `weekly`) {
      if (!isIntInRange(value.weekday, 1, 7)) return null
      trigger.weekday = value.weekday
    }
    if (value.interval === `monthly`) {
      if (!isIntInRange(value.dayOfMonth, 1, 28)) return null
      trigger.dayOfMonth = value.dayOfMonth
    }
    return trigger
  }

  if (value.kind === `event`) {
    // An old client reading a FUTURE event kind treats it as "no automation".
    if (!includesValue(actionTriggerEventValues, value.event)) return null
    const filters: AutomationEventTriggerFilters = {}
    if (isRecord(value.filters)) {
      const boardIds = idList(value.filters.boardIds)
      const labelIds = idList(value.filters.labelIds)
      const priorities = priorityList(value.filters.priorities)
      const toStatusIds = idList(value.filters.toStatusIds)
      if (boardIds) filters.boardIds = boardIds
      if (labelIds) filters.labelIds = labelIds
      if (priorities) filters.priorities = priorities
      if (toStatusIds) filters.toStatusIds = toStatusIds
    }
    return {
      kind: `event`,
      event: value.event,
      ...(Object.keys(filters).length > 0 ? { filters } : {}),
    }
  }

  return null
}

// ISO weekday names, index = weekday - 1 (1=Mon … 7=Sun).
const WEEKDAY_NAMES = [
  `Monday`,
  `Tuesday`,
  `Wednesday`,
  `Thursday`,
  `Friday`,
  `Saturday`,
  `Sunday`,
] as const

export function weekdayName(weekday: number): string {
  return WEEKDAY_NAMES[weekday - 1] ?? `Monday`
}

function formatMinuteOfDay(minuteOfDay: number): string {
  const hours = String(Math.floor(minuteOfDay / 60)).padStart(2, `0`)
  const minutes = String(minuteOfDay % 60).padStart(2, `0`)
  return `${hours}:${minutes}`
}

/** Event picker labels — `triggerSummary` derives its "When …" sentence from
 * these, so the two surfaces can never disagree. */
export const TRIGGER_EVENT_LABELS: Record<ActionTriggerEvent, string> = {
  created: `An issue is created`,
  status_changed: `Status changes`,
  assignee_changed: `The assignee changes`,
  label_added: `A label is added`,
  priority_changed: `Priority changes`,
  pr_opened: `A pull request is opened`,
  pr_merged: `A pull request is merged`,
}

function eventFilterCount(filters: AutomationEventTriggerFilters | undefined): number {
  if (!filters) return 0
  return (
    (filters.boardIds?.length ?? 0) +
    (filters.labelIds?.length ?? 0) +
    (filters.priorities?.length ?? 0) +
    (filters.toStatusIds?.length ?? 0)
  )
}

/** The shared one-line trigger sentence (cards, Automations tab rows). */
export function triggerSummary(trigger: AutomationTrigger): string {
  if (trigger.kind === `schedule`) {
    const at = formatMinuteOfDay(trigger.minuteOfDay)
    if (trigger.interval === `weekly`) {
      return `Weekly on ${weekdayName(trigger.weekday ?? 1)} at ${at}`
    }
    if (trigger.interval === `monthly`) {
      return `Monthly on day ${trigger.dayOfMonth ?? 1} at ${at}`
    }
    return `Daily at ${at}`
  }
  const label = TRIGGER_EVENT_LABELS[trigger.event]
  const sentence = `When ${label.charAt(0).toLowerCase()}${label.slice(1)}`
  const count = eventFilterCount(trigger.filters)
  if (count === 0) return sentence
  return `${sentence} · ${count} ${count === 1 ? `filter` : `filters`}`
}

/**
 * Next occurrence STRICTLY after `now`, computed in the BROWSER's timezone —
 * the schedule actually runs in the bound device's local time, so callers
 * label the result "(device time)".
 */
export function nextScheduleRun(
  trigger: AutomationScheduleTrigger,
  now: Date
): Date | null {
  const hours = Math.floor(trigger.minuteOfDay / 60)
  const minutes = trigger.minuteOfDay % 60
  const atTime = (base: Date): Date => {
    const candidate = new Date(base)
    candidate.setHours(hours, minutes, 0, 0)
    return candidate
  }

  if (trigger.interval === `daily`) {
    const today = atTime(now)
    if (today > now) return today
    const tomorrow = new Date(now)
    tomorrow.setDate(tomorrow.getDate() + 1)
    return atTime(tomorrow)
  }

  if (trigger.interval === `weekly`) {
    if (!isIntInRange(trigger.weekday, 1, 7)) return null
    // JS getDay is 0=Sun; ISO weekday is 1=Mon … 7=Sun.
    for (let offset = 0; offset <= 7; offset++) {
      const day = new Date(now)
      day.setDate(day.getDate() + offset)
      const isoWeekday = ((day.getDay() + 6) % 7) + 1
      if (isoWeekday !== trigger.weekday) continue
      const candidate = atTime(day)
      if (candidate > now) return candidate
    }
    return null
  }

  if (!isIntInRange(trigger.dayOfMonth, 1, 28)) return null
  // dayOfMonth caps at 28, so the day exists in every month.
  const thisMonth = new Date(
    now.getFullYear(),
    now.getMonth(),
    trigger.dayOfMonth,
    hours,
    minutes,
    0,
    0
  )
  if (thisMonth > now) return thisMonth
  return new Date(
    now.getFullYear(),
    now.getMonth() + 1,
    trigger.dayOfMonth,
    hours,
    minutes,
    0,
    0
  )
}

/** What an action+automation suggestion (or the create dialog's Automation
 * block) asks the creator agent to set up alongside the new action. */
export interface AutomationSpec {
  trigger: AutomationTrigger
  deviceId: string
  agent?: string
  model?: string
  effort?: string
}

/**
 * The machine-readable block the create dialog appends to the builtin
 * "Create action" description — the creator agent creates the action, then
 * copies this JSON verbatim into `exponential_automations_create` (adding the
 * new action's id). The dialog never talks to the server itself.
 */
export function formatAutomationBlock(spec: AutomationSpec): string {
  const payload: Record<string, unknown> = {
    deviceId: spec.deviceId,
    trigger: spec.trigger,
  }
  if (spec.agent) payload.agent = spec.agent
  if (spec.model) payload.model = spec.model
  if (spec.effort) payload.effort = spec.effort
  return `\n\nAutomation — after creating the action, call exponential_automations_create with its id and exactly these fields: \`${JSON.stringify(payload)}\`. An automated run fills no inputs, so declare none as required.`
}
