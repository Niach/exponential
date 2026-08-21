import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Check } from "lucide-react"
import {
  MAX_TRIGGER_FILTER_IDS,
  actionTriggerEventValues,
  type AutomationEventTriggerFilters,
  type ActionScheduleInterval,
  type AutomationScheduleTrigger,
  type AutomationTrigger,
  type ActionTriggerEvent,
  type IssuePriority,
} from "@exp/db-schema/domain"
import { TRIGGER_EVENT_LABELS, weekdayName } from "@/lib/action-triggers"
import { issuePriorityOptions } from "@/lib/domain"
import { agentEffortValues, agentModelValues } from "@/lib/coding-launch-prefs"
import {
  deviceAgentIds,
  deviceCanRunAutomations,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import {
  boardCollection,
  issueStatusCollection,
  labelCollection,
} from "@/lib/collections"
import { buildStatusOptions } from "@/lib/team-statuses"
import {
  AGENT_LABELS,
  effortLabel,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import type { Board, IssueStatusRow, Label as TeamLabel } from "@/db/schema"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"

// The reusable Automation editing PIECES (EXP-530, reshaped in EXP-583 when
// automations became their own rows): the trigger panes + event filters, the
// "Runs on" device picker, and the agent/model/effort picker. The automation
// dialog composes all three; the suggestion-prefilled create-action dialog
// composes the same three inside its "Automation" block. Everything is
// CONTROLLED — the parent holds an `AutomationDraft` (the when-part only,
// exactly what an `AutomationTrigger` carries) and seeds it in its open-reset
// effect, the same pattern as every other dialog field here.

export interface AutomationDraft {
  kind: `schedule` | `event`
  interval: ActionScheduleInterval
  /** `HH:MM` wall-clock string, straight off the time input. */
  time: string
  weekday: number
  dayOfMonth: number
  event: ActionTriggerEvent
  boardIds: string[]
  labelIds: string[]
  priorities: IssuePriority[]
  toStatusIds: string[]
}

export function emptyAutomationDraft(): AutomationDraft {
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
  trigger: AutomationTrigger | null
): AutomationDraft {
  const draft = emptyAutomationDraft()
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

/** The draft as a strict `AutomationTrigger`. Filters irrelevant to the
 * picked event are dropped, matching the server's write schema. */
export function draftToTrigger(draft: AutomationDraft): AutomationTrigger {
  if (draft.kind === `schedule`) {
    const trigger: AutomationScheduleTrigger = {
      kind: `schedule`,
      interval: draft.interval,
      minuteOfDay: timeToMinute(draft.time),
    }
    if (draft.interval === `weekly`) trigger.weekday = draft.weekday
    if (draft.interval === `monthly`) trigger.dayOfMonth = draft.dayOfMonth
    return trigger
  }
  const clamp = <T,>(list: T[]) => list.slice(0, MAX_TRIGGER_FILTER_IDS)
  const filters: AutomationEventTriggerFilters = {}
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

const INTERVAL_LABELS: Record<ActionScheduleInterval, string> = {
  daily: `Day`,
  weekly: `Week`,
  monthly: `Month`,
}

const MONTH_DAYS = Array.from({ length: 28 }, (_, index) => index + 1)
const WEEKDAYS = [1, 2, 3, 4, 5, 6, 7]

/** The when-part editor: Schedule | On event segmented control plus the
 * matching pane (interval/time, or event + filters). */
export function AutomationTriggerFields({
  draft,
  onChange,
  teamId,
}: {
  draft: AutomationDraft
  onChange: (draft: AutomationDraft) => void
  teamId: string
}) {
  const set = (patch: Partial<AutomationDraft>) =>
    onChange({ ...draft, ...patch })

  return (
    <div className="space-y-3">
      <Label>Trigger</Label>
      <Tabs
        value={draft.kind}
        onValueChange={(kind) => {
          if (kind === draft.kind) return
          set({ kind: kind as AutomationDraft[`kind`] })
        }}
      >
        <TabsList className="w-full">
          <TabsTrigger value="schedule" className="flex-1">
            Schedule
          </TabsTrigger>
          <TabsTrigger value="event" className="flex-1">
            On event
          </TabsTrigger>
        </TabsList>
      </Tabs>

      {draft.kind === `schedule` && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-muted-foreground">Every</span>
          <Select
            value={draft.interval}
            onValueChange={(value) =>
              set({ interval: value as ActionScheduleInterval })
            }
          >
            <SelectTrigger size="sm" className="w-24">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(Object.keys(INTERVAL_LABELS) as ActionScheduleInterval[]).map(
                (interval) => (
                  <SelectItem key={interval} value={interval}>
                    {INTERVAL_LABELS[interval]}
                  </SelectItem>
                )
              )}
            </SelectContent>
          </Select>
          {draft.interval === `weekly` && (
            <Select
              value={String(draft.weekday)}
              onValueChange={(value) => set({ weekday: Number(value) })}
            >
              <SelectTrigger size="sm" className="w-32">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {WEEKDAYS.map((weekday) => (
                  <SelectItem key={weekday} value={String(weekday)}>
                    {weekdayName(weekday)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          {draft.interval === `monthly` && (
            <Select
              value={String(draft.dayOfMonth)}
              onValueChange={(value) => set({ dayOfMonth: Number(value) })}
            >
              <SelectTrigger size="sm" className="w-24">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {MONTH_DAYS.map((day) => (
                  <SelectItem key={day} value={String(day)}>
                    {`Day ${day}`}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          <Input
            type="time"
            aria-label="Time of day"
            value={draft.time}
            onChange={(e) => set({ time: e.target.value })}
            className="h-8 w-28"
          />
        </div>
      )}

      {draft.kind === `event` && (
        <div className="space-y-2">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-sm text-muted-foreground">When</span>
            <Select
              value={draft.event}
              onValueChange={(value) =>
                set({ event: value as ActionTriggerEvent })
              }
            >
              <SelectTrigger size="sm" className="w-full max-w-56">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {actionTriggerEventValues.map((event) => (
                  <SelectItem key={event} value={event}>
                    {TRIGGER_EVENT_LABELS[event]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <EventFilterPickers draft={draft} set={set} teamId={teamId} />
        </div>
      )}
    </div>
  )
}

/** Automation-capable machines only (offline-but-capable stays pickable —
 * a schedule catches up when the machine comes back). */
export function automationDevices(devices: SteerDevice[]): SteerDevice[] {
  return devices.filter(deviceCanRunAutomations)
}

export function AutomationDevicePicker({
  deviceId,
  devices,
  onChange,
  id = `automation-device`,
}: {
  deviceId: string | null
  /** Automation-capable devices only (see `automationDevices`). */
  devices: SteerDevice[]
  onChange: (deviceId: string) => void
  id?: string
}) {
  if (devices.length === 0 && !deviceId) {
    return (
      <div className="space-y-2">
        <Label>Runs on</Label>
        <p className="text-xs text-muted-foreground">
          No automation-capable device. Run the desktop app or the exponential
          daemon and it will appear here.
        </p>
      </div>
    )
  }
  // An automation bound to a device the viewer cannot see (a teammate's
  // private machine) keeps its binding: the raw id renders as a fallback
  // entry so editing other fields never silently rebinds or drops it.
  const unknownDeviceId =
    deviceId && !devices.some((d) => d.deviceId === deviceId) ? deviceId : null
  return (
    <div className="space-y-2">
      <Label htmlFor={id}>Runs on</Label>
      <Select value={deviceId ?? undefined} onValueChange={onChange}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue placeholder="Select a device" />
        </SelectTrigger>
        <SelectContent>
          {unknownDeviceId && (
            <SelectItem value={unknownDeviceId}>
              <span className="text-muted-foreground">{unknownDeviceId}</span>
            </SelectItem>
          )}
          {devices.map((device) => (
            <SelectItem key={device.deviceId} value={device.deviceId}>
              <span
                aria-hidden
                className={`size-1.5 shrink-0 rounded-full ${
                  deviceIsOnline(device)
                    ? `bg-green-500`
                    : `bg-muted-foreground/40`
                }`}
              />
              {device.deviceLabel || device.deviceId}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

// Radix Select forbids an empty-string item value; the blank "Device default"
// agent/model/effort rides this sentinel inside the dialogs only.
export const DEVICE_DEFAULT = `device-default`

/** Agent + Model + Effort for an automated run. Blank on every select means
 * "whatever the device is configured to launch with" (the row stores NULL).
 * Agents are limited to what the bound device advertises; the model/effort
 * lists come from the contract per agent, exactly like the launch dialog. */
export function AutomationAgentFields({
  device,
  agent,
  onAgentChange,
  model,
  onModelChange,
  effort,
  onEffortChange,
  idPrefix = `automation`,
}: {
  /** The bound device — its advertisement bounds the agent list. */
  device: SteerDevice | undefined
  /** `` = device default. */
  agent: string
  onAgentChange: (agent: string) => void
  model: string
  onModelChange: (model: string) => void
  effort: string
  onEffortChange: (effort: string) => void
  idPrefix?: string
}) {
  // A pin the bound device doesn't advertise (an unseen teammate machine, or
  // an agent signed out since) stays listed so editing another field never
  // silently blanks the select.
  const availableAgents = useMemo(() => {
    const ids = device ? deviceAgentIds(device) : []
    return agent !== `` && !ids.includes(agent) ? [...ids, agent] : ids
  }, [device, agent])
  // A model or effort is only meaningful against a pinned agent (the server
  // validates the pair), so both stay locked on "Device default" until one is.
  const pinned = agent !== ``
  return (
    <div className="space-y-3">
      <div className="space-y-2">
        <Label htmlFor={`${idPrefix}-agent`}>Agent</Label>
        <Select
          value={agent === `` ? DEVICE_DEFAULT : agent}
          onValueChange={(value) =>
            onAgentChange(value === DEVICE_DEFAULT ? `` : value)
          }
        >
          <SelectTrigger id={`${idPrefix}-agent`} className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={DEVICE_DEFAULT}>Device default</SelectItem>
            {availableAgents.map((value) => (
              <SelectItem key={value} value={value}>
                {AGENT_LABELS[value] ?? value}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-model`}>Model</Label>
          <Select
            value={model === `` ? DEVICE_DEFAULT : model}
            onValueChange={(value) =>
              onModelChange(value === DEVICE_DEFAULT ? `` : value)
            }
            disabled={!pinned}
          >
            <SelectTrigger id={`${idPrefix}-model`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={DEVICE_DEFAULT}>Device default</SelectItem>
              {pinned &&
                agentModelValues(agent).map((value) => (
                  <SelectItem key={value} value={value}>
                    {modelLabel(value)}
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-2">
          <Label htmlFor={`${idPrefix}-effort`}>
            {agent === `pi`
              ? `Thinking`
              : agent === `codex`
                ? `Reasoning`
                : `Effort`}
          </Label>
          <Select
            value={effort === `` ? DEVICE_DEFAULT : effort}
            onValueChange={(value) =>
              onEffortChange(value === DEVICE_DEFAULT ? `` : value)
            }
            disabled={!pinned}
          >
            <SelectTrigger id={`${idPrefix}-effort`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={DEVICE_DEFAULT}>Device default</SelectItem>
              {pinned &&
                agentEffortValues(agent).map((value) => (
                  <SelectItem key={value} value={value}>
                    {effortLabel(value)}
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
        </div>
      </div>
    </div>
  )
}

/** Re-clamp a pinned model/effort to the agent they are paired with — the
 * server rejects a claude model on codex, so switching agent (or back to the
 * device default) drops anything that agent does not offer. */
export function clampAgentFields(
  agent: string,
  model: string,
  effort: string
): { model: string; effort: string } {
  if (agent === ``) return { model: ``, effort: `` }
  return {
    model: agentModelValues(agent).includes(model) ? model : ``,
    effort: agentEffortValues(agent).includes(effort) ? effort : ``,
  }
}

function EventFilterPickers({
  draft,
  set,
  teamId,
}: {
  draft: AutomationDraft
  set: (patch: Partial<AutomationDraft>) => void
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

  const boardOptions = useMemo(
    () =>
      [...((boardRows ?? []) as Board[])]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((board) => ({ id: board.id, name: board.name })),
    [boardRows]
  )
  const labelOptions = useMemo(
    () =>
      [...((labelRows ?? []) as TeamLabel[])]
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((label) => ({ id: label.id, name: label.name })),
    [labelRows]
  )
  // Duplicate is never a pickable target (same rule as the create/edit status
  // chips — it is only reachable via the duplicate-picker interception).
  const statusOptions = useMemo(
    () =>
      buildStatusOptions((statusRows ?? []) as IssueStatusRow[])
        .filter((option) => option.category !== `duplicate`)
        .map((option) => ({ id: option.id, name: option.name })),
    [statusRows]
  )
  const priorityOptions = useMemo(
    () =>
      issuePriorityOptions.map((option) => ({
        id: option.value,
        name: option.label,
      })),
    []
  )

  return (
    <div className="flex flex-wrap gap-2">
      <FilterMultiSelect
        anyLabel="Any board"
        noun="board"
        nounPlural="boards"
        options={boardOptions}
        selected={draft.boardIds}
        onChange={(boardIds) => set({ boardIds })}
      />
      {showLabels && (
        <FilterMultiSelect
          anyLabel="Any label"
          noun="label"
          nounPlural="labels"
          options={labelOptions}
          selected={draft.labelIds}
          onChange={(labelIds) => set({ labelIds })}
        />
      )}
      {showPriorities && (
        <FilterMultiSelect
          anyLabel="Any priority"
          noun="priority"
          nounPlural="priorities"
          options={priorityOptions}
          selected={draft.priorities}
          onChange={(priorities) =>
            set({ priorities: priorities as IssuePriority[] })
          }
        />
      )}
      {showStatuses && (
        <FilterMultiSelect
          anyLabel="Any status"
          noun="status"
          nounPlural="statuses"
          options={statusOptions}
          selected={draft.toStatusIds}
          onChange={(toStatusIds) => set({ toStatusIds })}
        />
      )}
    </div>
  )
}

// Compact multi-select chip (MobilePopover + Command, the board-picker
// pattern) — selections cap at MAX_TRIGGER_FILTER_IDS, matching the server's
// per-list limit.
function FilterMultiSelect({
  anyLabel,
  noun,
  nounPlural,
  options,
  selected,
  onChange,
}: {
  anyLabel: string
  noun: string
  nounPlural: string
  options: { id: string; name: string }[]
  selected: string[]
  onChange: (ids: string[]) => void
}) {
  const [open, setOpen] = useState(false)
  const atCap = selected.length >= MAX_TRIGGER_FILTER_IDS
  const toggle = (id: string) => {
    if (selected.includes(id)) {
      onChange(selected.filter((existing) => existing !== id))
    } else if (!atCap) {
      onChange([...selected, id])
    }
  }
  const summary =
    selected.length === 0
      ? anyLabel
      : selected.length === 1
        ? (options.find((option) => option.id === selected[0])?.name ??
          `1 ${noun}`)
        : `${selected.length} ${nounPlural}`
  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      <MobilePopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className={`h-8 font-normal ${selected.length === 0 ? `text-muted-foreground` : ``}`}
        >
          {summary}
        </Button>
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[14rem] p-0"
        align="start"
        mobileTitle={anyLabel}
      >
        <Command>
          <CommandInput placeholder={`Filter ${nounPlural}...`} />
          <CommandList>
            <CommandEmpty>{`No ${nounPlural} found.`}</CommandEmpty>
            <CommandGroup>
              {options.map((option) => {
                const isSelected = selected.includes(option.id)
                return (
                  <CommandItem
                    key={option.id}
                    value={`${option.name} ${option.id}`}
                    disabled={!isSelected && atCap}
                    onSelect={() => toggle(option.id)}
                    className="flex items-center gap-2"
                  >
                    <span className="min-w-0 truncate text-sm">
                      {option.name}
                    </span>
                    {isSelected && (
                      <Check className="ml-auto size-3.5 shrink-0" />
                    )}
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </CommandList>
        </Command>
      </MobilePopoverContent>
    </MobilePopover>
  )
}
