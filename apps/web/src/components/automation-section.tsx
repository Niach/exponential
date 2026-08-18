import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Check } from "lucide-react"
import {
  MAX_TRIGGER_FILTER_IDS,
  actionTriggerEventValues,
  type ActionEventTriggerFilters,
  type ActionScheduleInterval,
  type ActionScheduleTrigger,
  type ActionTrigger,
  type ActionTriggerEvent,
  type IssuePriority,
} from "@exp/db-schema/domain"
import { TRIGGER_EVENT_LABELS, weekdayName } from "@/lib/action-triggers"
import { issuePriorityOptions } from "@/lib/domain"
import {
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
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"

// The shared Automation editor section (EXP-530), used by BOTH the action
// editor dialog and the create-action dialog. The section is fully CONTROLLED
// via an `AutomationDraft` the parent holds and seeds in its open-reset
// effect (the same pattern as every other dialog field here) — a draft can
// represent states a valid `ActionTrigger` cannot (a kind picked but no
// capable device to bind), so the parent converts at submit time with
// `draftToTrigger` (incomplete → null = "no automation", which never loses an
// existing binding because a seeded deviceId is preserved even when the
// viewer cannot see that device's row).

export interface AutomationDraft {
  kind: `none` | `schedule` | `event`
  /** Steer deviceId of the bound machine; null = nothing bindable yet. */
  deviceId: string | null
  enabled: boolean
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
    kind: `none`,
    deviceId: null,
    enabled: true,
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

export function draftFromTrigger(trigger: ActionTrigger | null): AutomationDraft {
  const draft = emptyAutomationDraft()
  if (!trigger) return draft
  draft.kind = trigger.kind
  draft.deviceId = trigger.deviceId
  draft.enabled = trigger.enabled
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

/** The draft's trigger, or null when it is "none" OR incomplete (no device
 * bindable). Filters irrelevant to the picked event are dropped, matching the
 * server's strict write schema. */
export function draftToTrigger(draft: AutomationDraft): ActionTrigger | null {
  if (draft.kind === `none` || !draft.deviceId) return null
  if (draft.kind === `schedule`) {
    const trigger: ActionScheduleTrigger = {
      kind: `schedule`,
      deviceId: draft.deviceId,
      enabled: draft.enabled,
      interval: draft.interval,
      minuteOfDay: timeToMinute(draft.time),
    }
    if (draft.interval === `weekly`) trigger.weekday = draft.weekday
    if (draft.interval === `monthly`) trigger.dayOfMonth = draft.dayOfMonth
    return trigger
  }
  const clamp = <T,>(list: T[]) => list.slice(0, MAX_TRIGGER_FILTER_IDS)
  const filters: ActionEventTriggerFilters = {}
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
    deviceId: draft.deviceId,
    enabled: draft.enabled,
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

export function AutomationSection({
  draft,
  onChange,
  devices,
  teamId,
}: {
  draft: AutomationDraft
  onChange: (draft: AutomationDraft) => void
  devices: SteerDevice[]
  teamId: string
}) {
  const set = (patch: Partial<AutomationDraft>) =>
    onChange({ ...draft, ...patch })

  const capableDevices = useMemo(
    () => devices.filter(deviceCanRunAutomations),
    [devices]
  )

  const selectKind = (kind: string) => {
    if (kind === draft.kind) return
    const patch: Partial<AutomationDraft> = {
      kind: kind as AutomationDraft[`kind`],
    }
    // Picking a kind auto-binds the first capable device so a plain
    // "Schedule + save" is already complete.
    if (kind !== `none` && !draft.deviceId) {
      patch.deviceId = capableDevices[0]?.deviceId ?? null
    }
    set(patch)
  }

  return (
    <div className="space-y-3">
      <Label>Automation</Label>
      <Tabs value={draft.kind} onValueChange={selectKind}>
        <TabsList className="w-full">
          <TabsTrigger value="none" className="flex-1">
            None
          </TabsTrigger>
          <TabsTrigger value="schedule" className="flex-1">
            Schedule
          </TabsTrigger>
          <TabsTrigger value="event" className="flex-1">
            On event
          </TabsTrigger>
        </TabsList>
      </Tabs>

      {draft.kind === `schedule` && (
        <div className="space-y-2">
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
          <p className="text-xs text-muted-foreground">
            Runs on the selected device, in its local time. A run missed while
            the device was offline fires once when it comes back.
          </p>
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

      {draft.kind !== `none` && (
        <>
          <DevicePicker
            deviceId={draft.deviceId}
            devices={capableDevices}
            onChange={(deviceId) => set({ deviceId })}
          />
          <div className="flex items-center justify-between">
            <Label htmlFor="automation-enabled">Enabled</Label>
            <Switch
              id="automation-enabled"
              checked={draft.enabled}
              onCheckedChange={(enabled) => set({ enabled })}
            />
          </div>
        </>
      )}
    </div>
  )
}

function DevicePicker({
  deviceId,
  devices,
  onChange,
}: {
  deviceId: string | null
  /** Automation-capable devices only (offline-but-capable stays pickable). */
  devices: SteerDevice[]
  onChange: (deviceId: string) => void
}) {
  if (devices.length === 0 && !deviceId) {
    return (
      <div className="space-y-2">
        <Label>Device</Label>
        <p className="text-xs text-muted-foreground">
          No automation-capable device. Run the desktop app or the exponential
          daemon and it will appear here.
        </p>
      </div>
    )
  }
  // A trigger bound to a device the viewer cannot see (a teammate's private
  // machine) keeps its binding: the raw id renders as a fallback entry so
  // editing other fields never silently rebinds or drops it.
  const unknownDeviceId =
    deviceId && !devices.some((d) => d.deviceId === deviceId) ? deviceId : null
  return (
    <div className="space-y-2">
      <Label htmlFor="automation-device">Device</Label>
      <Select value={deviceId ?? undefined} onValueChange={onChange}>
        <SelectTrigger id="automation-device" className="w-full">
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
              {device.deviceLabel}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
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
