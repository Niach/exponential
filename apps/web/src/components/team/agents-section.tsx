import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import type {
  AgentTrigger,
  BoardIcon,
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
  GlassInputRow,
  GlassSectionHeader,
  ListRow,
  ModelPicker,
  Picker,
  PickerTrigger,
  Pill,
  Prompt,
  SETTINGS_LIST_CLASS,
  SkeletonRows,
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
import { useApps } from "@/hooks/use-apps"
import { sidebarAgents } from "@exp/domain-contract/chats"
import { useSessionTitles, useTeamSessions } from "@/hooks/use-session-tree"
import { useTeamUsers } from "@/hooks/use-team-data"
import { compactRelativeTime } from "@/lib/relative-time"
import { displayUserName } from "@/lib/user-display"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
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
  validateAgentForm,
  type AgentFormError,
  type AgentRow,
} from "@/lib/session-tree"
import { useSession } from "@/hooks/use-session"
import { useNow } from "@/hooks/use-now"
import { BoardIdentityRow } from "@/components/board-form-fields"
import { APPS_AGENT_SLOT } from "@exp/domain-contract/apps"
import { AgentTriggers, readTriggers, WRITTEN_TRIGGER_KINDS } from "@/components/agents/agent-triggers"
import {
  accountPinOf,
  seedAccountPin,
  triggerAccountKey,
  type TriggerAccountPin,
} from "@/components/trigger-fields"
import { accountOptionsOf } from "@/components/launch-dialog/use-launch-options"
import agentSettingsFixture from "@exp/domain-contract/fixtures/agent-settings.json"
import {
  AGENT_VOICE,
  AGENT_VOICE_COPY,
  agentVoiceOf,
  ASSISTANT_COPY,
  LEAD,
  LEAD_COPY,
  LEAD_NONE,
  leadOptions,
  assistantModelValue,
  runsOnDeviceIds,
} from "@exp/domain-contract/agent-settings"
import { modelChoices } from "@/components/launch-dialog/launch-options-pane"
import { VoiceSettingsSection, type VoiceSettings } from "@/components/account/voice-settings-card"
import { AssistantMark } from "@/components/team/assistant-mark"
import { languageLabel, VOICE_LANGUAGES, voiceTextInput, wakePhraseError } from "@exp/etalk/voice"
import { skillPickerItems } from "@/components/launch-dialog/skill-picker"
import { useSkills } from "@/hooks/use-skills"
import { agentSkillsValue, SKILLS } from "@exp/domain-contract/skills"
import { McpServerPicker } from "@/components/launch-dialog/mcp-server-picker"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { AssistantTeamPicks } from "@/components/team/assistant-team-picks"

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

/** #EXP-1849: the slot picker's `Each member's own device` entry. */
const EACH_MEMBER = `__each-member__`

function errorFor(errors: readonly AgentFormError[], field: string) {
  return errors.find((error) => error.field === field)?.message ?? null
}

export function AgentSettingsPage({
  team,
  agent,
  isOwner,
  appSlot = false,
}: {
  team: Team
  /** The live `agents` row; null = the create page. */
  agent: SyncedAgent | null
  isOwner: boolean
  /** #EXP-1849 (apps.json `agentSlot`): an app's Settings mounts ONLY the
   *  app agent's Runs on rows as its Agent group: the device is the SLOT
   *  (`Each member's own device` or a team-shared device), account and
   *  model only with a device. */
  appSlot?: boolean
}) {
  const navigate = useNavigate()
  const creating = agent === null
  // #EXP-1846 (agent-settings.json `page`): ONE page for the Assistant and
  // every team agent: identity, Runs on (device, account, model, skills, MCP
  // servers), Triggers (`AgentTriggers`: Check in + the list), then Voice
  // (#EXP-1847: every agent), Instructions + Delete (team agents only; #EXP-1439:
  // the Assistant's prompt is built in). Only its owner reaches an Assistant
  // (the server answers NOT_FOUND to anyone else), so it is never read-only.
  const assistant = agent?.role === `assistant`
  const readOnly = assistant ? false : !isOwner
  const devices = useAgentDevices(team.id)
  // #EXP-1847: the member's voice settings, every agent's voice fallback
  // (`agentVoiceOf`), read once per page.
  const memberVoice = useMemberVoice(!creating)

  // ── identity ──
  // Name is the one deferred write (blur/Enter), like the board page; the
  // pickers write at once and show the pick until the synced row agrees.
  const [name, setName] = useState(``)
  const [icon, setIcon] = useState<BoardIcon>(DEFAULT_AGENT_ICON)
  const [color, setColor] = useState(DEFAULT_AGENT_COLOR)
  const [deviceId, setDeviceId] = useState<string | null>(null)
  // #EXP-1659 (skills.json `agentRow`): the skills every run of the agent
  // starts with; a pick writes at once (`agents.update {skillIds}`).
  const [skillIds, setSkillIds] = useState<string[]>([])
  const { skills } = useSkills(team.id)
  // #EXP-1793 (agent-settings.json `mcpRow`): the MCP servers every run of
  // the agent starts with; a pick writes at once (`agents.update
  // {mcpServerIds}`).
  const [mcpServerIds, setMcpServerIds] = useState<string[]>([])
  const { servers: mcpServers } = useMcpServers(team.id)
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

  // Seed when the AGENT changes (never on a resync of the same one: that
  // would stomp what is being typed).
  const agentId = agent?.id ?? null
  useEffect(() => {
    setName(agent?.name ?? ``)
    setIcon(((agent?.icon as BoardIcon | null) ?? DEFAULT_AGENT_ICON))
    setColor(agent?.color ?? DEFAULT_AGENT_COLOR)
    setDeviceId(agent?.defaults?.deviceId ?? null)
    setSkillIds(agent?.skillIds ?? [])
    setMcpServerIds(agent?.mcpServerIds ?? [])
    setPin({ agent: agent?.defaults?.cli ?? ``, account: agent?.defaults?.account ?? `` })
    setModel(agent?.defaults?.model ?? null)
    setTriggers(readTriggers(agent?.triggers))
    setErrors([])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId])

  // Remote edits to the pickers and the trigger list land as they sync.
  const syncedIcon = agent?.icon
  const syncedColor = agent?.color
  const syncedDevice = agent?.defaults?.deviceId
  const syncedTriggers = agent?.triggers
  useEffect(() => {
    if (syncedIcon) setIcon(syncedIcon as BoardIcon)
  }, [syncedIcon])
  useEffect(() => {
    if (syncedColor) setColor(syncedColor)
  }, [syncedColor])
  useEffect(() => {
    if (syncedDevice || appSlot) setDeviceId(syncedDevice ?? null)
  }, [syncedDevice, appSlot])
  const syncedSkillIds = agent?.skillIds
  useEffect(() => {
    if (syncedSkillIds) setSkillIds(syncedSkillIds)
  }, [syncedSkillIds])
  const syncedMcpServerIds = agent?.mcpServerIds
  useEffect(() => {
    if (syncedMcpServerIds) setMcpServerIds(syncedMcpServerIds)
  }, [syncedMcpServerIds])
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
  const lockedCli = creating ? null : (agent?.defaults?.cli ?? null)
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
    // #EXP-1849: `` empties an app agent's slot (each member's own device).
    setDeviceId(next || null)
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

  // ── Model (every agent's Runs on, #EXP-1846) ──
  const modelCli = pin.agent || lockedCli || `claude`
  const modelOptions = useMemo(() => modelChoices(modelCli), [modelCli])
  const pickModel = (next: string) => {
    const previous = model
    setModel(next)
    if (!creating) quietly(update({ model: next }, () => setModel(previous)))
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

  // ── Triggers (`AgentTriggers` edits, the page owns the array) ──
  // An existing agent writes the whole array, the create page keeps it local.
  const writeTriggers = async (next: AgentTrigger[]) => {
    const previous = triggers
    setTriggers(next)
    if (!creating) await update({ triggers: next, triggerKinds: WRITTEN_TRIGGER_KINDS }, () => setTriggers(previous))
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
          ...(skillIds.length > 0 ? { skillIds } : {}),
          ...(mcpServerIds.length > 0 ? { mcpServerIds } : {}),
          ...(pin.agent ? { cli: pin.agent } : {}),
          ...(pin.account ? { account: pin.account } : {}),
          // A pinned model the login's CLI still offers.
          ...(model && modelOptions.some((option) => option.value === model) ? { model } : {}),
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

  // A pick naming a deleted skill drops it from the count and the rows.
  // Names in list order (skills.json `agentValueCases`).
  const pickedSkills = skills.filter((skill) => skillIds.includes(skill.id))
  const pickedSkillIds = pickedSkills.map((skill) => skill.id)
  const pickSkills = (next: readonly string[]) => {
    const previous = skillIds
    setSkillIds([...next])
    if (creating) return
    quietly(update({ skillIds: [...next] }, () => setSkillIds(previous)))
  }
  const skillsRow = (
    <Picker
      mode="multi"
      width="md"
      search
      mobileTitle={SKILLS.agentRow.label}
      value={pickedSkillIds}
      items={skillPickerItems(skills)}
      onChange={pickSkills}
      disabled={readOnly}
      emptyText={SKILLS.list.empty.title}
      trigger={
        <PickerTrigger
          variant="row"
          label={SKILLS.agentRow.label}
          value={
            pickedSkills.length > 0
              ? agentSkillsValue(pickedSkills.map((skill) => skill.name))
              : undefined
          }
          placeholder={SKILLS.agentRow.none}
          className={pickedSkills.length === 0 ? `[&>span:nth-child(2)]:text-foreground/50` : undefined}
          disabled={readOnly}
        />
      }
      data-testid="agent-skills"
    />
  )

  // #EXP-1793 (`mcpRow`): THE composer's MCP servers multi picker (an
  // unconnected server opens Settings › MCP servers to connect), names in
  // the team list's order; shown while the team has a server or the agent
  // already names one.
  const teamMcpServers = mcpServers ?? []
  const pickedMcpServerIds = teamMcpServers
    .filter((server) => mcpServerIds.includes(server.id))
    .map((server) => server.id)
  const toggleMcpServer = (id: string) => {
    const previous = mcpServerIds
    const next = pickedMcpServerIds.includes(id)
      ? pickedMcpServerIds.filter((current) => current !== id)
      : [...pickedMcpServerIds, id]
    setMcpServerIds(next)
    if (creating) return
    quietly(update({ mcpServerIds: next }, () => setMcpServerIds(previous)))
  }
  const mcpConnectHref = (serverId: string) =>
    `/t/${encodeURIComponent(team.slug)}/settings/mcp?connect=${encodeURIComponent(serverId)}`
  const mcpRow =
    mcpServers !== null && (mcpServers.length > 0 || mcpServerIds.length > 0) ? (
      <McpServerPicker
        servers={teamMcpServers}
        selectedIds={pickedMcpServerIds}
        onToggle={toggleMcpServer}
        connectHref={mcpConnectHref}
        disabled={readOnly}
        testId="agent-mcp-servers"
      />
    ) : null

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

  if (appSlot) {
    const slotRows: DevicePickerDevice[] = [
      { id: EACH_MEMBER, name: APPS_AGENT_SLOT.copy.eachMember },
      ...deviceRows(devices ?? [], deviceId, false).filter((row) =>
        (devices ?? []).some((d) => d.deviceId === row.id && (d.sharedTeamIds ?? []).includes(team.id))
      ),
    ]
    return (
      <div data-testid={APPS_AGENT_SLOT.testId}>
        <GlassSectionHeader label={APPS_AGENT_SLOT.copy.title} />
        <GlassGroup>
          <DevicePicker
            triggerVariant="row"
            mobileTitle="Device"
            value={deviceId || EACH_MEMBER}
            onChange={(next) => pickDevice(next === EACH_MEMBER ? `` : next)}
            devices={slotRows}
            disabled={readOnly}
            emptyText="No devices online"
            data-testid="app-agent-slot-device"
          />
          {deviceId ? accountRow : null}
          {deviceId ? (
            <ModelPicker
              triggerVariant="row"
              width="md"
              value={assistantModelValue({ model, cli: modelCli })}
              onChange={pickModel}
              models={modelOptions}
              disabled={readOnly}
              data-testid="agent-model"
            />
          ) : null}
          {skillsRow}
          {mcpRow}
        </GlassGroup>
      </div>
    )
  }

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
        </GlassGroup>
        <FieldError className="px-1">{identityError}</FieldError>
      </div>

      <div className="space-y-2">
        <GlassSectionHeader label={COPY.runsOn} />
        <GlassGroup>
          {deviceRow}
          {accountRow}
          <ModelPicker
            triggerVariant="row"
            width="md"
            // agent-settings.json `modelValueCases`: unpinned = the
            // device default for the login's CLI.
            value={assistantModelValue({ model, cli: modelCli })}
            onChange={pickModel}
            models={modelOptions}
            disabled={readOnly}
            data-testid="agent-model"
          />
          {/* #EXP-1988: the Assistant's skills / MCP servers live in one section per team. */}
          {assistant ? null : skillsRow}
          {assistant ? null : mcpRow}
        </GlassGroup>
        <FieldError className="px-1">{deviceError}</FieldError>
      </div>

      {assistant && agent ? <AssistantTeamPicks pageAgent={agent} /> : null}

      <AgentTriggers triggers={triggers} readOnly={readOnly} onWrite={writeTriggers} />

      {agent && (
        <AgentVoiceSection agent={agent} member={memberVoice} readOnly={readOnly}>
          {assistant && memberVoice.settings && (
            <VoiceSettingsSection settings={memberVoice.settings} assistantId={agent.id} memberRowsOnly />
          )}
        </AgentVoiceSection>
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
                size="xs"
                onClick={() => setDeleteOpen(true)}
                data-testid="agent-delete"
              >
                <UiDeleteIcon />
                Delete agent
              </Button>
            </div>
          )}

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

/** #EXP-1847: the member's voice settings (`voice.get`), the fallback of
 *  every agent's voice. A failed load offers Retry (feedback-states.json). */
function useMemberVoice(enabled: boolean) {
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
  useEffect(() => {
    if (enabled) load()
  }, [enabled, load])
  return { settings, failed, retry: () => load(true) }
}

type MemberVoice = ReturnType<typeof useMemberVoice>

const AGENT_VOICE_TEST_IDS = {
  section: `agent-voice`,
  language: `agent-voice-language`,
  clip: `agent-voice-clip`,
  clipRemove: `agent-voice-clip-remove`,
  transcript: `agent-voice-transcript`,
  wakePhrase: `agent-voice-wake-phrase`,
} as const

const agentVoiceClipPath = (agentId: string) =>
  AGENT_VOICE.clipUpload.route.replace(`{agentId}`, encodeURIComponent(agentId))

/** The route's `{error}` (the fixture's copy), else a plain failure. */
async function clipRequest(agentId: string, init: RequestInit) {
  const response = await fetch(agentVoiceClipPath(agentId), { ...init, credentials: `same-origin` })
  if (!response.ok) {
    const result = (await response.json().catch(() => null)) as { error?: string } | null
    throw new Error(result?.error ?? `Upload failed`)
  }
}

/** #EXP-1847 (agent-settings.json `voice`): every agent's Voice section,
 *  right after Triggers: Language, Voice (the uploaded clip; Built-in in the row's usual value colour), Transcript,
 *  Wake phrase over `agentVoiceOf(agent, member)`. The Assistant's member
 *  rows (push to talk, wake word, meetings) follow as `children`. */
function AgentVoiceSection({
  agent,
  member,
  readOnly,
  children,
}: {
  agent: SyncedAgent
  member: MemberVoice
  readOnly: boolean
  children?: React.ReactNode
}) {
  const settings = member.settings
  return (
    <div className="space-y-2" data-testid={AGENT_VOICE_TEST_IDS.section}>
      <GlassSectionHeader label={AGENT_VOICE_COPY.label} />
      {settings ? (
        <>
          <AgentVoiceRows agent={agent} member={settings} readOnly={readOnly} />
          {children}
        </>
      ) : member.failed ? (
        <GlassGroup>
          <div className="flex items-center gap-3 px-4 py-3 text-sm text-muted-foreground">
            <span className="min-w-0 flex-1">Voice settings unavailable</span>
            <Button type="button" variant="outline" size="sm" onClick={member.retry}>
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

type VoiceTextField = `transcript` | `wakePhrase`

function AgentVoiceRows({
  agent,
  member,
  readOnly,
}: {
  agent: SyncedAgent
  member: VoiceSettings
  readOnly: boolean
}) {
  const voice = agent.voice ?? {}
  // A just-picked language / uploaded clip shows until the synced row agrees.
  const [pickedLanguage, setPickedLanguage] = useState<string | null>(null)
  const [clip, setClip] = useState<{ clipId: string; clipName: string } | null | undefined>(undefined)
  const [clipBusy, setClipBusy] = useState(false)
  useEffect(() => setPickedLanguage(null), [voice.language])
  useEffect(() => setClip(undefined), [voice.clipId, voice.clipName])

  const shown = {
    ...voice,
    ...(pickedLanguage ? { language: pickedLanguage as typeof voice.language } : {}),
    ...(clip === undefined ? {} : clip === null ? { clipId: undefined, clipName: undefined } : clip),
  }
  const resolved = agentVoiceOf({ role: agent.role, name: agent.name, voice: shown }, member)

  // The text rows: typed locally, saved on blur / Enter.
  const [text, setText] = useState<Record<VoiceTextField, string>>({
    transcript: resolved.transcript,
    wakePhrase: resolved.wakePhrase,
  })
  const [wakePhraseInvalid, setWakePhraseInvalid] = useState<string | null>(null)
  useEffect(() => setText((current) => ({ ...current, transcript: resolved.transcript })), [resolved.transcript])
  useEffect(() => setText((current) => ({ ...current, wakePhrase: resolved.wakePhrase })), [resolved.wakePhrase])

  type VoicePatch = NonNullable<Parameters<typeof trpc.agents.update.mutate>[0][`voice`]>
  const update = (patch: VoicePatch, revert: () => void) =>
    trpc.agents.update.mutate({ id: agent.id, voice: patch }, SKIP_ERROR_TOAST).catch((err: unknown) => {
      revert()
      failedAction(`save`, `the agent's voice`, err)
    })

  const pickLanguage = (language: string | null) => {
    if (language === null || language === resolved.language) return
    setPickedLanguage(language)
    void update({ language: language as VoicePatch[`language`] }, () => setPickedLanguage(null))
  }

  const limits: Record<VoiceTextField, number> = {
    transcript: AGENT_VOICE.limits.transcript,
    wakePhrase: AGENT_VOICE.limits.wakePhrase,
  }
  const editText = (field: VoiceTextField, raw: string) => {
    setText((current) => ({ ...current, [field]: voiceTextInput(raw, limits[field]) }))
    if (field === `wakePhrase`) setWakePhraseInvalid(null)
  }
  const saveText = (field: VoiceTextField) => {
    const trimmed = text[field].trim()
    if (field === `wakePhrase`) {
      const invalid = wakePhraseError(trimmed)
      if (invalid) {
        setWakePhraseInvalid(invalid)
        return
      }
    }
    if (trimmed === resolved[field].trim()) return
    const previous = resolved[field]
    void update({ [field]: trimmed === `` ? null : trimmed }, () =>
      setText((current) => ({ ...current, [field]: previous }))
    )
  }
  const enterSaves = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === `Enter`) event.currentTarget.blur()
  }

  // The clip: a press opens the file picker; uploads go through the route.
  const fileInput = useRef<HTMLInputElement | null>(null)
  const uploadClip = async (file: File) => {
    setClipBusy(true)
    try {
      const body = new FormData()
      body.append(`file`, file)
      const response = await fetch(agentVoiceClipPath(agent.id), {
        method: `POST`,
        body,
        credentials: `same-origin`,
      })
      const result = (await response.json().catch(() => null)) as
        | { clipId?: string; clipName?: string; error?: string }
        | null
      if (!response.ok || !result?.clipId) throw new Error(result?.error ?? `Upload failed`)
      setClip({ clipId: result.clipId, clipName: result.clipName ?? file.name })
    } catch (err) {
      failedAction(`upload`, `the voice clip`, err, { retry: () => void uploadClip(file) })
    } finally {
      setClipBusy(false)
    }
  }
  const removeClip = async () => {
    setClipBusy(true)
    try {
      await clipRequest(agent.id, { method: `DELETE` })
      setClip(null)
    } catch (err) {
      failedAction(`remove`, `the voice clip`, err, { retry: () => void removeClip() })
    } finally {
      setClipBusy(false)
    }
  }
  const ownClip = Boolean(shown.clipId)

  const languageItems = VOICE_LANGUAGES.map((language) => ({
    value: language.value,
    label: languageLabel(language.value, null),
  }))

  return (
    <>
      <GlassGroup>
        <Picker
          mode="single"
          triggerVariant="row"
          triggerLabel={AGENT_VOICE_COPY.language}
          width="md"
          mobileTitle={AGENT_VOICE_COPY.language}
          data-testid={AGENT_VOICE_TEST_IDS.language}
          value={resolved.language}
          items={languageItems}
          onChange={pickLanguage}
          disabled={readOnly}
        />
        <div className="flex items-center">
          <input
            ref={fileInput}
            type="file"
            hidden
            accept={[`.wav`, ...AGENT_VOICE.clipUpload.accept].join(`,`)}
            data-testid={`${AGENT_VOICE_TEST_IDS.clip}-input`}
            onChange={(event) => {
              const file = event.target.files?.[0]
              event.target.value = ``
              if (file) void uploadClip(file)
            }}
          />
          <PickerTrigger
            variant="row"
            label={AGENT_VOICE_COPY.clip}
            value={resolved.clipLabel}
            title={AGENT_VOICE_COPY.clipUpload}
            className="min-w-0 flex-1"
            disabled={readOnly || clipBusy}
            data-testid={AGENT_VOICE_TEST_IDS.clip}
            onClick={() => fileInput.current?.click()}
          />
          {ownClip && !readOnly && (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              className="mr-3 size-7 shrink-0 text-muted-foreground"
              disabled={clipBusy}
              aria-label={AGENT_VOICE_COPY.clipRemove}
              title={AGENT_VOICE_COPY.clipRemove}
              data-testid={AGENT_VOICE_TEST_IDS.clipRemove}
              onClick={() => void removeClip()}
            >
              <UiDeleteIcon className="size-4" />
            </Button>
          )}
        </div>
        <GlassInputRow
          id={AGENT_VOICE_TEST_IDS.transcript}
          data-testid={AGENT_VOICE_TEST_IDS.transcript}
          label={AGENT_VOICE_COPY.transcript}
          placeholder={AGENT_VOICE_COPY.transcriptPlaceholder}
          value={text.transcript}
          readOnly={readOnly}
          onChange={(event) => editText(`transcript`, event.target.value)}
          onBlur={() => !readOnly && saveText(`transcript`)}
          onKeyDown={enterSaves}
        />
        <GlassInputRow
          id={AGENT_VOICE_TEST_IDS.wakePhrase}
          data-testid={AGENT_VOICE_TEST_IDS.wakePhrase}
          label={AGENT_VOICE_COPY.wakePhrase}
          placeholder={AGENT_VOICE_COPY.wakePhrasePlaceholder.replace(`{agent}`, agent.name)}
          aria-invalid={wakePhraseInvalid !== null || undefined}
          aria-describedby={wakePhraseInvalid === null ? undefined : `${AGENT_VOICE_TEST_IDS.wakePhrase}-error`}
          value={text.wakePhrase}
          readOnly={readOnly}
          onChange={(event) => editText(`wakePhrase`, event.target.value)}
          onBlur={() => !readOnly && saveText(`wakePhrase`)}
          onKeyDown={enterSaves}
        />
      </GlassGroup>
      <FieldError id={`${AGENT_VOICE_TEST_IDS.wakePhrase}-error`} className="px-1">
        {wakePhraseInvalid}
      </FieldError>
    </>
  )
}

/** The Agents band: "New agent" as its trailing pill (owners) over the
 *  team's agents as Settings rows (icon + colour + name, like boards), or
 *  the list-size empty state while there are none. #FEED-239
 *  (`sidebarAgents`): an app's agent is listed under Apps only. */
export function AgentsListBand({
  team,
  isOwner,
  agents: allAgents,
  loading = false,
}: {
  team: Team
  isOwner: boolean
  agents: readonly AgentRow[]
  /** The `agents` shape has not synced yet: skeleton rows, not the empty state. */
  loading?: boolean
}) {
  const navigate = useNavigate()
  const { apps } = useApps(team.id)
  const agents = useMemo(
    () => sidebarAgents(allAgents, apps.map((app) => ({ agentId: app.row?.agentId ?? null }))),
    [allAgents, apps]
  )
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
      {agents.length === 0 && loading ? (
        <SkeletonRows className="py-1" />
      ) : agents.length === 0 ? (
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

/** #EXP-1735 (agent-settings.json `lead`): Team settings › Agents' Lead
 *  Assistant row where the Coordinator row was (the lead replaces the
 *  coordinator), THE settings Picker row: None or one member, whose
 *  Assistant every member's Assistant reports to; owners pick, members read. */
export function LeadAssistantRow({ team, isOwner }: { team: Team; isOwner: boolean }) {
  const { users } = useTeamUsers(team.id)
  const [busy, setBusy] = useState(false)
  // `undefined` = not read yet (the row stays disabled, #EXP-1788 like the
  // IDE's agents_index), `null` = no lead.
  const [leadUserId, setLeadUserId] = useState<string | null | undefined>(undefined)
  // The lead row is another member's Assistant (never in my synced agents):
  // its member comes from the server, re-read whenever the lead changes.
  const leadAssistantId = team.leadAssistantId ?? null
  useEffect(() => {
    if (!leadAssistantId) {
      setLeadUserId(null)
      return
    }
    setLeadUserId(undefined)
    let live = true
    // A failed read keeps the row unknown (disabled), never a false None.
    trpc.agents.lead
      .query({ teamId: team.id })
      .then((result) => live && setLeadUserId(result.lead?.userId ?? null))
      .catch(() => {})
    return () => {
      live = false
    }
  }, [team.id, leadAssistantId])
  const known = leadUserId !== undefined
  const current = leadAssistantId && leadUserId ? leadUserId : LEAD_NONE
  const items = leadOptions(users.map((user) => ({ userId: user.id, name: displayUserName(user, user.id) }))).map(
    (option) =>
      typeof option === `string`
        ? { value: LEAD_NONE, label: LEAD_COPY.none }
        : { value: option.userId, label: option.name }
  )
  const save = async (next: string) => {
    setBusy(true)
    try {
      await trpc.agents.setLeadAssistant.mutate(
        { teamId: team.id, userId: next === LEAD_NONE ? null : next },
        SKIP_ERROR_TOAST
      )
      setLeadUserId(next === LEAD_NONE ? null : next)
    } catch (error) {
      failedAction(`save`, `the lead Assistant`, error, { retry: () => void save(next) })
    } finally {
      setBusy(false)
    }
  }
  return (
    <GlassGroup>
      <Picker
        mode="single"
        triggerVariant="row"
        triggerLabel={LEAD_COPY.label}
        mobileTitle={LEAD_COPY.label}
        value={current}
        onChange={(picked) => {
          if (picked === null || picked === current) return
          void save(picked)
        }}
        items={items}
        width="md"
        disabled={!isOwner || busy || !known}
        data-testid={LEAD.testIds.row}
      />
    </GlassGroup>
  )
}
