import { useMemo, useState } from "react"
import {
  builtinEventNames,
  cronSchema,
  eventNameSchema,
  timeZoneSchema,
  type TriggerWhen,
} from "@exp/db-schema/domain"
import { AGENT_SETTINGS_COPY } from "@/lib/session-tree"
import { parseCron } from "@/lib/agents/cron"
import { accountOptionsOf } from "@/components/launch-dialog/use-launch-options"
import {
  accountOptionKey,
  lastUsedAccountOption,
  type AccountOption,
} from "@/lib/accounts/account-option"
import type { SteerDevice } from "@/lib/steer-devices"
import {
  FieldError,
  GlassGroup,
  GlassTabsRow,
  Picker,
  ScheduleFields,
  TabsTrigger,
  emptyScheduleDraft,
  scheduleDraftCron,
  scheduleDraftFromCron,
  validateScheduleDraft,
  type ScheduleDraft,
} from "@exp/ui"

// The reusable trigger editing PIECES (EXP-530; NEW-2: a trigger lives on
// its `agents` row, which owns the device and account, so only the WHEN is
// edited): a schedule = THE schedule picker (`@exp/ui` `ScheduleFields`,
// `fixtures/schedule-picker.json` ×4 — nobody types cron; the stored form is
// a 5-field cron in an IANA zone), an event = ONE event name (the contract
// builtins offered, any custom name allowed). Plus the account-pin helpers
// the agent page's Account row shares. CONTROLLED: the parent holds a
// `TriggerDraft` and seeds it on open.

export interface TriggerDraft {
  kind: `schedule` | `event`
  /** The schedule picker's rows (the cron is derived). */
  schedule: ScheduleDraft
  /** The events row name. */
  name: string
}

export function emptyTriggerDraft(): TriggerDraft {
  return { kind: `schedule`, schedule: emptyScheduleDraft(), name: `` }
}

export function draftFromTrigger(trigger: TriggerWhen | null): TriggerDraft {
  const draft = emptyTriggerDraft()
  if (!trigger) return draft
  if (trigger.kind === `schedule`) {
    return { ...draft, kind: `schedule`, schedule: scheduleDraftFromCron(trigger.cron, trigger.tz) }
  }
  return { ...draft, kind: `event`, name: trigger.name ?? `` }
}

export interface TriggerDraftError {
  field: `days` | `cron` | `tz` | `name`
  message: string
}

const firstIssue = (result: { success: boolean; error?: { issues: { message: string }[] } }) =>
  result.success ? null : (result.error?.issues[0]?.message ?? `Invalid`)

/** Inline errors for the draft's visible pane; [] = savable. */
export function validateTriggerDraft(draft: TriggerDraft): TriggerDraftError[] {
  const errors: TriggerDraftError[] = []
  if (draft.kind === `schedule`) {
    errors.push(...validateScheduleDraft(draft.schedule))
    const cron = scheduleDraftCron(draft.schedule)
    if (errors.length === 0 && (firstIssue(cronSchema.safeParse(cron)) || !parseCron(cron))) {
      errors.push({ field: `cron`, message: `Use five fields: minute hour day month weekday` })
    }
    if (firstIssue(timeZoneSchema.safeParse(draft.schedule.tz))) {
      errors.push({ field: `tz`, message: `Pick a time zone` })
    }
    return errors
  }
  if (!draft.name.trim()) errors.push({ field: `name`, message: `Name the event` })
  else if (firstIssue(eventNameSchema.safeParse(draft.name))) {
    errors.push({ field: `name`, message: `Use letters, digits and . _ : / -` })
  }
  return errors
}

/** The draft as a strict `TriggerWhen` (call after `validateTriggerDraft`). */
export function draftToTrigger(draft: TriggerDraft): TriggerWhen {
  if (draft.kind === `schedule`) {
    return { kind: `schedule`, cron: scheduleDraftCron(draft.schedule), tz: draft.schedule.tz }
  }
  return { kind: `event`, name: draft.name.trim() }
}

/** The label the event Picker reads for a name: a builtin's sentence, else
 *  the name itself. */
function eventLabel(name: string): string {
  return (AGENT_SETTINGS_COPY.events as Record<string, string>)[name] ?? name
}

/** The when-part editor: Schedule | On event, then the matching pane. */
export function TriggerWhenFields({
  draft,
  onChange,
  errors = [],
}: {
  draft: TriggerDraft
  onChange: (draft: TriggerDraft) => void
  errors?: readonly TriggerDraftError[]
}) {
  const set = (patch: Partial<TriggerDraft>) => onChange({ ...draft, ...patch })
  const errorOf = (field: TriggerDraftError[`field`]) =>
    errors.find((error) => error.field === field)?.message ?? null
  const fieldErrors =
    draft.kind === `schedule`
      ? [errorOf(`days`), errorOf(`cron`), errorOf(`tz`)].filter(Boolean)
      : [errorOf(`name`)].filter(Boolean)

  return (
    <div className="space-y-1">
      <GlassGroup>
        <GlassTabsRow
          value={draft.kind}
          onValueChange={(kind) => {
            if (kind === draft.kind) return
            set({ kind: kind as TriggerDraft[`kind`] })
          }}
        >
          <TabsTrigger value="schedule">Schedule</TabsTrigger>
          <TabsTrigger value="event">On event</TabsTrigger>
        </GlassTabsRow>

        {draft.kind === `schedule` && (
          <ScheduleFields
            value={draft.schedule}
            onChange={(schedule) => set({ schedule })}
            errors={errors.filter(
              (error): error is TriggerDraftError & { field: `days` | `cron` } =>
                error.field === `days` || error.field === `cron`
            )}
          />
        )}

        {draft.kind === `event` && (
          <EventNamePicker value={draft.name} onChange={(name) => set({ name })} />
        )}
      </GlassGroup>
      <FieldError className="px-1">{fieldErrors[0] ?? null}</FieldError>
    </div>
  )
}

/** The event row: THE Picker over the contract builtins, its search field
 *  the free-text name (a typed name that is no builtin is offered as its
 *  own row). */
function EventNamePicker({
  value,
  onChange,
}: {
  value: string
  onChange: (name: string) => void
}) {
  const [query, setQuery] = useState(``)
  const typed = query.trim()
  const items = useMemo(() => {
    const names: string[] = [...builtinEventNames]
    if (value && !names.includes(value)) names.unshift(value)
    if (typed && !names.includes(typed)) names.unshift(typed)
    return names.map((name) => ({
      value: name,
      label: eventLabel(name),
      keywords: [name, eventLabel(name)],
      hint: eventLabel(name) === name ? undefined : name,
    }))
  }, [typed, value])
  return (
    <Picker
      mode="single"
      triggerVariant="row"
      mobileTitle="Event"
      triggerLabel="Pick an event"
      search
      searchPlaceholder="Event name"
      query={query}
      onQueryChange={setQuery}
      value={value || null}
      onChange={(name) => {
        onChange(name)
        setQuery(``)
      }}
      items={items}
      data-testid="trigger-event"
    />
  )
}

/** EXP-995: a launch pin (EXP-1293: an agent's Account row), keyed the way
 * every picker keys it (`accountOptionKey`): the pinned agent + its profile
 * id. */
export interface TriggerAccountPin {
  /** `` = no agent pinned yet (only while no device is bound). */
  agent: string
  /** `` = unpinned (stored NULL): the machine's LAST USED login for `agent`. */
  account: string
}

export function triggerAccountKey(pin: TriggerAccountPin): string {
  return accountOptionKey({
    agent: pin.agent as AccountOption[`agent`],
    id: pin.account,
  })
}

/** The pin an `AccountOption` stores: the agent and the profile id VERBATIM
 * (`` for an unpinned option). */
export function accountPinOf(option: AccountOption): TriggerAccountPin {
  return { agent: option.agent, account: option.id }
}

/** EXP-995: which option the Account row reads back for a stored pin — the
 * exact (agent, profile) pair, else that agent's first login (a profile the
 * machine no longer reports), else nothing. */
export function pickedAccountOption(
  options: readonly AccountOption[],
  pin: TriggerAccountPin
): AccountOption | undefined {
  if (pin.agent === ``) return undefined
  const key = triggerAccountKey(pin)
  return (
    options.find((option) => accountOptionKey(option) === key) ??
    options.find((option) => option.agent === pin.agent)
  )
}

/** EXP-995: the pin a bound machine seeds — the row's own when that machine
 * reports EXACTLY that login (a manual pick sticks), else that machine's
 * last used login for the same agent when it runs it (its first login of
 * that agent), else the machine's LAST USED login (which names the agent).
 * An unpinned row seeds the same way. Profile ids are device-LOCAL, so a device switch
 * always lands on a login the new machine reports: what the Account row
 * shows (`pickedAccountOption`) IS what Save stores. `undefined` = leave the
 * pin alone (no machine bound, or one reporting no login at all). */
export function seedAccountPin(
  device: SteerDevice | undefined,
  current: TriggerAccountPin
): TriggerAccountPin | undefined {
  if (!device) return undefined
  const options = accountOptionsOf(device)
  if (current.agent !== ``) {
    const key = triggerAccountKey(current)
    if (options.some((option) => accountOptionKey(option) === key)) {
      return undefined
    }
  }
  // Options run the agent's active (last used) login first within each
  // agent, so the agent's first row IS its last used login.
  const fallback =
    (current.agent !== ``
      ? options.find((option) => option.agent === current.agent)
      : undefined) ?? lastUsedAccountOption(options)
  return fallback ? accountPinOf(fallback) : undefined
}
