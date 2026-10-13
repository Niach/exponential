import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  MAX_TRIGGER_FILTER_IDS,
  agentTriggerEventValues,
  type EventTriggerFilters,
  type AgentScheduleInterval,
  type ScheduleTriggerWhen,
  type TriggerWhen,
  type AgentTriggerEvent,
  type IssuePriority,
} from "@exp/db-schema/domain"
import { AGENT_SETTINGS_COPY } from "@/lib/session-tree"
import { issuePriorityOptions } from "@/lib/domain"
import { accountOptionsOf } from "@/components/launch-dialog/use-launch-options"
import {
  accountOptionKey,
  lastUsedAccountOption,
  type AccountOption,
} from "@/lib/accounts/account-option"
import type { SteerDevice } from "@/lib/steer-devices"
import {
  boardCollection,
  issueStatusCollection,
  labelCollection,
} from "@/lib/collections"
import { buildStatusOptions } from "@/lib/team-statuses"
import { toStatusPickerStatus } from "@/components/issue-properties/status-dropdown"
import type { Board, IssueStatusRow, Label as TeamLabel } from "@/db/schema"
import {
  BoardPicker,
  Picker,
  LabelPicker,
  PriorityPicker,
  StatusPicker,
  Button,
  TabsTrigger,
  GLASS_SELECT_TRIGGER,
  GlassGroup,
  GlassTabsRow,
} from "@exp/ui"
import { cn } from "@/lib/utils"

// The reusable trigger editing PIECES (EXP-530; EXP-1293: a trigger lives on
// its team AGENT, which owns the device and account, so only the WHEN is
// edited): the when-part panes + event filters, plus the account-pin helpers
// the agent page's Account row shares. Everything is CONTROLLED — the parent holds a `TriggerDraft`
// (the when-part only, exactly what an `TriggerWhen` carries) and seeds
// it in its open-reset effect, the same pattern as every other dialog field
// here.

export interface TriggerDraft {
  kind: `schedule` | `event`
  interval: AgentScheduleInterval
  /** `HH:MM` wall-clock string, straight off the time input. */
  time: string
  weekday: number
  dayOfMonth: number
  event: AgentTriggerEvent
  boardIds: string[]
  labelIds: string[]
  priorities: IssuePriority[]
  toStatusIds: string[]
}

export function emptyTriggerDraft(): TriggerDraft {
  return {
    kind: `schedule`,
    interval: `daily`,
    time: `09:00`,
    weekday: 1,
    dayOfMonth: 1,
    event: `created`,
    boardIds: [],
    labelIds: [],
    priorities: [],
    toStatusIds: [],
  }
}

function minuteToTime(minuteOfDay: number): string {
  const hours = String(Math.floor(minuteOfDay / 60)).padStart(2, `0`)
  const minutes = String(minuteOfDay % 60).padStart(2, `0`)
  return `${hours}:${minutes}`
}

function timeToMinute(time: string): number {
  const match = /^(\d{1,2}):(\d{2})$/.exec(time)
  if (!match) return 540
  const minute = Number(match[1]) * 60 + Number(match[2])
  return minute >= 0 && minute <= 1439 ? minute : 540
}

export function draftFromTrigger(
  trigger: TriggerWhen | null
): TriggerDraft {
  const draft = emptyTriggerDraft()
  if (!trigger) return draft
  draft.kind = trigger.kind
  if (trigger.kind === `schedule`) {
    draft.interval = trigger.interval
    draft.time = minuteToTime(trigger.minuteOfDay)
    if (trigger.weekday !== undefined) draft.weekday = trigger.weekday
    if (trigger.dayOfMonth !== undefined) draft.dayOfMonth = trigger.dayOfMonth
  } else {
    draft.event = trigger.event
    draft.boardIds = trigger.filters?.boardIds ?? []
    draft.labelIds = trigger.filters?.labelIds ?? []
    draft.priorities = trigger.filters?.priorities ?? []
    draft.toStatusIds = trigger.filters?.toStatusIds ?? []
  }
  return draft
}

/** The draft as a strict `TriggerWhen`. Filters irrelevant to the
 * picked event are dropped, matching the server's write schema. */
export function draftToTrigger(draft: TriggerDraft): TriggerWhen {
  if (draft.kind === `schedule`) {
    const trigger: ScheduleTriggerWhen = {
      kind: `schedule`,
      interval: draft.interval,
      minuteOfDay: timeToMinute(draft.time),
    }
    if (draft.interval === `weekly`) trigger.weekday = draft.weekday
    if (draft.interval === `monthly`) trigger.dayOfMonth = draft.dayOfMonth
    return trigger
  }
  const clamp = <T,>(list: T[]) => list.slice(0, MAX_TRIGGER_FILTER_IDS)
  const filters: EventTriggerFilters = {}
  if (draft.boardIds.length > 0) filters.boardIds = clamp(draft.boardIds)
  if (draft.event === `label_added` && draft.labelIds.length > 0) {
    filters.labelIds = clamp(draft.labelIds)
  }
  if (
    (draft.event === `created` || draft.event === `priority_changed`) &&
    draft.priorities.length > 0
  ) {
    filters.priorities = clamp(draft.priorities)
  }
  if (draft.event === `status_changed` && draft.toStatusIds.length > 0) {
    filters.toStatusIds = clamp(draft.toStatusIds)
  }
  return {
    kind: `event`,
    event: draft.event,
    ...(Object.keys(filters).length > 0 ? { filters } : {}),
  }
}

const INTERVAL_LABELS: Record<AgentScheduleInterval, string> = {
  daily: `Day`,
  weekly: `Week`,
  monthly: `Month`,
}

const MONTH_DAYS = Array.from({ length: 28 }, (_, index) => index + 1)
const WEEKDAYS = [1, 2, 3, 4, 5, 6, 7]

/** The when-part editor: Schedule | On event segmented control plus the
 * matching pane (interval/time, or event + filters). */
export function TriggerWhenFields({
  draft,
  onChange,
  teamId,
}: {
  draft: TriggerDraft
  onChange: (draft: TriggerDraft) => void
  teamId: string
}) {
  const set = (patch: Partial<TriggerDraft>) =>
    onChange({ ...draft, ...patch })

  return (
    // EXP-698 r4: ONE card, no "Trigger" caption above it — the Schedule /
    // On event strip IS the group's first row (the embedded segmented style
    // the agent card already uses), and the when-part below it is a list of
    // picker rows with the same geometry.
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
        <>
          <Picker
            mode="single"
            triggerVariant="row"
            mobileTitle="Every"
            value={draft.interval}
            onChange={(value) => {
              if (value !== null) {
                set({ interval: value as AgentScheduleInterval })
              }
            }}
            items={(
              Object.keys(INTERVAL_LABELS) as AgentScheduleInterval[]
            ).map((interval) => ({
              value: interval,
              label: INTERVAL_LABELS[interval],
            }))}
          />
          {draft.interval === `weekly` && (
            <Picker
              mode="single"
              triggerVariant="row"
              mobileTitle="Weekday"
              value={String(draft.weekday)}
              onChange={(value) => {
                if (value !== null) set({ weekday: Number(value) })
              }}
              items={WEEKDAYS.map((weekday) => ({
                value: String(weekday),
                label: AGENT_SETTINGS_COPY.weekdays[weekday - 1] ?? ``,
              }))}
            />
          )}
          {draft.interval === `monthly` && (
            <Picker
              mode="single"
              triggerVariant="row"
              mobileTitle="Day of month"
              value={String(draft.dayOfMonth)}
              onChange={(value) => {
                if (value !== null) set({ dayOfMonth: Number(value) })
              }}
              items={MONTH_DAYS.map((day) => ({
                value: String(day),
                label: `Day ${day}`,
              }))}
            />
          )}
          <TriggerTimeRow
            time={draft.time}
            onChange={(time) => set({ time })}
          />
        </>
      )}

      {draft.kind === `event` && (
        <>
          <Picker
            mode="single"
            triggerVariant="row"
            mobileTitle="When"
            value={draft.event}
            onChange={(value) => {
              if (value !== null) set({ event: value as AgentTriggerEvent })
            }}
            items={agentTriggerEventValues.map((event) => ({
              value: event,
              label: (AGENT_SETTINGS_COPY.events as Record<string, string>)[event] ?? event,
            }))}
          />
          <div className="px-4 py-3">
            <EventFilterPickers draft={draft} set={set} teamId={teamId} />
          </div>
        </>
      )}
    </GlassGroup>
  )
}

const pad2 = (value: number) => String(value).padStart(2, `0`)
const HOUR_ITEMS = Array.from({ length: 24 }, (_, hour) => ({
  value: pad2(hour),
  label: pad2(hour),
}))
const MINUTE_ITEMS = Array.from({ length: 60 }, (_, minute) => ({
  value: pad2(minute),
  label: pad2(minute),
}))

/** The Time row ×4: the 24h `HH:mm` value, its hour and its minute each a
 *  Picker with the natives' choices (every hour, every minute) — never the
 *  locale-dependent browser time control. */
function TriggerTimeRow({
  time,
  onChange,
}: {
  time: string
  onChange: (time: string) => void
}) {
  const [hour = `09`, minute = `00`] = time.split(`:`)
  return (
    <div
      className="flex w-full items-center gap-3 px-4 py-3 text-sm"
      data-testid="trigger-time"
    >
      <span className="shrink-0 text-foreground">Time</span>
      <span className="ml-auto flex items-center gap-0.5 text-foreground/70 tabular-nums">
        <Picker
          mode="single"
          triggerVariant="inline"
          mobileTitle="Hour"
          width="sm"
          value={pad2(Number(hour) || 0)}
          items={HOUR_ITEMS}
          onChange={(value) => {
            if (value !== null) onChange(`${value}:${pad2(Number(minute) || 0)}`)
          }}
        />
        <span aria-hidden>:</span>
        <Picker
          mode="single"
          triggerVariant="inline"
          mobileTitle="Minute"
          width="sm"
          value={pad2(Number(minute) || 0)}
          items={MINUTE_ITEMS}
          onChange={(value) => {
            if (value !== null) onChange(`${pad2(Number(hour) || 0)}:${value}`)
          }}
        />
      </span>
    </div>
  )
}

/** EXP-995: a launch pin (EXP-1293: an agent's Account row), keyed the way
 * every picker keys it (`accountOptionKey`): the pinned agent + its profile
 * id. */
export interface TriggerAccountPin {
  /** `` = no agent pinned yet (only while no device is bound). */
  agent: string
  /** `` = unpinned (stored NULL): the machine's LAST USED login for `agent`.
   *  The server folds a legacy `system` to unpinned too. */
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

function EventFilterPickers({
  draft,
  set,
  teamId,
}: {
  draft: TriggerDraft
  set: (patch: Partial<TriggerDraft>) => void
  teamId: string
}) {
  const { data: boardRows } = useLiveQuery(
    (q) =>
      q
        .from({ boards: boardCollection })
        .where(({ boards }) => eq(boards.teamId, teamId)),
    [teamId]
  )
  const showLabels = draft.event === `label_added`
  const showPriorities =
    draft.event === `created` || draft.event === `priority_changed`
  const showStatuses = draft.event === `status_changed`

  const { data: labelRows } = useLiveQuery(
    (q) =>
      showLabels
        ? q
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.teamId, teamId))
        : undefined,
    [teamId, showLabels]
  )
  const { data: statusRows } = useLiveQuery(
    (q) =>
      showStatuses
        ? q
            .from({ issueStatuses: issueStatusCollection })
            .where(({ issueStatuses }) => eq(issueStatuses.teamId, teamId))
        : undefined,
    [teamId, showStatuses]
  )

  // EXP-1021: the filter rows are the typed pickers' rows — a board keeps its
  // icon and colour, a label its dot, a status its glyph — so a filter list
  // reads exactly like the list it filters.
  const boardOptions = useMemo(
    () =>
      [...((boardRows ?? []) as Board[])]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((board) => ({
          id: board.id,
          name: board.name,
          icon: board.icon,
          color: board.color,
          repositoryId: board.repositoryId,
        })),
    [boardRows]
  )
  const labelOptions = useMemo(
    () =>
      [...((labelRows ?? []) as TeamLabel[])]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((label) => ({ id: label.id, name: label.name, color: label.color })),
    [labelRows]
  )
  // Duplicate is never a pickable target (same rule as the create/edit status
  // chips — it is only reachable via the duplicate-picker interception).
  const statusOptions = useMemo(
    () =>
      buildStatusOptions((statusRows ?? []) as IssueStatusRow[])
        .filter((option) => option.category !== `duplicate`)
        .map((option) => toStatusPickerStatus(option)),
    [statusRows]
  )
  const priorityOptions = useMemo(
    () =>
      issuePriorityOptions.map((option) => ({
        ...option,
        id: option.value,
        name: option.label,
      })),
    []
  )

  return (
    <div className="flex flex-wrap gap-2">
      <BoardPicker
        mode="multi"
        max={MAX_TRIGGER_FILTER_IDS}
        boards={boardOptions}
        value={draft.boardIds}
        onChange={(boardIds) => set({ boardIds })}
        mobileTitle="Any board"
        searchPlaceholder="Filter boards..."
        emptyText="No boards found."
        width="sm"
        trigger={filterTrigger(
          filterSummary(boardOptions, draft.boardIds, `Any board`, `board`, `boards`),
          draft.boardIds.length === 0
        )}
      />
      {showLabels && (
        <LabelPicker
          max={MAX_TRIGGER_FILTER_IDS}
          labels={labelOptions}
          value={draft.labelIds}
          onChange={(labelIds) => set({ labelIds })}
          mobileTitle="Any label"
          searchPlaceholder="Filter labels..."
          emptyText="No labels found."
          width="sm"
          trigger={filterTrigger(
            filterSummary(labelOptions, draft.labelIds, `Any label`, `label`, `labels`),
            draft.labelIds.length === 0
          )}
        />
      )}
      {showPriorities && (
        <PriorityPicker
          mode="multi"
          max={MAX_TRIGGER_FILTER_IDS}
          options={priorityOptions}
          value={draft.priorities}
          onChange={(priorities) =>
            set({ priorities: priorities as IssuePriority[] })
          }
          mobileTitle="Any priority"
          width="sm"
          trigger={filterTrigger(
            filterSummary(
              priorityOptions,
              draft.priorities,
              `Any priority`,
              `priority`,
              `priorities`
            ),
            draft.priorities.length === 0
          )}
        />
      )}
      {showStatuses && (
        <StatusPicker
          mode="multi"
          max={MAX_TRIGGER_FILTER_IDS}
          statuses={statusOptions}
          value={draft.toStatusIds}
          onChange={(toStatusIds) => set({ toStatusIds })}
          mobileTitle="Any status"
          search
          searchPlaceholder="Filter statuses..."
          emptyText="No statuses found."
          width="sm"
          trigger={filterTrigger(
            filterSummary(
              statusOptions,
              draft.toStatusIds,
              `Any status`,
              `status`,
              `statuses`
            ),
            draft.toStatusIds.length === 0
          )}
        />
      )}
    </div>
  )
}

// EXP-941/EXP-1021: the popover+Command copy these four filters shared is now
// the TYPED picker of each domain in its multi arm, capped at
// MAX_TRIGGER_FILTER_IDS to match the server's per-list limit. What stays
// local is the only thing a picker can't know: the filter's own `3 boards`
// summary, and the compact chip that shows it.
type FilterRow = { id: string; name: string }

function filterSummary(
  rows: FilterRow[],
  selected: string[],
  anyLabel: string,
  noun: string,
  nounPlural: string
): string {
  if (selected.length === 0) return anyLabel
  if (selected.length === 1) {
    return rows.find((row) => row.id === selected[0])?.name ?? `1 ${noun}`
  }
  return `${selected.length} ${nounPlural}`
}

function filterTrigger(summary: string, empty: boolean) {
  return (
    <Button
      variant="outline"
      size="sm"
      className={cn(
        `h-8 font-normal`,
        GLASS_SELECT_TRIGGER,
        empty && `text-muted-foreground`
      )}
    >
      {summary}
    </Button>
  )
}
