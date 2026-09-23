import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
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
import { AgentOptionsFields } from "@/components/launch-dialog/launch-options-pane"
import { accountOptionsOf } from "@/components/launch-dialog/use-launch-options"
import {
  accountOptionKey,
  defaultAccountOption,
  type AccountOption,
} from "@/lib/accounts/account-option"
import { healthBadgeLabel, SYSTEM_PROFILE_ID } from "@/lib/agent-usage"
import { deviceCanRunAutomations, type SteerDevice } from "@/lib/steer-devices"
import { contract } from "@exp/domain-contract"
import {
  boardCollection,
  issueStatusCollection,
  labelCollection,
} from "@/lib/collections"
import { buildStatusOptions } from "@/lib/team-statuses"
import type { Board, IssueStatusRow, Label as TeamLabel } from "@/db/schema"
import {
  AccountPicker,
  agentLabel,
  Combobox,
  Button,
  Label,
  type PickerOption,
  TabsTrigger,
  GLASS_SELECT_TRIGGER,
  GlassGroup,
  GlassInputRow,
  GlassTabsRow,
} from "@exp/ui"
import { cn } from "@/lib/utils"

// The reusable Automation editing PIECES (EXP-530, reshaped in EXP-583 when
// automations became their own rows): the trigger panes + event filters, the
// "Runs on" device picker, and the account/model/effort picker. The automation
// dialog composes all three (EXP-825: a suggestion seed no longer has an
// "Automation" block of its own — it rides the create-action request as the
// `formatAutomationBlock` text instead). Everything is CONTROLLED — the parent holds an `AutomationDraft` (the when-part only,
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
    // EXP-698 r4: ONE card, no "Trigger" caption above it — the Schedule /
    // On event strip IS the group's first row (the embedded segmented style
    // the agent card already uses), and the when-part below it is a list of
    // picker rows with the same geometry.
    <GlassGroup>
      <GlassTabsRow
        value={draft.kind}
        onValueChange={(kind) => {
          if (kind === draft.kind) return
          set({ kind: kind as AutomationDraft[`kind`] })
        }}
      >
        <TabsTrigger value="schedule">Schedule</TabsTrigger>
        <TabsTrigger value="event">On event</TabsTrigger>
      </GlassTabsRow>

      {draft.kind === `schedule` && (
        <>
          <Combobox
            triggerVariant="row"
            searchable={false}
            mobileTitle="Every"
            value={draft.interval}
            onChange={(value) => {
              if (value !== null) {
                set({ interval: value as ActionScheduleInterval })
              }
            }}
            options={(
              Object.keys(INTERVAL_LABELS) as ActionScheduleInterval[]
            ).map((interval) => ({
              value: interval,
              label: INTERVAL_LABELS[interval],
            }))}
          />
          {draft.interval === `weekly` && (
            <Combobox
              triggerVariant="row"
              searchable={false}
              mobileTitle="Weekday"
              value={String(draft.weekday)}
              onChange={(value) => {
                if (value !== null) set({ weekday: Number(value) })
              }}
              options={WEEKDAYS.map((weekday) => ({
                value: String(weekday),
                label: weekdayName(weekday),
              }))}
            />
          )}
          {draft.interval === `monthly` && (
            <Combobox
              triggerVariant="row"
              searchable={false}
              mobileTitle="Day of month"
              value={String(draft.dayOfMonth)}
              onChange={(value) => {
                if (value !== null) set({ dayOfMonth: Number(value) })
              }}
              options={MONTH_DAYS.map((day) => ({
                value: String(day),
                label: `Day ${day}`,
              }))}
            />
          )}
          <GlassInputRow
            id="automation-trigger-time"
            label="Time"
            type="time"
            inputClassName="ml-auto w-auto flex-none"
            value={draft.time}
            onChange={(e) => set({ time: e.target.value })}
          />
        </>
      )}

      {draft.kind === `event` && (
        <>
          <Combobox
            triggerVariant="row"
            searchable={false}
            mobileTitle="When"
            value={draft.event}
            onChange={(value) => {
              if (value !== null) set({ event: value as ActionTriggerEvent })
            }}
            options={actionTriggerEventValues.map((event) => ({
              value: event,
              label: TRIGGER_EVENT_LABELS[event],
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

/** Automation-capable machines only (offline-but-capable stays pickable —
 * a schedule catches up when the machine comes back). */
export function automationDevices(devices: SteerDevice[]): SteerDevice[] {
  return devices.filter(deviceCanRunAutomations)
}

export function AutomationDevicePicker({
  deviceId,
  devices,
  onChange,
}: {
  deviceId: string | null
  /** Automation-capable devices only (see `automationDevices`). */
  devices: SteerDevice[]
  onChange: (deviceId: string) => void
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
    // EXP-616: a grouped-form row — "Runs on" leads, the machine trails.
    <GlassGroup>
      <Combobox
        triggerVariant="row"
        searchable={false}
        mobileTitle="Runs on"
        value={deviceId}
        onChange={(value) => {
          if (value !== null) onChange(value)
        }}
        triggerLabel="Select a device"
        options={[
          ...(unknownDeviceId
            ? [
                {
                  value: unknownDeviceId,
                  label: (
                    <span className="text-muted-foreground">
                      {unknownDeviceId}
                    </span>
                  ),
                },
              ]
            : []),
          // EXP-615: no online dot here — every automation-capable machine is
          // equally bindable (a schedule catches up on reconnect), so the
          // live state belongs on the Automations tab's rows, not in the
          // picker.
          ...devices.map((device) => ({
            value: device.deviceId,
            label: device.deviceLabel || device.deviceId,
          })),
        ]}
      />
    </GlassGroup>
  )
}

/** EXP-995: the launch pin of an automated run, keyed the way every picker
 * keys it (`accountOptionKey`): the pinned agent + its profile, the ambient
 * login as `system`. */
export interface AutomationAccountPin {
  /** `` = no agent pinned yet (only while no device is bound). */
  agent: string
  /** `` = the machine's default login for `agent` (stored NULL). */
  account: string
}

export function automationAccountKey(pin: AutomationAccountPin): string {
  return accountOptionKey({
    agent: pin.agent as AccountOption[`agent`],
    id: pin.account === `` ? SYSTEM_PROFILE_ID : pin.account,
  })
}

/** The pin an `AccountOption` stores: the agent, and the profile id unless it
 * is the ambient `system` login (which stores as NULL). */
export function accountPinOf(option: AccountOption): AutomationAccountPin {
  return {
    agent: option.agent,
    account: option.id === SYSTEM_PROFILE_ID ? `` : option.id,
  }
}

/** EXP-995: which option the Account row reads back for a stored pin — the
 * exact (agent, profile) pair, else that agent's first login (a profile the
 * machine no longer reports), else nothing. */
export function pickedAccountOption(
  options: readonly AccountOption[],
  pin: AutomationAccountPin
): AccountOption | undefined {
  if (pin.agent === ``) return undefined
  const key = automationAccountKey(pin)
  return (
    options.find((option) => accountOptionKey(option) === key) ??
    options.find((option) => option.agent === pin.agent)
  )
}

/** EXP-995: the pin a bound machine seeds — the row's own when that machine
 * reports EXACTLY that login (a manual pick sticks), else that machine's
 * default account for the same agent when it runs it (its first login of
 * that agent, the device default first), else the machine's DEFAULT account
 * (which names the agent). Profile ids are device-LOCAL, so a device switch
 * always lands on a login the new machine reports: what the Account row
 * shows (`pickedAccountOption`) IS what Save stores. `undefined` = leave the
 * pin alone (no machine bound, or one reporting no login at all). */
export function seedAccountPin(
  device: SteerDevice | undefined,
  current: AutomationAccountPin
): AutomationAccountPin | undefined {
  if (!device) return undefined
  const options = accountOptionsOf(device)
  if (current.agent !== ``) {
    const key = automationAccountKey(current)
    if (options.some((option) => accountOptionKey(option) === key)) {
      return undefined
    }
  }
  // Options run device default first, so the agent's first row IS the
  // device default whenever that is the agent.
  const fallback =
    (current.agent !== ``
      ? options.find((option) => option.agent === current.agent)
      : undefined) ?? defaultAccountOption(options)
  return fallback ? accountPinOf(fallback) : undefined
}

/** Account + Model + Effort for an automated run. EXP-995: the agent strip
 * is gone — the FIRST row is THE account picker every launch surface shares
 * (`@exp/ui` `AccountPicker`: brand mark + email, the bound machine's default
 * first), and a pick implies the agent. Model/Effort below it are the launch
 * dialog's own cluster in its `automation` variant (EXP-615), fed a single
 * agent so it draws no strip. Blank on Model/Effort means "whatever the
 * device is configured to launch with" (the row stores NULL).
 *
 * A machine that reports no login at all offers one ambient row per runnable
 * agent, named by the agent (`accountOptionsOf`); a binding to a machine the
 * viewer cannot see (a teammate's private device) keeps its stored agent on a
 * plain Agent row, so editing another field never silently rebinds it. */
export function AutomationLaunchFields({
  device,
  pin,
  onPinChange,
  model,
  onModelChange,
  effort,
  onEffortChange,
  idPrefix = `automation`,
}: {
  /** The bound device — its logins are the Account rows. */
  device: SteerDevice | undefined
  pin: AutomationAccountPin
  onPinChange: (pin: AutomationAccountPin) => void
  model: string
  onModelChange: (model: string) => void
  effort: string
  onEffortChange: (effort: string) => void
  idPrefix?: string
}) {
  const options = useMemo(() => accountOptionsOf(device), [device])
  const pickerOptions = useMemo(
    () =>
      options.map((option) => ({
        key: accountOptionKey(option),
        agent: option.agent,
        email: option.email,
        // EXP-849: an expired credential is the one thing worth knowing
        // BEFORE the run starts on it.
        hint: healthBadgeLabel(option.health) ?? undefined,
        limits: option.limits,
      })),
    [options]
  )
  const picked = pickedAccountOption(options, pin)
  return (
    <div className="space-y-3">
      <GlassGroup>
        {pickerOptions.length > 0 ? (
          <AccountPicker
            variant="row"
            mobileTitle="Account"
            value={picked ? accountOptionKey(picked) : null}
            options={pickerOptions}
            onChange={(key) => {
              const option = options.find(
                (candidate) => accountOptionKey(candidate) === key
              )
              if (option) onPinChange(accountPinOf(option))
            }}
            data-testid={`${idPrefix}-account`}
          />
        ) : (
          <Combobox
            triggerVariant="row"
            searchable={false}
            mobileTitle="Agent"
            value={pin.agent === `` ? null : pin.agent}
            triggerLabel="Select an agent"
            options={contract.codingAgent.values.map((value) => ({
              value,
              label: agentLabel(value),
            }))}
            onChange={(value) => {
              if (value !== null) onPinChange({ agent: value, account: `` })
            }}
            data-testid={`${idPrefix}-agent`}
          />
        )}
      </GlassGroup>
      <AgentOptionsFields
        variant="automation"
        idPrefix={idPrefix}
        device={device}
        agent={pin.agent}
        availableAgents={pin.agent === `` ? [] : [pin.agent]}
        onAgentChange={(agent) => onPinChange({ agent, account: `` })}
        model={model}
        onModelChange={onModelChange}
        effortValue={effort}
        onEffortChange={onEffortChange}
      />
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
      <Combobox
        multiple
        max={MAX_TRIGGER_FILTER_IDS}
        options={filterOptions(boardOptions)}
        value={draft.boardIds}
        onChange={(boardIds) => set({ boardIds })}
        triggerVariant="field"
        mobileTitle="Any board"
        placeholder="Filter boards..."
        emptyText="No boards found."
        width="sm"
        renderTrigger={() =>
          filterTrigger(
            filterSummary(boardOptions, draft.boardIds, `Any board`, `board`, `boards`),
            draft.boardIds.length === 0
          )
        }
      />
      {showLabels && (
        <Combobox
          multiple
          max={MAX_TRIGGER_FILTER_IDS}
          options={filterOptions(labelOptions)}
          value={draft.labelIds}
          onChange={(labelIds) => set({ labelIds })}
          triggerVariant="field"
          mobileTitle="Any label"
          placeholder="Filter labels..."
          emptyText="No labels found."
          width="sm"
          renderTrigger={() =>
            filterTrigger(
              filterSummary(labelOptions, draft.labelIds, `Any label`, `label`, `labels`),
              draft.labelIds.length === 0
            )
          }
        />
      )}
      {showPriorities && (
        <Combobox
          multiple
          max={MAX_TRIGGER_FILTER_IDS}
          options={filterOptions(priorityOptions)}
          value={draft.priorities}
          onChange={(priorities) =>
            set({ priorities: priorities as IssuePriority[] })
          }
          triggerVariant="field"
          mobileTitle="Any priority"
          placeholder="Filter priorities..."
          emptyText="No priorities found."
          width="sm"
          renderTrigger={() =>
            filterTrigger(
              filterSummary(
                priorityOptions,
                draft.priorities,
                `Any priority`,
                `priority`,
                `priorities`
              ),
              draft.priorities.length === 0
            )
          }
        />
      )}
      {showStatuses && (
        <Combobox
          multiple
          max={MAX_TRIGGER_FILTER_IDS}
          options={filterOptions(statusOptions)}
          value={draft.toStatusIds}
          onChange={(toStatusIds) => set({ toStatusIds })}
          triggerVariant="field"
          mobileTitle="Any status"
          placeholder="Filter statuses..."
          emptyText="No statuses found."
          width="sm"
          renderTrigger={() =>
            filterTrigger(
              filterSummary(
                statusOptions,
                draft.toStatusIds,
                `Any status`,
                `status`,
                `statuses`
              ),
              draft.toStatusIds.length === 0
            )
          }
        />
      )}
    </div>
  )
}

// EXP-941: the popover+Command copy these four filters shared is now the
// shared `Combobox` (multi-select, capped at MAX_TRIGGER_FILTER_IDS to match
// the server's per-list limit). What stays local is the only thing the
// primitive can't know: the filter's own `3 boards` summary, and the compact
// chip that shows it.
type FilterRow = { id: string; name: string }

function filterOptions(rows: FilterRow[]): PickerOption[] {
  return rows.map((row) => ({ value: row.id, label: row.name }))
}

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
