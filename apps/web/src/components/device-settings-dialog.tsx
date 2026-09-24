// Device settings (EXP-481) — the per-device view the row's settings gear
// opens. Name, the EXP-622 default-device toggle and sharing are registry
// writes (work offline); agent defaults edit the SERVER-AUTHORITATIVE devices
// row (an offline device converges on its next heartbeat). Owner-only: the
// gear only exists on "My devices" rows.
//
// EXP-862: NO accounts here. Signing in, picking the default login and
// removing one all live on the account chips (the device row's and the
// Accounts section's) — one surface for a device's logins, not two.
//
// EXP-909 follow-up: a device row carries ONE control, the gear — so Update
// and Remove live HERE, as the last two sections, with the predicates and the
// confirm copy they had on the row.
//
// EXP-1020: ONE layout on the four clients. No worktrees — a machine's
// worktrees are a LOCAL surface, the IDE's Settings → Worktrees, and the
// remote command queue that drove them from here is gone with them. The
// agent-defaults card ends in a "Workflow settings" SUB-SHELL row (the model
// pair a new workflow is seeded from, `launch_defaults.workflow`), and
// "Remove device" is a plain row of the same shell rather than a section of
// its own.
import { useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import { contract } from "@exp/domain-contract"
import type { DeviceIcon } from "@exp/db-schema/domain"
import type { Device } from "@/db/schema"
import {
  conceptIcon,
  Button,
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AccountPicker,
  Combobox,
  AlertDialogHeader,
  AlertDialogTitle,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  BARE_FIELD_CLASS,
  DEVICE_ICON_OPTIONS,
  getDeviceIconName,
  GlassGroup,
  IconPicker,
  Input,
  GlassSectionHeader,
  GlassToggleRow,
  Pill,
  SubShell,
  SubShellHost,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { useNow } from "@/hooks/use-now"
import { deviceCollection, teamCollection } from "@/lib/collections"
import {
  agentModelValues,
  agentSeed,
  agentSupportsPlanMode,
  agentSupportsSubagentModel,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import {
  workflowDefaultsFor,
  workflowDefaultsSummary,
} from "@/lib/devices/workflow-defaults"
import {
  deviceCanUpdateNow,
  deviceRowIsOnline,
  deviceUpdateAvailable,
  showDeviceUpdateButton,
  type SteerDevice,
} from "@/lib/steer-devices"
import {
  AgentOptionsFields,
  CLI_DEFAULT_EFFORT,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import { accountOptionKey, flattenAccounts, type AccountOption } from "@/lib/accounts/account-option"
import { healthBadgeLabel, SYSTEM_PROFILE_ID } from "@/lib/agent-usage"
import { agentLabel } from "@exp/ui"

const RemoveIcon = conceptIcon(`ui-delete`)
const WorkflowIcon = conceptIcon(`nav-workflows`)
const OfflineIcon = conceptIcon(`ui-device-offline`)
const UpdateIcon = conceptIcon(`ui-update`)

/** FEED-36: the tooltip on a queued Update button — the daemon's own rules
 * for getting there (every session ends, or one sits idle for 2 hours). */
/** The tracked-command key of an agent CLI update row (`update claude`) —
 * shared with the desktop dialog's slot naming. */
const agentUpdateKey = (agent: string) => `update ${agent}`
const isAgentUpdateKey = (key: string) => key.startsWith(`update `)

export const QUEUED_UPDATE_TOOLTIP = `Live sessions hold this update — the device restarts itself once every session ends or sits idle for 2 hours.`

// EXP-490 autosave cadence. Defaults debounce longer than the name: every
// setLaunchDefaults call nudges the device over the relay, so coalescing a
// burst of toggles into ONE write matters more than instant persistence.
const NAME_DEBOUNCE_MS = 800
const DEFAULTS_DEBOUNCE_MS = 1_000

interface AgentDraft {
  model: string
  /** EXP-981: claude only; `""` = the CLI's own default. */
  subagentModel?: string
  effort: string
  ultracode: boolean
  planMode: boolean
}

/** One queued/in-flight command the dialog is watching. `key` anchors the
 * inline error/progress to its row (`update <agent>`). */
interface TrackedCommand {
  id: string
  key: string
}

export function DeviceSettingsDialog({
  device,
  open,
  onOpenChange,
  latestVersions,
  liveSessionCount = 0,
  onChanged,
}: {
  /** The row being edited (must be one of the caller's own machines). */
  device: SteerDevice | null
  open: boolean
  onOpenChange: (open: boolean) => void
  /** EXP-420: what the Update section compares the device's version against. */
  latestVersions?: { desktop: string | null; cli: string | null } | null
  /** FEED-36: the caller's live sessions on this device — the "Update now"
   * confirmation counts them (the list owns the query, one for the page). */
  liveSessionCount?: number
  /** Refresh the caller's device list after an update request or a remove. */
  onChanged?: () => void
}) {
  const rowId = device?.rowId
  const deviceId = device?.deviceId

  // The LIVE synced row — renames/share/defaults applied elsewhere stream in;
  // the drafts below latch once per open so sync never stomps typing.
  const { data: liveRows } = useLiveQuery(
    (query) =>
      open && rowId
        ? query.from({ d: deviceCollection }).where(({ d }) => eq(d.id, rowId))
        : undefined,
    [open, rowId]
  )
  const row = (liveRows?.[0] as Device | undefined) ?? null

  const { data: teamRows } = useLiveQuery(
    (query) => (open ? query.from({ t: teamCollection }) : undefined),
    [open]
  )
  const teams = useMemo(
    () =>
      [...(teamRows ?? [])].sort((a, b) => a.name.localeCompare(b.name)) as {
        id: string
        name: string
      }[],
    [teamRows]
  )

  const now = useNow(30_000)
  const online = row ? deviceRowIsOnline(row.lastSeenAt, now) : false
  const kind = row?.kind ?? device?.kind
  const label = row?.label ?? device?.deviceLabel ?? ``

  // ── Drafts, seeded from the live row (EXP-437's seededDeviceRef pattern) ─
  // Since EXP-490 they also RESEED whenever the server-authoritative row
  // changes underneath us — but never on top of an unsaved edit, an in-flight
  // save, or (for the name) a focused input.
  const seededRef = useRef<string | null>(null)
  const [nameDraft, setNameDraft] = useState(``)
  const [agentTab, setAgentTab] = useState<string>(
    contract.codingAgent.values[0]
  )
  const [defaultAgentDraft, setDefaultAgentDraft] = useState<string>(
    contract.codingAgent.values[0]
  )
  // EXP-872: "default agent" is "default account" now — the profile id of
  // the default agent's login the machine starts on; undefined = its
  // active login.
  const [defaultAccountDraft, setDefaultAccountDraft] = useState<
    string | undefined
  >(undefined)
  const [drafts, setDrafts] = useState<Record<string, AgentDraft>>({})
  // EXP-1020: the "Workflow settings" pair (`launch_defaults.workflow`) —
  // what a new workflow created on this machine is seeded from. It belongs
  // to the DEFAULT account's agent, not to the agent tab, so it reseeds
  // whenever that changes.
  const [workflowDraft, setWorkflowDraft] = useState(() =>
    workflowDefaultsFor(contract.codingAgent.values[0], null)
  )

  // ── Autosave state (EXP-490 — no Save buttons) ───────────────────────────
  // `*Pending` = edited but not yet written; `saving*` = a write is in flight.
  const [namePending, setNamePending] = useState(false)
  const [defaultsPending, setDefaultsPending] = useState(false)
  const [savingName, setSavingName] = useState(false)
  const [savingDefaults, setSavingDefaults] = useState(false)

  // The defaults editor covers every agent the machine knows about — runnable
  // ∪ signed-out ∪ already-configured; an offline/quiet machine falls back to
  // the full contract set so its defaults stay editable.
  const editorAgents = useMemo(() => {
    const union = [
      ...(row?.agents ?? []),
      ...(row?.unauthedAgents ?? []),
      ...Object.keys(row?.launchDefaults?.agents ?? {}),
      // EXP-688: an agent the device only reported an ACCOUNT or usage for
      // still gets a tab — its defaults are editable either way.
      ...Object.keys(row?.agentAccounts ?? {}),
      ...Object.keys(row?.agentUsage ?? {}),
    ].filter((agent) => contract.codingAgent.values.includes(agent))
    const unique = [...new Set(union)]
    return unique.length > 0 ? unique : [...contract.codingAgent.values]
  }, [
    row?.agents,
    row?.unauthedAgents,
    row?.launchDefaults,
    row?.agentAccounts,
    row?.agentUsage,
  ])

  // EXP-872: the default-account rows — the machine's flattened logins, or
  // one ambient option per editable agent while it reports none (an offline
  // machine's defaults stay editable either way).
  const defaultAccountOptions = useMemo(() => {
    const flat: AccountOption[] = row ? flattenAccounts(row) : []
    const options: AccountOption[] =
      flat.length > 0
        ? flat
        : editorAgents.map((agent) => ({
            id: SYSTEM_PROFILE_ID,
            agent: agent as AccountOption[`agent`],
            email: agentLabel(agent),
            isDeviceDefault: false,
            health: `unknown` as const,
          }))
    return options.map((option) => ({
      key: accountOptionKey(option),
      id: option.id,
      agent: option.agent,
      email: option.email,
      hint: healthBadgeLabel(option.health) ?? undefined,
      limits: option.limits,
    }))
  }, [row, editorAgents])
  // EXP-1020: the workflow pair is picked from the DEFAULT account's agent's
  // models — the two vocabularies do not overlap, so the tab's agent would
  // offer names a workflow on this machine could never run.
  const workflowModelOptions = useMemo(
    () =>
      agentModelValues(defaultAgentDraft).map((value) => ({
        value,
        label: modelLabel(value),
      })),
    [defaultAgentDraft]
  )

  const defaultAccountKey =
    defaultAccountOptions.find(
      (option) =>
        option.agent === defaultAgentDraft &&
        (defaultAccountDraft
          ? option.id === defaultAccountDraft
          : true)
    )?.key ?? null

  // The value we last wrote, so our OWN write doesn't reseed the drafts back
  // to the pre-write row in the window before it syncs home.
  const sentNameRef = useRef<string | null>(null)
  const sentDefaultsStampRef = useRef(0)

  /** Applies a row's launch defaults to the drafts; returns the default agent. */
  const seedDefaultsFrom = (source: Device, agents: string[]) => {
    const seeded: Record<string, AgentDraft> = {}
    for (const agent of agents) {
      seeded[agent] = agentSeed(
        agent,
        source.launchDefaults?.agents?.[agent] ?? null
      )
    }
    setDrafts(seeded)
    const configuredDefault = source.launchDefaults?.defaultAgent
    const defaultAgent =
      configuredDefault && agents.includes(configuredDefault)
        ? configuredDefault
        : (agents[0] ?? contract.codingAgent.values[0])
    setDefaultAgentDraft(defaultAgent)
    setDefaultAccountDraft(
      configuredDefault === defaultAgent
        ? (source.launchDefaults?.defaultAccount ?? undefined)
        : undefined
    )
    setWorkflowDraft(
      workflowDefaultsFor(defaultAgent, source.launchDefaults?.workflow ?? null)
    )
    return defaultAgent
  }

  useEffect(() => {
    if (!open || !row) return
    if (seededRef.current === row.id) return
    seededRef.current = row.id
    setNameDraft(row.label)
    setAgentTab(seedDefaultsFrom(row, editorAgents))
    setNamePending(false)
    setDefaultsPending(false)
    sentNameRef.current = null
    sentDefaultsStampRef.current = 0
    setSectionErrors({})
    setTracked([])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, row?.id])
  useEffect(() => {
    if (!open) seededRef.current = null
  }, [open])

  // ── Reseed from the live row (EXP-490) ───────────────────────────────────
  // The devices row is server-authoritative: defaults edited on the machine
  // itself or in another client stream in and must win — unless the user has
  // an unsaved edit here, in which case theirs is the newer write.
  const defaultsVersion = useMemo(
    () =>
      `${row?.launchDefaultsUpdatedAt?.getTime() ?? 0}:${JSON.stringify(
        row?.launchDefaults ?? null
      )}`,
    [row?.launchDefaultsUpdatedAt, row?.launchDefaults]
  )
  useEffect(() => {
    if (!open || !row || seededRef.current !== row.id) return
    if (defaultsPending || savingDefaults) return
    // Our own write hasn't come back through Electric yet — keep the drafts.
    if (
      (row.launchDefaultsUpdatedAt?.getTime() ?? 0) < sentDefaultsStampRef.current
    ) {
      return
    }
    seedDefaultsFrom(row, editorAgents)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, row?.id, defaultsVersion, defaultsPending, savingDefaults])

  const nameFocusedRef = useRef(false)
  useEffect(() => {
    if (!open || !row || seededRef.current !== row.id) return
    if (namePending || savingName || nameFocusedRef.current) return
    if (sentNameRef.current !== null) {
      if (row.label !== sentNameRef.current) return
      sentNameRef.current = null
    }
    setNameDraft(row.label)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, row?.id, row?.label, namePending, savingName])

  // ── Icon (EXP-924) ───────────────────────────────────────────────────────
  // Mutates on pick. The pick shows at once and yields to the synced row as
  // soon as it changes (our own write landing, or a pick made elsewhere).
  const [iconPick, setIconPick] = useState<DeviceIcon | null>(null)
  useEffect(() => setIconPick(null), [row?.id, row?.icon])
  const iconName = iconPick ?? getDeviceIconName(row ?? {})
  const pickIcon = (next: DeviceIcon) => {
    if (!deviceId || next === iconName) return
    setIconPick(next)
    setSectionErrors((current) => ({ ...current, name: `` }))
    void trpc.devices.setIcon
      .mutate({ deviceId, icon: next })
      .catch((error) => {
        setIconPick(null)
        setSectionErrors((current) => ({
          ...current,
          name: trpcErrorMessage(error, `That didn't go through. Try again.`),
        }))
      })
  }

  const draft = drafts[agentTab] ?? agentSeed(agentTab, null)

  // ── Section busy/error state ─────────────────────────────────────────────
  const [busySection, setBusySection] = useState<string | null>(null)
  const [sectionErrors, setSectionErrors] = useState<Record<string, string>>(
    {}
  )
  // A finished command's device-reported SUCCESS message, per key — the
  // per-agent Update rows show theirs ("Claude Code updated: …"); a worktree
  // removal shows nothing (its row simply vanishes).
  const [sectionNotes, setSectionNotes] = useState<Record<string, string>>({})
  const runSection = async (section: string, work: () => Promise<void>) => {
    if (busySection) return
    setBusySection(section)
    setSectionErrors((current) => ({ ...current, [section]: `` }))
    try {
      await work()
    } catch (error) {
      setSectionErrors((current) => ({
        ...current,
        [section]: trpcErrorMessage(error, `That didn't go through. Try again.`),
      }))
    } finally {
      setBusySection(null)
    }
  }

  // FEED-33: one switch per team, written straight through like the default
  // toggle (the switches render off the LIVE row, so the server's sorted set
  // streams back on its own). Each write moves ONE team in or out.
  const setShared = (teamId: string, shared: boolean) =>
    runSection(`sharing`, async () => {
      if (!deviceId) return
      await trpc.devices.setShared.mutate({ deviceId, teamId, shared })
    })

  // EXP-622: a single toggle — written straight through, no debounce. The
  // switch renders off the LIVE row, so the server clearing the previous
  // default streams back on its own.
  const setDefault = (isDefault: boolean) =>
    runSection(`default`, async () => {
      if (!deviceId) return
      await trpc.devices.setDefault.mutate({ deviceId, isDefault })
    })

  // ── Autosave (EXP-490) ───────────────────────────────────────────────────
  // Edits schedule a debounced write; blur and dialog close flush it. Timers
  // fire outside render, so the payload is built from this mirror of the
  // newest drafts rather than a stale closure. Pending clears BEFORE the
  // request goes out and comes back only on failure — last write wins, so an
  // edit made mid-flight is never mistaken for saved.
  const latest = useRef({
    deviceId,
    label,
    nameDraft,
    drafts,
    defaultAgentDraft,
    defaultAccountDraft,
    workflowDraft,
    namePending,
    defaultsPending,
  })
  latest.current = {
    deviceId,
    label,
    nameDraft,
    drafts,
    defaultAgentDraft,
    defaultAccountDraft,
    workflowDraft,
    namePending,
    defaultsPending,
  }

  const nameTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const defaultsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const flushName = () => {
    if (nameTimerRef.current) {
      clearTimeout(nameTimerRef.current)
      nameTimerRef.current = null
    }
    const snapshot = latest.current
    if (!snapshot.namePending) return
    const nextLabel = snapshot.nameDraft.trim()
    latest.current.namePending = false
    setNamePending(false)
    // Blank or unchanged never writes — the reseed effect restores the row's
    // name once the field loses focus.
    if (!snapshot.deviceId || !nextLabel || nextLabel === snapshot.label) return
    sentNameRef.current = nextLabel
    setSavingName(true)
    setSectionErrors((current) => ({ ...current, name: `` }))
    void trpc.devices.rename
      .mutate({ deviceId: snapshot.deviceId, label: nextLabel })
      .catch((error) => {
        sentNameRef.current = null
        latest.current.namePending = true
        setNamePending(true)
        setSectionErrors((current) => ({
          ...current,
          name: trpcErrorMessage(error, `That didn't go through. Try again.`),
        }))
      })
      .finally(() => setSavingName(false))
  }

  const flushDefaults = () => {
    if (defaultsTimerRef.current) {
      clearTimeout(defaultsTimerRef.current)
      defaultsTimerRef.current = null
    }
    const snapshot = latest.current
    if (!snapshot.defaultsPending) return
    latest.current.defaultsPending = false
    setDefaultsPending(false)
    if (!snapshot.deviceId) return
    const agents: Record<
      string,
      {
        model?: string
        effort?: string
        ultracode?: boolean
        planMode?: boolean
      }
    > = {}
    for (const [agent, value] of Object.entries(snapshot.drafts)) {
      agents[agent] = {
        model: value.model,
        // EXP-981: claude only, and only when it is not the CLI default.
        ...(agentSupportsSubagentModel(agent) && value.subagentModel
          ? { subagentModel: value.subagentModel }
          : {}),
        effort: value.effort,
        ...(agentSupportsUltracode(agent) ? { ultracode: value.ultracode } : {}),
        ...(agentSupportsPlanMode(agent) ? { planMode: value.planMode } : {}),
      }
    }
    setSavingDefaults(true)
    setSectionErrors((current) => ({ ...current, defaults: `` }))
    void trpc.devices.setLaunchDefaults
      .mutate({
        deviceId: snapshot.deviceId,
        launchDefaults: {
          defaultAgent: snapshot.defaultAgentDraft,
          // An explicit null CLEARS the stored pin (the system login); an
          // absent key means "an older client, keep it" to the server.
          defaultAccount:
            snapshot.defaultAccountDraft &&
            snapshot.defaultAccountDraft !== SYSTEM_PROFILE_ID
              ? snapshot.defaultAccountDraft
              : null,
          agents,
          // EXP-1020: the pair rides EVERY save — setLaunchDefaults REPLACES
          // the stored object, and the server only carries a stored pair
          // forward for clients that predate the key.
          workflow: snapshot.workflowDraft,
        },
      })
      .then((result) => {
        const stamp = result.launchDefaultsUpdatedAt
        const parsed = stamp ? Date.parse(stamp) : Number.NaN
        if (!Number.isNaN(parsed)) sentDefaultsStampRef.current = parsed
      })
      .catch((error) => {
        latest.current.defaultsPending = true
        setDefaultsPending(true)
        setSectionErrors((current) => ({
          ...current,
          defaults: trpcErrorMessage(
            error,
            `That didn't go through. Try again.`
          ),
        }))
      })
      .finally(() => setSavingDefaults(false))
  }

  const scheduleName = () => {
    latest.current.namePending = true
    setNamePending(true)
    if (nameTimerRef.current) clearTimeout(nameTimerRef.current)
    nameTimerRef.current = setTimeout(() => flushName(), NAME_DEBOUNCE_MS)
  }

  const scheduleDefaults = () => {
    latest.current.defaultsPending = true
    setDefaultsPending(true)
    if (defaultsTimerRef.current) clearTimeout(defaultsTimerRef.current)
    defaultsTimerRef.current = setTimeout(
      () => flushDefaults(),
      DEFAULTS_DEBOUNCE_MS
    )
  }

  const patchWorkflow = (patch: Partial<typeof workflowDraft>) => {
    setWorkflowDraft((current) => ({ ...current, ...patch }))
    scheduleDefaults()
  }

  const patchDraft = (patch: Partial<AgentDraft>) => {
    setDrafts((current) => ({
      ...current,
      [agentTab]: {
        ...(current[agentTab] ?? agentSeed(agentTab, null)),
        ...patch,
      },
    }))
    scheduleDefaults()
  }

  // Closing (or unmounting) writes whatever is still pending — the tRPC
  // promise outlives the component, so fire-and-forget is enough.
  useEffect(() => {
    if (!open) return
    return () => {
      flushName()
      flushDefaults()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // ── Agent CLI updates (durable queue + poll while open) ──────────────────
  // EXP-1020: worktree_remove / worktree_prune left with the worktrees
  // section — the only command this dialog still queues is an agent update.
  const [tracked, setTracked] = useState<TrackedCommand[]>([])

  const queueCommand = async (
    key: string,
    input: { kind: `agent_update`; agent: string }
  ) => {
    if (!deviceId) return
    setSectionErrors((current) => ({ ...current, [key]: `` }))
    setSectionNotes((current) => ({ ...current, [key]: `` }))
    try {
      const { id } = await trpc.devices.createCommand.mutate(
        {
          deviceId,
          ...input,
        },
        // The section prints the failure itself — no second toast from the
        // global link.
        { context: { skipErrorToast: true } }
      )
      setTracked((current) => [...current, { id, key }])
    } catch (error) {
      setSectionErrors((current) => ({
        ...current,
        [key]: trpcErrorMessage(error, `Couldn't queue that on the device.`),
      }))
    }
  }

  // Poll queued commands until terminal — the material outcome (a worktree
  // vanishing) additionally streams in via sync when the device re-reports.
  // Offline devices keep their commands queued server-side; poll slowly.
  useEffect(() => {
    if (!open || tracked.length === 0) return
    let cancelled = false
    const tick = async () => {
      for (const command of tracked) {
        try {
          const result = await trpc.devices.getCommand.query({
            commandId: command.id,
          })
          if (cancelled || result.status === `pending`) continue
          setTracked((current) => current.filter((c) => c.id !== command.id))
          if (result.status === `failed`) {
            setSectionErrors((current) => ({
              ...current,
              [command.key]:
                result.result ?? `The device reported a failure.`,
            }))
          } else if (isAgentUpdateKey(command.key) && result.result) {
            // The version move is the outcome — the row's version line
            // follows on the next heartbeat; the message says it now.
            const note = result.result
            setSectionNotes((current) => ({ ...current, [command.key]: note }))
          }
        } catch {
          // Transient — keep polling.
        }
      }
    }
    // Leading tick (EXP-490): with the relay nudge an online machine finishes
    // a remove in ~1-2s — a trailing-only interval added a guaranteed 2s.
    void tick()
    const interval = setInterval(() => void tick(), online ? 2_000 : 8_000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [open, tracked, online])

  const pendingKey = (key: string) =>
    tracked.some((command) => command.key === key)

  // ── Update + Remove (EXP-909 follow-up) ──────────────────────────────────
  // The controls the device row used to carry, predicates and confirm copy
  // unchanged. `device` re-resolves from the live list on every render, so
  // these read the same synced state the row did.
  const [requestingUpdate, setRequestingUpdate] = useState(false)
  const [updateNowOpen, setUpdateNowOpen] = useState(false)
  const [removeOpen, setRemoveOpen] = useState(false)
  const [deviceBusy, setDeviceBusy] = useState(false)

  const latestVersion =
    (kind === `server` ? latestVersions?.cli : latestVersions?.desktop) ?? null
  const version = row?.version ?? device?.version ?? null
  const outdated = deviceUpdateAvailable(version, latestVersion)
  // FEED-36: a queued update parked behind live sessions says so, and a
  // capable daemon offers to end them now.
  const updateQueued = Boolean(device?.updateRequested && device?.updateBlocked)
  const showUpdateButton =
    (device ? showDeviceUpdateButton(device, latestVersion) : false) ||
    requestingUpdate
  // The agent CLI rows: one per agent the machine reports an install for
  // (its heartbeat account row), version off that row. The "Update" control
  // queues `agent_update` (the CLI's own self-updater, run on the machine).
  // No cap: only daemons that REPORT `version` on the account row can run
  // the command (older ones answer "doesn't support that command yet"), so
  // the button is gated on a known version; the release min-version gate
  // retires those builds eventually.
  const agentUpdateRows = contract.codingAgent.values.flatMap((agent) => {
    const account = row?.agentAccounts?.[agent]
    return account ? [{ agent, version: account.version ?? null }] : []
  })
  const showUpdateSection = kind === `server` || agentUpdateRows.length > 0

  const requestUpdate = async () => {
    if (!deviceId || requestingUpdate) return
    setRequestingUpdate(true)
    try {
      await trpc.devices.requestUpdate.mutate({ deviceId })
      onChanged?.()
    } finally {
      setRequestingUpdate(false)
    }
  }

  const updateNow = async () => {
    if (!deviceId || deviceBusy) return
    setDeviceBusy(true)
    try {
      await trpc.devices.requestUpdate.mutate({ deviceId, endSessions: true })
      setUpdateNowOpen(false)
      onChanged?.()
    } finally {
      setDeviceBusy(false)
    }
  }

  const removeDevice = async () => {
    if (!deviceId || deviceBusy) return
    setDeviceBusy(true)
    try {
      await trpc.devices.remove.mutate({ deviceId })
      setRemoveOpen(false)
      onChanged?.()
      // The row is gone — so is the thing this dialog edits.
      onOpenChange(false)
    } finally {
      setDeviceBusy(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* EXP-686: no description — the machine's name is already the row you
          opened this from, and repeating it pushed the settings down a line
          on every client. `aria-describedby={undefined}` keeps Radix from
          pointing at a description that no longer exists. */}
      <DialogContent
        mobile="sheet-full"
        // EXP-862: height = CONTENT, capped. Losing the account block left a
        // fixed 46rem sheet half empty; the columns still scroll on their own
        // once the cap bites.
        className="gap-4 sm:max-h-[min(90dvh,46rem)] sm:max-w-3xl"
        aria-describedby={undefined}
        // EXP-698: Radix autofocuses the first field and SELECTS its text, so
        // the Name row opened as a white selection block filling the row (and
        // the phone keyboard shot up over the sheet). The sheet is a list of
        // settings, not a form to type into — focus stays on the panel until
        // the field is tapped.
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>Device settings</DialogTitle>
        </DialogHeader>
        {/* EXP-694: one inset-grouped card stack — the same rows, in the same
            order, as the iOS/Android device sheets. 8px between groups. ONE
            column on every width since EXP-1020 took the worktrees away: the
            landscape split (EXP-798) existed to park them beside the
            settings, and a single column is what the phone sheet and the
            three native clients show.
            EXP-1029/1020: the stack is a SUB-SHELL host — "Workflow settings"
            slides its page in place of the whole stack, with a back button on
            top, rather than opening a dialog on top of a dialog. */}
        <SubShellHost className="min-h-0 flex-1 gap-2 overflow-y-auto">
          <div className="flex flex-col gap-2 *:shrink-0">
            {/* ── Name ─────────────────────────────────────────────────── */}
            {/* EXP-924: the identity row every form shares (board, action):
                the icon picker, then the bare name field. */}
            <GlassGroup>
              <div className="flex items-center gap-2 px-4 py-3">
                <IconPicker
                  id="device-settings-icon"
                  value={iconName}
                  options={DEVICE_ICON_OPTIONS}
                  onChange={(next) => next && pickIcon(next)}
                  disabled={!row}
                />
                <Input
                  id="device-settings-name"
                  aria-label="Name"
                  placeholder="Name"
                  className={BARE_FIELD_CLASS}
                  value={nameDraft}
                  maxLength={255}
                  onChange={(event) => {
                    setNameDraft(event.target.value)
                    scheduleName()
                  }}
                  onFocus={() => {
                    nameFocusedRef.current = true
                  }}
                  onBlur={() => {
                    nameFocusedRef.current = false
                    // A rename that arrived while the field was focused was
                    // deliberately skipped — catch up unless an edit is owed.
                    const hadPending = latest.current.namePending
                    flushName()
                    if (!hadPending && row) setNameDraft(row.label)
                  }}
                  onKeyDown={(event) => {
                    if (event.key === `Enter`) flushName()
                  }}
                />
                {savingName && (
                  <LoaderCircle className="size-3 shrink-0 animate-spin text-muted-foreground" />
                )}
              </div>
            </GlassGroup>
            {sectionErrors.name && (
              <p className="px-1 text-xs text-destructive">
                {sectionErrors.name}
              </p>
            )}

            {/* ── Default device (EXP-622) ─────────────────────────────── */}
            <GlassGroup>
              <GlassToggleRow
                id="device-settings-default"
                label="Default device"
                checked={row?.isDefault ?? false}
                onCheckedChange={(checked) => void setDefault(checked)}
                disabled={busySection !== null}
              />
            </GlassGroup>
            {sectionErrors.default && (
              <p className="px-1 text-xs text-destructive">
                {sectionErrors.default}
              </p>
            )}

            {/* ── Sharing (server devices only, EXP-432/FEED-33) ───────── */}
            {kind === `server` && (
              <>
                <GlassSectionHeader label="Sharing" />
                {teams.length > 0 ? (
                  /* EXP-939: a long roster scrolls INSIDE the group instead of
                     pushing the agent card off the column. */
                  <GlassGroup scroll className="max-h-56 *:shrink-0">
                    {teams.map((team) => (
                      <GlassToggleRow
                        key={team.id}
                        id={`device-settings-share-${team.id}`}
                        label={team.name}
                        checked={row?.sharedTeamIds?.includes(team.id) ?? false}
                        onCheckedChange={(checked) =>
                          void setShared(team.id, checked)
                        }
                        disabled={busySection !== null}
                      />
                    ))}
                  </GlassGroup>
                ) : (
                  <p className="px-1 text-xs text-muted-foreground">
                    Join a team to share this device.
                  </p>
                )}
                <p className="px-1 text-xs text-muted-foreground">
                  Teammates of a shared team can start coding sessions on this
                  device.
                </p>
                {sectionErrors.sharing && (
                  <p className="px-1 text-xs text-destructive">
                    {sectionErrors.sharing}
                  </p>
                )}
              </>
            )}

            {/* ── Agent defaults (server-authoritative, EXP-481) ────────── */}
            {(!online || savingDefaults) && (
              <div className="flex items-center justify-between px-1">
                {!online ? (
                  <span className="flex items-center gap-1 text-xs text-muted-foreground">
                    <OfflineIcon className="size-3" />
                    Applies when the device comes online.
                  </span>
                ) : (
                  <span />
                )}
                {savingDefaults && (
                  <LoaderCircle className="size-3 animate-spin text-muted-foreground" />
                )}
              </div>
            )}
            {/* EXP-872: the ONE account picker — "Default agent" is "Default
                account": the machine's logins by email, both agents, and a
                pick names the agent too. */}
            <GlassGroup>
              <AccountPicker
                variant="row"
                mobileTitle="Default account"
                value={defaultAccountKey}
                options={defaultAccountOptions}
                onChange={(key) => {
                  const option = defaultAccountOptions.find(
                    (candidate) => candidate.key === key
                  )
                  if (!option) return
                  setDefaultAgentDraft(option.agent)
                  setDefaultAccountDraft(option.id)
                  // The pair belongs to the picked agent now: a name from the
                  // other vocabulary falls back to that agent's defaults.
                  setWorkflowDraft((current) =>
                    workflowDefaultsFor(option.agent, current)
                  )
                  scheduleDefaults()
                }}
                data-testid="device-settings-default-account"
              />
            </GlassGroup>
            <AgentOptionsFields
              idPrefix="device-settings"
              agent={agentTab}
              availableAgents={editorAgents}
              onAgentChange={setAgentTab}
              model={draft.model}
              onModelChange={(value) => patchDraft({ model: value })}
              subagentModel={draft.subagentModel ?? ``}
              onSubagentModelChange={(value) =>
                patchDraft({ subagentModel: value })
              }
              effortValue={
                draft.effort === `` ? CLI_DEFAULT_EFFORT : draft.effort
              }
              onEffortChange={(value) =>
                patchDraft({
                  effort: value === CLI_DEFAULT_EFFORT ? `` : value,
                })
              }
              ultracode={draft.ultracode}
              onUltracodeChange={(value) => patchDraft({ ultracode: value })}
              planMode={draft.planMode}
              onPlanModeChange={(value) => patchDraft({ planMode: value })}
            />
            {/* EXP-1020: the model pair a workflow started on this machine is
                seeded from. Shown whichever agent is selected — it hangs off
                the DEFAULT account's agent, not the tab — and it opens as a
                page of this same shell rather than a second dialog. */}
            <GlassGroup>
              <SubShell
                label="Workflow settings"
                icon={WorkflowIcon}
                value={workflowDefaultsSummary(workflowDraft, modelLabel)}
                data-testid="device-settings-workflow"
              >
                <GlassGroup>
                  <Combobox
                    triggerVariant="row"
                    searchable={false}
                    mobileTitle="Model"
                    value={workflowDraft.model}
                    onChange={(value) => {
                      if (value !== null) patchWorkflow({ model: value })
                    }}
                    options={workflowModelOptions}
                  />
                  <Combobox
                    triggerVariant="row"
                    searchable={false}
                    mobileTitle="Strong model"
                    value={workflowDraft.strongModel}
                    onChange={(value) => {
                      if (value !== null) patchWorkflow({ strongModel: value })
                    }}
                    options={workflowModelOptions}
                  />
                </GlassGroup>
                <p className="px-1 text-xs text-muted-foreground">
                  Leaf nodes and the subagents inside them run on the model.
                  Contract, integration and risky nodes, and every review, run
                  on the strong model.
                </p>
              </SubShell>
            </GlassGroup>
            {sectionErrors.defaults && (
              <p className="px-1 text-xs text-destructive">
                {sectionErrors.defaults}
              </p>
            )}
            {/* ── Update: the daemon row (server devices only — desktop apps
                update themselves, EXP-420/FEED-36) plus one row per agent CLI
                the machine reports, each with its own remote self-update. */}
            {showUpdateSection && (
              <>
                <GlassSectionHeader label="Update" />
                <GlassGroup>
                  {kind === `server` && (
                    <div className="flex items-center gap-3 px-4 py-3">
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm text-foreground">
                          {version ? `v${version}` : `Version unknown`}
                        </div>
                        {outdated && (
                          <div className="truncate text-xs text-amber-500">
                            Update available: v{latestVersion}
                          </div>
                        )}
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        {showUpdateButton && (
                          <Button
                            variant="ghost"
                            size="sm"
                            className={
                              outdated ? `text-amber-500` : `text-muted-foreground`
                            }
                            disabled={device?.updateRequested || requestingUpdate}
                            title={
                              updateQueued
                                ? QUEUED_UPDATE_TOOLTIP
                                : `Ask the daemon to self-update (it restarts when idle)`
                            }
                            onClick={() => void requestUpdate()}
                          >
                            {updateQueued ? (
                              // EXP-411: parked behind live sessions — say so
                              // instead of spinning until the last one closes.
                              <>
                                <UpdateIcon />
                                Queued
                              </>
                            ) : device?.updateRequested || requestingUpdate ? (
                              <>
                                <LoaderCircle className="animate-spin" />
                                Updating…
                              </>
                            ) : (
                              <>
                                <UpdateIcon />
                                Update
                              </>
                            )}
                          </Button>
                        )}
                        {updateQueued && device && deviceCanUpdateNow(device) && (
                          <Pill
                            mode="action"
                            onClick={() => setUpdateNowOpen(true)}
                            title={`End this device's live sessions and restart it on the new version now.`}
                          >
                            <UpdateIcon className="size-3" />
                            Update now…
                          </Pill>
                        )}
                      </div>
                    </div>
                  )}
                  {agentUpdateRows.map(({ agent, version: agentVersion }) => {
                    const key = agentUpdateKey(agent)
                    const updating = pendingKey(key)
                    return (
                      <div
                        key={agent}
                        className="flex flex-col gap-0.5 px-4 py-3"
                      >
                        <div className="flex items-center gap-3">
                          <div className="min-w-0 flex-1 truncate text-sm text-foreground">
                            {agentLabel(agent)}
                            {agentVersion && (
                              <>
                                {` `}
                                <span className="text-muted-foreground">
                                  {`v${agentVersion}`}
                                </span>
                              </>
                            )}
                          </div>
                          {agentVersion ? (
                            <Button
                              variant="ghost"
                              size="sm"
                              className="shrink-0 text-muted-foreground"
                              disabled={updating}
                              title={
                                online
                                  ? `Run \`${agent} update\` on this device.`
                                  : `Run \`${agent} update\` on this device (queued until it comes online).`
                              }
                              onClick={() =>
                                void queueCommand(key, {
                                  kind: `agent_update`,
                                  agent,
                                })
                              }
                            >
                              {updating ? (
                                <>
                                  <LoaderCircle className="animate-spin" />
                                  Updating…
                                </>
                              ) : (
                                <>
                                  <UpdateIcon />
                                  Update
                                </>
                              )}
                            </Button>
                          ) : null}
                        </div>
                        {sectionErrors[key] ? (
                          <p className="text-xs text-destructive">
                            {sectionErrors[key]}
                          </p>
                        ) : sectionNotes[key] ? (
                          <p className="text-xs text-muted-foreground">
                            {sectionNotes[key]}
                          </p>
                        ) : (
                          // No reported version = a daemon that cannot run
                          // `agent_update` yet. The hint sits UNDER the label
                          // (like a note) so the agent's name never truncates
                          // beside a sentence-long trailing slot.
                          !agentVersion && (
                            <p className="text-xs text-muted-foreground">
                              Update the app on this machine first
                            </p>
                          )
                        )}
                      </div>
                    )
                  })}
                </GlassGroup>
                {updateQueued && (
                  <p className="px-1 text-xs text-amber-500">
                    {QUEUED_UPDATE_TOOLTIP}
                  </p>
                )}
              </>
            )}

            {/* ── Remove (EXP-1020: a row of the same shell, not a section
                of its own — it needs no headline to be found) ───────────── */}
            <GlassGroup>
              <Button
                variant="ghost"
                className="h-auto w-full justify-start rounded-none px-4 py-3 text-destructive hover:text-destructive"
                onClick={() => setRemoveOpen(true)}
              >
                <RemoveIcon className="size-3.5" />
                Remove device
              </Button>
            </GlassGroup>
          </div>
        </SubShellHost>

        {/* FEED-36: Update now — the daemon ends every live session on the
            machine and restarts on the queued version; confirmed, since it
            interrupts work (repo-backed runs resume from their session page). */}
        <AlertDialog
          open={updateNowOpen}
          onOpenChange={(nextOpen) => {
            if (!nextOpen && !deviceBusy) setUpdateNowOpen(false)
          }}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {`Update ${label || deviceId || `this device`} now?`}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {liveSessionCount > 0
                  ? `Ends the ${liveSessionCount} live ${
                      liveSessionCount === 1 ? `session` : `sessions`
                    } on this device (repo-backed runs can be resumed from their session page) and restarts it on the new version.`
                  : `Ends every live session on this device (repo-backed runs can be resumed from their session page) and restarts it on the new version.`}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={deviceBusy}>Cancel</AlertDialogCancel>
              <AlertDialogAction
                disabled={deviceBusy}
                onClick={(event) => {
                  event.preventDefault()
                  void updateNow()
                }}
              >
                {deviceBusy && <LoaderCircle className="animate-spin" />}
                Update now
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        <AlertDialog
          open={removeOpen}
          onOpenChange={(nextOpen) => {
            if (!nextOpen && !deviceBusy) setRemoveOpen(false)
          }}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Remove device</AlertDialogTitle>
              <AlertDialogDescription>
                Remove “{label || deviceId}” from your devices? A device with
                the daemon still running will re-register itself on its next
                heartbeat.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={deviceBusy}>Cancel</AlertDialogCancel>
              <AlertDialogAction
                disabled={deviceBusy}
                onClick={(event) => {
                  event.preventDefault()
                  void removeDevice()
                }}
              >
                {deviceBusy && <LoaderCircle className="animate-spin" />}
                Remove
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  )
}
