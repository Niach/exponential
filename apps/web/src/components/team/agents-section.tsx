import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import type {
  AgentTrigger,
  BoardIcon,
  TriggerWhen,
} from "@exp/db-schema/domain"
import {
  AccountPicker,
  BOARD_ICON_OPTIONS,
  BoardGlyph,
  Button,
  conceptIcon,
  DevicePicker,
  EmptyState,
  FEEDBACK_ERRORS,
  FEEDBACK_WORDS,
  FieldError,
  GlassGroup,
  GlassSectionHeader,
  ListRow,
  Menu,
  ModelPicker,
  Picker,
  Pill,
  Prompt,
  SETTINGS_LIST_CLASS,
  SkeletonRows,
  Switch,
  Textarea,
  type DevicePickerDevice,
} from "@exp/ui"
import type { Agent, Device, Event as EventRow, Team, User } from "@/db/schema"
import {
  agentCollection,
  deviceCollection,
  eventCollection,
  userCollection,
} from "@/lib/collections"
import { useSessionTitles, useTeamSessions } from "@/hooks/use-session-tree"
import { useTeamUsers } from "@/hooks/use-team-data"
import { compactRelativeTime } from "@/lib/relative-time"
import { displayUserName } from "@/lib/user-display"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { deleteTriggerPrompt, promptActions } from "@/lib/prompts"
import { healthBadgeLabel } from "@/lib/agent-usage"
import {
  accountOptionKey,
  lastUsedAccountOption,
} from "@/lib/accounts/account-option"
import {
  composeDeviceList,
  defaultDeviceId,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import {
  AGENT_SETTINGS_COPY,
  agentAccountValue,
  triggerSummary,
  validateAgentForm,
  type AgentFormError,
  type AgentRow,
} from "@/lib/session-tree"
import { useSession } from "@/hooks/use-session"
import { useNow } from "@/hooks/use-now"
import { BoardIdentityRow } from "@/components/board-form-fields"
import { TriggerDialog } from "@/components/trigger-dialog"
import {
  accountPinOf,
  seedAccountPin,
  triggerAccountKey,
  type TriggerAccountPin,
} from "@/components/trigger-fields"
import { accountOptionsOf } from "@/components/launch-dialog/use-launch-options"
import agentSettingsFixture from "@exp/domain-contract/fixtures/agent-settings.json"
import {
  ASSISTANT_COPY,
  ASSISTANT_SCHEDULES,
  assistantModelValue,
  assistantScheduleOf,
  assistantScheduleTriggers,
  runsOnDeviceIds,
  type AssistantScheduleId,
} from "@exp/domain-contract/agent-settings"
import { modelChoices } from "@/components/launch-dialog/launch-options-pane"
import { VoiceSettingsSection, type VoiceSettings } from "@/components/account/voice-settings-card"
import { AssistantMark } from "@/components/team/assistant-mark"

// EXP-1293 / NEW-2: Team settings → Agents (fixtures/agent-settings.json
// ×4) over the `agents` DEFINITION rows. Agents are managed exactly like
// boards: the settings nav lists every agent (icon + colour + name) then
// "New agent", and an agent's page is the board page's layout,
// `AGENT_SETTINGS_FIELDS` top to bottom — identity (icon + colour + name in
// ONE row), Device, Account, Triggers, Instructions, Delete. One component
// serves both an existing agent (every field writes `agents.update` as it
// changes, name and instructions on blur) and the create page (everything
// local until "Create agent" sends `agents.create`). Owner-only controls are
// hidden from members, who read the same page.

/** An `agents` row as the shape syncs it (instructions stay server-side). */
export type SyncedAgent = Omit<Agent, `instructions` | `triggerState`>

// The grouped form's borderless field row (EXP-694).
const GROUPED_FIELD_ROW = `rounded-none border-0 bg-transparent text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm px-4 py-3`

const COPY = AGENT_SETTINGS_COPY
/** The fixture's test ids: the settings nav's agent list + an agent page. */
export const AGENT_SETTINGS_TEST_IDS = agentSettingsFixture.testIds
const UiAddIcon = conceptIcon(`ui-add`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiEditIcon = conceptIcon(`ui-edit`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const SettingsAgentsIcon = conceptIcon(`settings-agents`)
const TriggerEventIcon = conceptIcon(`trigger-event`)

const DEFAULT_AGENT_COLOR = `#6366f1`
const DEFAULT_AGENT_ICON: BoardIcon = BOARD_ICON_OPTIONS[0].name
const EMPTY_PIN: TriggerAccountPin = { agent: ``, account: `` }

/** The device's logins, narrowed to one CLI when the agent has one. */
function cliAccountOptions(
  device: SteerDevice | undefined,
  cli: string | null
) {
  const all = accountOptionsOf(device)
  return cli ? all.filter((option) => option.agent === cli) : all
}

/** `seedAccountPin` for an agent locked to one CLI: keep a pin the machine
 *  still reports, else that CLI's last used login, else the CLI's default
 *  (no account). Never crosses to another CLI. */
function seedCliAccountPin(
  device: SteerDevice | undefined,
  cli: string,
  current: TriggerAccountPin
): TriggerAccountPin | undefined {
  if (!device) return undefined
  const options = cliAccountOptions(device, cli)
  const key = triggerAccountKey(current)
  if (
    current.agent === cli &&
    options.some((option) => accountOptionKey(option) === key)
  ) {
    return undefined
  }
  const fallback = lastUsedAccountOption(options)
  return fallback ? accountPinOf(fallback) : { agent: cli, account: `` }
}
const DELETE_TRIGGER = deleteTriggerPrompt()

/**
 * The machines an agent may be bound to: the caller's own devices plus the
 * servers teammates share into this team (the composer's list, off the
 * synced `devices` shape, ticking against the online window). null while the
 * first snapshot loads.
 */
export function useAgentDevices(teamId: string | undefined): SteerDevice[] | null {
  const { data: session } = useSession()
  const currentUserId = session?.user?.id
  const enabled = Boolean(teamId && currentUserId)
  const { data: deviceRows } = useLiveQuery(
    (query) => (enabled ? query.from({ d: deviceCollection }) : undefined),
    [enabled]
  )
  const { data: userRows } = useLiveQuery(
    (query) => (enabled ? query.from({ u: userCollection }) : undefined),
    [enabled]
  )
  const now = useNow(30_000)
  return useMemo(() => {
    if (!enabled || !currentUserId || deviceRows === undefined) return null
    const usersById = new Map(
      ((userRows ?? []) as User[]).map((user) => [user.id, user])
    )
    return composeDeviceList(
      deviceRows as Device[],
      usersById,
      now,
      currentUserId,
      teamId
    )
  }, [enabled, currentUserId, deviceRows, userRows, now, teamId])
}

/** The Device row's entries: online machines are pickable, an offline one
 *  stays listed (disabled) so a bound agent still names it, and a binding the
 *  viewer cannot see (a teammate's private machine) renders by its raw id.
 *  NEW-99: an Assistant's lists desktops only (`runsOnDeviceIds`). */
function deviceRows(
  all: readonly SteerDevice[],
  boundId: string | null,
  assistant: boolean
): DevicePickerDevice[] {
  const kept = new Set(runsOnDeviceIds(all, boundId, assistant))
  const devices = all.filter((device) => kept.has(device.deviceId))
  const rows: DevicePickerDevice[] = devices.map((device) => ({
    id: device.deviceId,
    name: device.deviceLabel || device.deviceId,
    icon: device.icon,
    kind: device.kind,
    ...(deviceIsOnline(device)
      ? {}
      : { description: `Offline`, disabled: device.deviceId !== boundId }),
  }))
  if (boundId && !all.some((device) => device.deviceId === boundId)) {
    rows.unshift({ id: boundId, name: boundId })
  }
  return rows
}

function errorFor(errors: readonly AgentFormError[], field: string) {
  return errors.find((error) => error.field === field)?.message ?? null
}

/** A trigger as stored: the when-part plus its id and switch. */
function withTriggerMeta(
  when: TriggerWhen,
  id: string,
  enabled: boolean
): AgentTrigger {
  return { ...when, id, enabled }
}

/** Tolerant read: the synced `triggers` jsonb may hold a shape a newer
 *  client wrote; only the two known kinds render. */
function readTriggers(value: unknown): AgentTrigger[] {
  if (!Array.isArray(value)) return []
  return value.filter(
    (trigger): trigger is AgentTrigger =>
      typeof trigger === `object` &&
      trigger !== null &&
      typeof (trigger as { id?: unknown }).id === `string` &&
      ((trigger as { kind?: unknown }).kind === `schedule` ||
        (trigger as { kind?: unknown }).kind === `event`)
  )
}

export function AgentSettingsPage({
  team,
  agent,
  isOwner,
}: {
  team: Team
  /** The live `agents` row; null = the create page. */
  agent: SyncedAgent | null
  isOwner: boolean
}) {
  const navigate = useNavigate()
  const creating = agent === null
  // S-3 (agent-settings.json `assistant`): the member's Assistant renders
  // `assistant.sections`: identity (name only, the brand mark), Runs on
  // (device, account, model), Check in (THE Picker over the presets), Voice
  // (the voice settings block); no Instructions (#EXP-1439: its prompt is
  // built in), no Delete. Only its owner
  // reaches it (the server answers NOT_FOUND to anyone else), so it is never
  // read-only.
  const assistant = agent?.role === `assistant`
  const readOnly = assistant ? false : !isOwner
  const devices = useAgentDevices(team.id)

  // ── identity ──
  // Name is the one deferred write (blur/Enter), like the board page; the
  // pickers write at once and show the pick until the synced row agrees.
  const [name, setName] = useState(``)
  const [icon, setIcon] = useState<BoardIcon>(DEFAULT_AGENT_ICON)
  const [color, setColor] = useState(DEFAULT_AGENT_COLOR)
  const [deviceId, setDeviceId] = useState<string | null>(null)
  const [pin, setPin] = useState<TriggerAccountPin>(EMPTY_PIN)
  const [model, setModel] = useState<string | null>(null)
  const [triggers, setTriggers] = useState<AgentTrigger[]>([])
  const [instructions, setInstructions] = useState(``)
  const [savedInstructions, setSavedInstructions] = useState(``)
  const [instructionsLoading, setInstructionsLoading] = useState(false)
  const [instructionsFailed, setInstructionsFailed] = useState(false)
  const [errors, setErrors] = useState<AgentFormError[]>([])
  const [creatingBusy, setCreatingBusy] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [triggerDialog, setTriggerDialog] = useState<{
    open: boolean
    editing: AgentTrigger | null
  }>({ open: false, editing: null })
  const [deleteTrigger, setDeleteTrigger] = useState<AgentTrigger | null>(null)

  // Seed when the AGENT changes (never on a resync of the same one: that
  // would stomp what is being typed).
  const agentId = agent?.id ?? null
  useEffect(() => {
    setName(agent?.name ?? ``)
    setIcon(((agent?.icon as BoardIcon | null) ?? DEFAULT_AGENT_ICON))
    setColor(agent?.color ?? DEFAULT_AGENT_COLOR)
    setDeviceId(agent?.deviceId ?? null)
    setPin({ agent: agent?.cli ?? ``, account: agent?.account ?? `` })
    setModel(agent?.model ?? null)
    setTriggers(readTriggers(agent?.triggers))
    setErrors([])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId])

  // Remote edits to the pickers and the trigger list land as they sync.
  const syncedIcon = agent?.icon
  const syncedColor = agent?.color
  const syncedDevice = agent?.deviceId
  const syncedTriggers = agent?.triggers
  useEffect(() => {
    if (syncedIcon) setIcon(syncedIcon as BoardIcon)
  }, [syncedIcon])
  useEffect(() => {
    if (syncedColor) setColor(syncedColor)
  }, [syncedColor])
  useEffect(() => {
    if (syncedDevice) setDeviceId(syncedDevice)
  }, [syncedDevice])
  useEffect(() => {
    if (!creating) setTriggers(readTriggers(syncedTriggers))
  }, [creating, syncedTriggers])

  // The create page lands on the caller's default machine, else the first
  // online one, once the list is in.
  useEffect(() => {
    if (!creating || deviceId || !devices) return
    const online = devices.filter(deviceIsOnline)
    const first = defaultDeviceId(online) ?? online[0]?.deviceId ?? null
    if (first) setDeviceId(first)
  }, [creating, deviceId, devices])

  const device = devices?.find((candidate) => candidate.deviceId === deviceId)

  // ── persistence (an existing agent) ──
  type AgentUpdate = Omit<Parameters<typeof trpc.agents.update.mutate>[0], `id`>
  const update = useCallback(
    async (patch: AgentUpdate, onFail?: () => void): Promise<void> => {
      if (!agentId) return
      try {
        await trpc.agents.update.mutate({ id: agentId, ...patch }, SKIP_ERROR_TOAST)
      } catch (err) {
        onFail?.()
        failedAction(`save`, `the agent`, err, {
          retry: () => void update(patch, onFail),
        })
        throw err
      }
    },
    [agentId]
  )
  const quietly = (promise: Promise<void>) => void promise.catch(() => {})

  // ── Account ──
  // An existing agent's CLI stays put (the page never re-picks it), so its
  // Account picker offers only that CLI's logins; no stored CLI = all.
  const lockedCli = creating ? null : (agent?.cli ?? null)
  // EXP-1306: only logins the machine reports (a profile id); the stand-in
  // row named after the agent never reads as an account.
  const accountOptions = useMemo(
    () => cliAccountOptions(device, lockedCli).filter((option) => option.id !== ``),
    [device, lockedCli]
  )
  // EXP-1306: the row reads the login exactly like the composer's chip (the
  // agent mark + its email), `agentAccountValue` ×4.
  const pickedAccount = agentAccountValue({
    options: accountOptions,
    account: pin.account || null,
    cli: pin.agent || lockedCli,
  })
  // A machine switch re-seeds the pin to a login THAT machine reports (the
  // trigger editor's EXP-995 rule): profile ids are device-local. The create
  // page seeds as its default machine lands; an existing agent re-seeds only
  // on a pick, and stores the new pin WITH the machine, so what the row
  // shows is what is stored.
  useEffect(() => {
    if (!creating) return
    const next = seedAccountPin(device, pin)
    if (next) setPin(next)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [device, creating])

  const pickDevice = (next: string) => {
    const previous = { deviceId, pin }
    setDeviceId(next)
    setErrors((current) => current.filter((error) => error.field !== `device`))
    if (creating) return
    const nextDevice = devices?.find((candidate) => candidate.deviceId === next)
    const nextPin = lockedCli
      ? seedCliAccountPin(nextDevice, lockedCli, pin)
      : seedAccountPin(nextDevice, pin)
    if (nextPin) setPin(nextPin)
    quietly(
      update(
        {
          deviceId: next,
          ...(nextPin ? { account: nextPin.account || null } : {}),
        },
        () => {
          setDeviceId(previous.deviceId)
          setPin(previous.pin)
        }
      )
    )
  }

  const pickAccount = (key: string) => {
    const option = accountOptions.find(
      (candidate) => accountOptionKey(candidate) === key
    )
    if (!option) return
    const previous = pin
    const next = accountPinOf(option)
    setPin(next)
    if (!creating) {
      quietly(update({ account: next.account || null }, () => setPin(previous)))
    }
  }

  // ── Model (the Assistant's Runs on) ──
  const modelCli = pin.agent || lockedCli || `claude`
  const modelOptions = useMemo(() => modelChoices(modelCli), [modelCli])
  const pickModel = (next: string) => {
    const previous = model
    setModel(next)
    quietly(update({ model: next }, () => setModel(previous)))
  }

  // ── Check in (the Assistant's ONE schedule preset) ──
  const schedule = assistantScheduleOf(triggers)
  const scheduleItems = ASSISTANT_SCHEDULES.map((preset) => ({ value: preset.id, label: preset.label }))
  const pickSchedule = (id: AssistantScheduleId) => {
    // A preset keeps the stored time zone (`scheduleCases`).
    const tz = triggers.find((trigger) => trigger.kind === `schedule` && `tz` in trigger)
    const zone = tz && `tz` in tz ? (tz.tz as string | undefined) : undefined
    const next = assistantScheduleTriggers(id, zone).map(
      (trigger) => ({ ...trigger, id: crypto.randomUUID() }) as AgentTrigger
    )
    quietly(writeTriggers(next))
  }

  // ── identity writes ──
  // The name draft is flushed on unmount / agent swap, like the board page:
  // pulling a focused input out of the DOM dispatches no blur.
  const draftRef = useRef<{ agentId: string; name: string } | null>(null)
  useEffect(() => {
    if (!agentId) return
    const seeded = agent?.name ?? ``
    return () => {
      const draft = draftRef.current
      if (!draft || draft.agentId !== agentId) return
      draftRef.current = null
      const trimmed = draft.name.trim()
      if (!trimmed || trimmed === seeded) return
      void trpc.agents.update
        .mutate({ id: agentId, name: trimmed }, SKIP_ERROR_TOAST)
        .catch(() => {})
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId])

  const changeName = (next: string) => {
    setName(next)
    setErrors((current) => current.filter((error) => error.field !== `identity`))
    if (agentId) draftRef.current = { agentId, name: next }
  }

  const saveName = () => {
    draftRef.current = null
    if (creating || !agent) return
    const trimmed = name.trim()
    const nameErrors = validateAgentForm({
      name,
      deviceId: deviceId ?? `bound`,
      instructions: ``,
    }).filter((error) => error.field === `identity`)
    if (nameErrors.length > 0) {
      setErrors(nameErrors)
      return
    }
    if (trimmed === agent.name) return
    quietly(update({ name: trimmed }))
  }

  const pickIcon = (next: BoardIcon) => {
    const previous = icon
    setIcon(next)
    if (!creating) quietly(update({ icon: next }, () => setIcon(previous)))
  }

  const pickColor = (next: string) => {
    const previous = color
    setColor(next)
    if (!creating) quietly(update({ color: next }, () => setColor(previous)))
  }

  // ── Triggers ──
  // The editor returns a when-part; the page owns the array. An existing
  // agent writes the whole array, the create page keeps it local.
  const writeTriggers = async (next: AgentTrigger[]) => {
    const previous = triggers
    setTriggers(next)
    if (!creating) await update({ triggers: next }, () => setTriggers(previous))
  }

  const saveTrigger = async (when: TriggerWhen) => {
    const editing = triggerDialog.editing
    const next = editing
      ? triggers.map((existing) =>
          existing.id === editing.id
            ? withTriggerMeta(when, existing.id, existing.enabled)
            : existing
        )
      : [...triggers, withTriggerMeta(when, crypto.randomUUID(), true)]
    await writeTriggers(next)
  }

  // ── Instructions (server-only: `agents.get`) ──
  const loadSeq = useRef(0)
  const loadInstructions = useCallback(
    (retried = false) => {
      if (!agentId || assistant) return
      const seq = ++loadSeq.current
      setInstructionsLoading(true)
      trpc.agents.get
        .query({ id: agentId })
        .then((res) => {
          if (seq !== loadSeq.current) return
          setInstructions(res.agent.instructions)
          setSavedInstructions(res.agent.instructions)
          setInstructionsFailed(false)
          setInstructionsLoading(false)
        })
        .catch((err: unknown) => {
          if (seq !== loadSeq.current) return
          setInstructionsFailed(true)
          setInstructionsLoading(false)
          // The load on open fails quietly (FEED-79); a Retry tap toasts.
          if (retried) {
            failedAction(`load`, `the instructions`, err, {
              retry: () => loadInstructions(true),
            })
          }
        })
    },
    [agentId, assistant]
  )
  useEffect(() => {
    setInstructions(``)
    setSavedInstructions(``)
    setInstructionsFailed(false)
    loadInstructions()
    return () => {
      loadSeq.current++
    }
  }, [loadInstructions])

  const saveInstructions = () => {
    if (creating || readOnly || instructions === savedInstructions) return
    const tooLong = validateAgentForm({
      name: `ok`,
      deviceId: `bound`,
      instructions,
    }).filter((error) => error.field === `instructions`)
    setErrors((current) => [
      ...current.filter((error) => error.field !== `instructions`),
      ...tooLong,
    ])
    if (tooLong.length > 0) return
    const written = instructions
    void update({ instructions: written })
      .then(() => setSavedInstructions(written))
      .catch(() => {})
  }

  // ── Create / Delete ──
  const create = async () => {
    const found = validateAgentForm({ name, deviceId, instructions })
    setErrors(found)
    if (found.length > 0 || !deviceId) return
    setCreatingBusy(true)
    try {
      const result = await trpc.agents.create.mutate(
        {
          teamId: team.id,
          name: name.trim(),
          icon,
          color,
          instructions,
          triggers,
          deviceId,
          ...(pin.agent ? { cli: pin.agent } : {}),
          ...(pin.account ? { account: pin.account } : {}),
        },
        SKIP_ERROR_TOAST
      )
      // Land on the agent's page once its row has synced (the page resolves
      // the id against the live shape).
      await agentCollection.utils.awaitTxId(result.txid).catch(() => {})
      void navigate({
        to: `/t/$teamSlug/settings/agents/$agentId`,
        params: { teamSlug: team.slug, agentId: result.agent.id },
        replace: true,
      })
    } catch (err) {
      failedAction(`create`, `the agent`, err, { retry: () => void create() })
    } finally {
      setCreatingBusy(false)
    }
  }

  const confirmDelete = async () => {
    if (!agentId) return
    setDeleting(true)
    try {
      await trpc.agents.delete.mutate({ id: agentId }, SKIP_ERROR_TOAST)
      draftRef.current = null
      setDeleteOpen(false)
      void navigate({
        to: `/t/$teamSlug/settings/agents`,
        params: { teamSlug: team.slug },
        replace: true,
      })
    } catch (err) {
      failedAction(`delete`, `the agent`, err)
    } finally {
      setDeleting(false)
    }
  }

  const identityError = errorFor(errors, `identity`)
  const deviceError = errorFor(errors, `device`)
  const instructionsError = errorFor(errors, `instructions`)

  const accountPickerOptions = accountOptions.map((option) => ({
    key: accountOptionKey(option),
    agent: option.agent,
    email: option.email,
    // EXP-849: an expired credential is the one thing worth knowing BEFORE a
    // session starts on it.
    hint: healthBadgeLabel(option.health) ?? undefined,
    limits: option.limits,
  }))

  const deviceRow = (
    <DevicePicker
      triggerVariant="row"
      mobileTitle="Device"
      value={deviceId}
      onChange={pickDevice}
      devices={deviceRows(devices ?? [], deviceId, assistant)}
      disabled={readOnly}
      emptyText="No devices online"
      data-testid="agent-device"
    />
  )
  const accountRow =
    accountPickerOptions.length > 0 ? (
      <AccountPicker
        variant="row"
        mobileTitle="Account"
        value={pickedAccount ? accountOptionKey(pickedAccount) : null}
        options={accountPickerOptions}
        onChange={pickAccount}
        disabled={readOnly}
        data-testid="agent-account"
      />
    ) : (
      <div className="flex items-center gap-3 px-4 py-3 text-sm">
        <span className="shrink-0 text-foreground">Account</span>
        <span className="ml-auto truncate text-foreground/50">
          {COPY.accountLastUsed}
        </span>
      </div>
    )

  return (
    <div className="space-y-4" data-testid={AGENT_SETTINGS_TEST_IDS.page}>
      <div className="space-y-2">
        <GlassGroup>
          <BoardIdentityRow
            name={name}
            onNameChange={changeName}
            onNameBlur={creating ? undefined : saveName}
            placeholder={assistant ? ASSISTANT_COPY.name : COPY.namePlaceholder}
            icon={icon}
            onIconChange={pickIcon}
            color={color}
            onColorChange={pickColor}
            disabled={readOnly}
            invalid={Boolean(identityError)}
            autoFocus={creating}
            lead={assistant ? <AssistantMark className="size-9" /> : undefined}
          />
          {!assistant && deviceRow}
        </GlassGroup>
        <FieldError className="px-1">{identityError ?? (assistant ? null : deviceError)}</FieldError>
      </div>

      {assistant && (
        <div className="space-y-2">
          <GlassSectionHeader label={ASSISTANT_COPY.runsOn} />
          <GlassGroup>
            {deviceRow}
            {accountRow}
            <ModelPicker
              triggerVariant="row"
              width="md"
              // agent-settings.json `modelValueCases`: unpinned = the
              // dispatcher root's default for the login's CLI.
              value={assistantModelValue({ model, cli: modelCli })}
              onChange={pickModel}
              models={modelOptions}
              data-testid="assistant-model"
            />
          </GlassGroup>
          <FieldError className="px-1">{deviceError}</FieldError>
        </div>
      )}

      {assistant && (
        <div>
          <GlassSectionHeader label={ASSISTANT_COPY.checkIn} />
          <GlassGroup>
            <Picker
              mode="single"
              triggerVariant="row"
              width="md"
              mobileTitle={ASSISTANT_COPY.checkIn}
              triggerLabel={ASSISTANT_COPY.checkIn}
              value={schedule}
              items={scheduleItems}
              onChange={pickSchedule}
              data-testid="assistant-check-in"
            />
          </GlassGroup>
        </div>
      )}

      {assistant && agentId && <AssistantVoiceSection assistantId={agentId} />}

      {!assistant && (
        <GlassGroup>
          {accountRow}
          <div className="flex items-center gap-3 px-4 py-3 text-sm">
            <span className="shrink-0 text-foreground">Triggers</span>
            <span className="ml-auto flex min-w-0 items-center gap-2">
              {triggers.length === 0 && (
                <span className="truncate text-foreground/50">{COPY.noSchedule}</span>
              )}
              {!readOnly && (
                <Pill
                  mode="action"
                  onClick={() => setTriggerDialog({ open: true, editing: null })}
                  data-testid="agent-add-trigger"
                >
                  <UiAddIcon className="size-3" />
                  {COPY.addTrigger}
                </Pill>
              )}
            </span>
          </div>
          {triggers.map((trigger) => (
            <AgentTriggerRow
              key={trigger.id}
              trigger={trigger}
              readOnly={readOnly}
              onToggle={(enabled) =>
                quietly(
                  writeTriggers(
                    triggers.map((existing) =>
                      existing.id === trigger.id ? { ...existing, enabled } : existing
                    )
                  )
                )
              }
              onEdit={() => setTriggerDialog({ open: true, editing: trigger })}
              onDelete={() => setDeleteTrigger(trigger)}
            />
          ))}
        </GlassGroup>
      )}

      {!assistant && (
        <div className="space-y-1">
          <GlassGroup>
            {instructionsLoading ? (
              <SkeletonRows dot={false} className="min-h-40 py-1" />
            ) : instructionsFailed ? (
              <div className="flex min-h-40 flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
                Instructions unavailable
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => loadInstructions(true)}
                >
                  {FEEDBACK_ERRORS.retry}
                </Button>
              </div>
            ) : (
              <Textarea
                id="agent-instructions"
                aria-label="Instructions"
                value={instructions}
                onChange={(e) => setInstructions(e.target.value)}
                onBlur={saveInstructions}
                placeholder={COPY.instructionsPlaceholder}
                readOnly={readOnly}
                rows={6}
                aria-invalid={Boolean(instructionsError) || undefined}
                className={`${GROUPED_FIELD_ROW} min-h-40 resize-y`}
                data-testid="agent-instructions"
              />
            )}
          </GlassGroup>
          <p className="px-1.5 text-xs text-muted-foreground">Instructions</p>
          <FieldError className="px-1">{instructionsError}</FieldError>
        </div>
      )}

      {creating
        ? isOwner && (
            <div className="flex justify-end pt-2">
              <Button
                onClick={() => void create()}
                pending={creatingBusy}
                pendingLabel={FEEDBACK_WORDS.pending.save}
                data-testid="agent-create"
              >
                Create agent
              </Button>
            </div>
          )
        : isOwner && !assistant && (
            <div className="flex flex-wrap gap-2 pt-2">
              <Button
                variant="destructive"
                size="sm"
                onClick={() => setDeleteOpen(true)}
                data-testid="agent-delete"
              >
                <UiDeleteIcon />
                Delete agent
              </Button>
            </div>
          )}

      {!readOnly && (
        <TriggerDialog
          open={triggerDialog.open}
          onOpenChange={(open) =>
            setTriggerDialog((current) =>
              open ? { ...current, open } : { open: false, editing: null }
            )
          }
          trigger={triggerDialog.editing}
          onSave={saveTrigger}
        />
      )}

      <Prompt
        open={deleteTrigger !== null}
        onOpenChange={(next) => {
          if (!next) setDeleteTrigger(null)
        }}
        title={DELETE_TRIGGER.title}
        body={DELETE_TRIGGER.body}
        actions={promptActions(DELETE_TRIGGER, {
          delete: {
            onSelect: async () => {
              const target = deleteTrigger
              if (!target) return
              try {
                await writeTriggers(
                  triggers.filter((existing) => existing.id !== target.id)
                )
                setDeleteTrigger(null)
              } catch {
                // Toast already shown; the confirm stays open for a retry.
              }
            },
          },
        })}
      />

      <Prompt
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        busy={deleting}
        title="Delete agent"
        body={COPY.deleteConfirm}
        actions={[
          { label: `Cancel`, role: `cancel` },
          {
            label: `Delete`,
            role: `destructive`,
            busy: deleting,
            onSelect: confirmDelete,
            testId: `agent-delete-confirm`,
          },
        ]}
      />
    </div>
  )
}

/** S-3: the Assistant page's Voice section, the voice settings block MOVED
 *  here (agent-settings.json `assistant.sections`), minus its Voice device
 *  and Agent rows. Loads the member's voice settings on open; a failed load
 *  offers Retry (feedback-states.json). */
function AssistantVoiceSection({ assistantId }: { assistantId: string }) {
  const [settings, setSettings] = useState<VoiceSettings | null>(null)
  const [failed, setFailed] = useState(false)
  const load = useCallback((retried = false) => {
    setFailed(false)
    trpc.voice.get
      .query()
      .then(setSettings)
      .catch((err: unknown) => {
        setFailed(true)
        if (retried) failedAction(`load`, `the voice settings`, err, { retry: () => load(true) })
      })
  }, [])
  useEffect(() => load(), [load])
  return (
    <div data-testid="assistant-voice">
      <GlassSectionHeader label={ASSISTANT_COPY.voice} />
      {settings ? (
        <VoiceSettingsSection settings={settings} assistantId={assistantId} />
      ) : failed ? (
        <GlassGroup>
          <div className="flex items-center gap-3 px-4 py-3 text-sm text-muted-foreground">
            <span className="min-w-0 flex-1">Voice settings unavailable</span>
            <Button type="button" variant="outline" size="sm" onClick={() => load(true)}>
              {FEEDBACK_ERRORS.retry}
            </Button>
          </div>
        </GlassGroup>
      ) : (
        <SkeletonRows dot={false} className="py-1" />
      )}
    </div>
  )
}

/** One Triggers row: the trigger's sentence, its switch, and (owners) a ⋯
 *  with Edit / Delete; the row itself opens the editor. */
function AgentTriggerRow({
  trigger,
  readOnly,
  onToggle,
  onEdit,
  onDelete,
}: {
  trigger: AgentTrigger
  readOnly: boolean
  onToggle: (enabled: boolean) => void
  onEdit: () => void
  onDelete: () => void
}) {
  const sentence = triggerSummary({ ...trigger, enabled: true })
  return (
    <ListRow
      interactive={!readOnly}
      onClick={readOnly ? undefined : onEdit}
      aria-label={readOnly ? undefined : sentence}
      className="px-4"
      data-testid={`agent-trigger-${trigger.id}`}
    >
      <span
        className={`min-w-0 flex-1 truncate text-sm ${trigger.enabled ? `` : `text-muted-foreground`}`}
      >
        {sentence}
      </span>
      <div
        className="flex shrink-0 items-center gap-1"
        // The switch and the menu are their own targets.
        onClick={(e) => e.stopPropagation()}
      >
        <Switch
          checked={trigger.enabled}
          disabled={readOnly}
          onCheckedChange={onToggle}
          aria-label={`Enabled: ${sentence}`}
        />
        {!readOnly && (
          <Menu
            align="end"
            aria-label={`${sentence} actions`}
            title={sentence}
            entries={[
              { kind: `item`, id: `edit`, label: `Edit`, icon: UiEditIcon, onSelect: onEdit },
              {
                kind: `item`,
                id: `delete`,
                label: `Delete`,
                icon: UiDeleteIcon,
                destructive: true,
                onSelect: onDelete,
              },
            ]}
            trigger={
              <Button variant="ghost" size="icon-sm" aria-label={`Trigger menu for ${sentence}`}>
                <UiMoreIcon />
              </Button>
            }
          />
        )}
      </div>
    </ListRow>
  )
}

/** The Agents band: "New agent" as its trailing pill (owners) over the
 *  team's agents as Settings rows (icon + colour + name, like boards), or
 *  the list-size empty state while there are none. */
export function AgentsListBand({
  team,
  isOwner,
  agents,
}: {
  team: Team
  isOwner: boolean
  agents: readonly AgentRow[]
}) {
  const navigate = useNavigate()
  return (
    <div>
      <GlassSectionHeader
        label={COPY.title}
        trailing={
          isOwner ? (
            <Pill
              mode="action"
              onClick={() =>
                void navigate({
                  to: `/t/$teamSlug/settings/agents/new`,
                  params: { teamSlug: team.slug },
                })
              }
            >
              <UiAddIcon className="size-3" />
              {COPY.newAgent}
            </Pill>
          ) : undefined
        }
      />
      {agents.length === 0 ? (
        <EmptyState icon={SettingsAgentsIcon} title="No agents yet" size="list" />
      ) : (
        <div className={SETTINGS_LIST_CLASS}>
          {agents.map((agent) => (
            <ListRow
              key={agent.agentId}
              interactive
              onClick={() =>
                // S-3: the viewer's Assistant (first) opens the Assistant page.
                void (agent.assistant
                  ? navigate({ to: `/t/$teamSlug/settings/assistant`, params: { teamSlug: team.slug } })
                  : navigate({
                      to: `/t/$teamSlug/settings/agents/$agentId`,
                      params: { teamSlug: team.slug, agentId: agent.agentId },
                    }))
              }
              className="gap-3 px-3 py-2 text-sm"
              data-testid={`agents-list-row-${agent.agentId}`}
            >
              {agent.assistant ? (
                <AssistantMark />
              ) : (
                <BoardGlyph board={{ icon: agent.icon, color: agent.color }} />
              )}
              <span className="min-w-0 flex-1 truncate font-medium">{agent.name}</span>
            </ListRow>
          ))}
        </div>
      )}
    </div>
  )
}

/** The newest events the Events band lists. */
export const AGENT_EVENTS_CAP = 50

/** Who emitted an event, as its row's caption: the session's title, the
 *  person's name, or the server. */
export function eventEmitterLabel(
  event: Pick<EventRow, `emittedBy` | `sessionId` | `userId`>,
  lookup: {
    sessionTitle: (sessionId: string) => string | null
    userName: (userId: string) => string
  }
): string {
  if (event.emittedBy === `session`) {
    return (event.sessionId && lookup.sessionTitle(event.sessionId)) || `Session`
  }
  if (event.emittedBy === `user`) return event.userId ? lookup.userName(event.userId) : `Someone`
  return `Exponential`
}

/** The Events band (NEW-2): the team's event stream newest first, capped at
 *  `AGENT_EVENTS_CAP`, as flat Settings rows (the trigger-event glyph, the
 *  name, who emitted it, a relative time). */
export function AgentEventsBand({ teamId }: { teamId: string }) {
  const { data, isReady } = useLiveQuery(
    (query) =>
      query
        .from({ events: eventCollection })
        .where(({ events }) => eq(events.teamId, teamId))
        .orderBy(({ events }) => events.createdAt, `desc`)
        .orderBy(({ events }) => events.id, `desc`)
        .limit(AGENT_EVENTS_CAP),
    [teamId]
  )
  const events = (data ?? []) as EventRow[]
  // The emitting sessions, titled by THE session title chain.
  const { rows: teamSessions } = useTeamSessions(teamId)
  const emitterIds = new Set(events.map((event) => event.sessionId).filter(Boolean))
  const emitters = useMemo(
    () => teamSessions.filter((row) => emitterIds.has(row.id)),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [teamSessions, [...emitterIds].sort().join(`,`)]
  )
  const { titles: sessionTitles } = useSessionTitles(emitters)
  const { userMap } = useTeamUsers(teamId)
  const now = useNow(60_000)
  return (
    <div data-testid="agent-events">
      <GlassSectionHeader label="Events" />
      {!isReady ? (
        <SkeletonRows className="py-1" />
      ) : events.length === 0 ? (
        <EmptyState icon={TriggerEventIcon} title="No events yet" size="list" />
      ) : (
        <div className={SETTINGS_LIST_CLASS}>
          {events.map((event) => (
            <ListRow
              key={event.id}
              className="gap-3 px-3 py-2 text-sm"
              data-testid={`agent-event-${event.id}`}
            >
              <TriggerEventIcon className="size-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 truncate font-medium">{event.name}</span>
              <span className="min-w-0 flex-1 truncate text-muted-foreground">
                {eventEmitterLabel(event, {
                  sessionTitle: (id) => sessionTitles.get(id) || null,
                  userName: (id) => displayUserName(userMap.get(id), id),
                })}
              </span>
              <span className="shrink-0 text-muted-foreground tabular-nums">
                {compactRelativeTime(event.createdAt, now.getTime())}
              </span>
            </ListRow>
          ))}
        </div>
      )}
    </div>
  )
}
