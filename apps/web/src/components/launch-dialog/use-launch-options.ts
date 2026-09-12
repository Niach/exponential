import { useEffect, useMemo, useRef, useState } from "react"
import {
  agentSeed,
  agentSupportsPlanMode,
  agentSupportsUltracode,
  DEFAULT_LAUNCH_AGENT,
  loadMcpServerPick,
  saveMcpServerPick,
  type CodingLaunchPrefs,
} from "@/lib/coding-launch-prefs"
import {
  preselectMcpServerIds,
  type McpServerRow,
} from "@/lib/mcp-servers"
import { resolveLaunchDeviceId } from "@/lib/launch-device"
import {
  deviceAgentIds,
  deviceAgentLaunchDefaults,
  deviceAgentNotReady,
  deviceDefaultAgent,
  type SteerDevice,
} from "@/lib/steer-devices"
import { CLI_DEFAULT_EFFORT } from "@/components/launch-dialog/launch-options-pane"
import { agentHealth, SYSTEM_PROFILE_ID } from "@/lib/agent-usage"
import type { DeviceAgentHealth } from "@/db/schema"

// EXP-615: the launch-options cluster every start-coding surface shares —
// device settle, the EXP-437 device-seeded agent/model/effort/toggle state,
// the EXP-201 agent clamp, and the resolved `StartCodingOptions` payload.
// Extracted from the launch dialog (which carried it since EXP-257); since
// EXP-825 its one launch caller is the Agent page composer
// (`use-launch-composer.ts`), the others are the automation and device-settings
// editors. The CANDIDATE device list stays with the caller.

export interface LaunchOptions {
  /** The settled device (undefined while the candidate list is empty). */
  device: SteerDevice | undefined
  deviceId: string | null
  /** The PERSON picking a machine in the select — it drops any pending
   * `requestDevice` (EXP-836: an explicit request is one-shot and never
   * fights a human choice). */
  setDeviceId: (deviceId: string) => void
  /** EXP-836: pre-pick a machine a play button named (`?device=`). It wins
   * over the default machine, and keeps winning until that machine shows up
   * in the candidate list — a devices shape that hydrates after the seed can
   * no longer settle on the default instead. Never persisted. */
  requestDevice: (deviceId: string) => void
  /** The requested machine while it is NOT among the candidates (it is
   * offline, has no runnable agent, or has not synced yet) — the caller
   * explains the mismatch. Null once it settles or a person picks. */
  unavailableRequestId: string | null
  /** Agents the settled device advertised (EXP-201). */
  availableAgents: string[]
  agent: string
  /** EXP-773: the picked agent is outside the settled device's reported ACP
   * set, so it cannot start there — every launch surface blocks submit on it
   * while the options pane renders the "not ready" note. */
  agentNotReady: boolean
  /** Re-seeds model/effort/toggles from the device's defaults for `next`. */
  switchAgent: (next: string) => void
  model: string
  setModel: (model: string) => void
  effortValue: string
  setEffortValue: (effort: string) => void
  ultracode: boolean
  setUltracode: (value: boolean) => void
  planMode: boolean
  setPlanMode: (value: boolean) => void
  /** EXP-792: the picked team MCP servers (row ids, list order). Seeded from
   * the team's last pick or its `enabledByDefault` rows once the list loads;
   * every change is persisted per team. */
  mcpServerIds: string[]
  setMcpServerIds: (ids: string[]) => void
  toggleMcpServer: (id: string) => void
  /** EXP-825: the agent account profiles the settled device reports for the
   * picked agent (id + label), the machine's ACTIVE one first. Empty on a
   * pre-profile build. A picker renders only with two or more. */
  accountProfiles: {
    id: string
    label: string
    active: boolean
    /** EXP-849: the device's verdict on that login — a run started on a dead
     *  credential dies on its first call, so the picker says so. */
    health: DeviceAgentHealth
  }[]
  /** The picked profile id — the active one by default, re-seeded on every
   * device or agent change; `undefined` while the device reports none. */
  account: string | undefined
  setAccount: (account: string | undefined) => void
  /** The capability-clamped payload for `steer.startSession`. */
  buildOptions: (args?: { resume?: boolean }) => CodingLaunchPrefs
}

export function useLaunchOptions({
  open,
  devices,
  initialDeviceId,
  planModeOff = false,
  teamId,
  mcpServers = null,
}: {
  open: boolean
  /** The caller's CANDIDATE devices, already capability-filtered. */
  devices: SteerDevice[]
  /** Device to pre-select on open — wins over the first candidate. */
  initialDeviceId?: string
  /** EXP-792: keys the persisted MCP pick; absent = no pick is ever seeded
   * or saved (a surface with no server list). */
  teamId?: string
  /** EXP-792: the team's servers once loaded (null while in flight). The
   * pick seeds ONCE per open from this list. */
  mcpServers?: readonly Pick<McpServerRow, `id` | `enabledByDefault`>[] | null
  /** EXP-772: never seed plan mode from the device's defaults — the chat page
   * starts every conversation in build mode unless the user flips the switch,
   * and a surface that HIDES the switch must send `planMode: false` rather
   * than a value nobody could see. */
  planModeOff?: boolean
}): LaunchOptions {
  const [agent, setAgent] = useState<string>(DEFAULT_LAUNCH_AGENT)
  const [model, setModel] = useState(``)
  const [effortValue, setEffortValue] = useState(CLI_DEFAULT_EFFORT)
  const [ultracode, setUltracode] = useState(false)
  const [planMode, setPlanMode] = useState(false)
  // EXP-836: two slots, not one — the explicit REQUEST and the person's PICK.
  // `resolveLaunchDeviceId` derives the selection from them on every render,
  // so no effect can settle the request away (it used to: the settle effect
  // ran before the seed was consumed, fell back to the default machine and
  // the default then looked like the user's choice).
  const [requestedDeviceId, setRequestedDeviceId] = useState<string | null>(
    initialDeviceId ?? null
  )
  const [pickedDeviceId, setPickedDeviceId] = useState<string | null>(null)
  const [mcpServerIds, setMcpServerIdsState] = useState<string[]>([])
  // EXP-825: the profile pick is keyed to (device, agent) — a switch of
  // either re-seeds it to that pair's active profile (below).
  const [account, setAccount] = useState<string | undefined>(undefined)
  // EXP-792: the pick seeds once per open, the moment the list is there — a
  // reopen reseeds (a teammate may have flipped a default meanwhile).
  const mcpSeededRef = useRef(false)
  // EXP-437: the deviceId whose launch defaults last seeded the options —
  // the 15s devices re-poll must not stomp in-dialog edits, but an actual
  // device change (explicit switch, or a re-settle after the picked machine
  // dropped offline) reseeds.
  const seededDeviceRef = useRef<string | null>(null)

  // Static contract defaults on OPEN until a device settles — the device-seed
  // effect below overlays the selected machine's advertised defaults
  // (EXP-437; its latch is reset here so a reopen reseeds).
  useEffect(() => {
    if (!open) return
    setRequestedDeviceId(initialDeviceId ?? null)
    setPickedDeviceId(null)
    seededDeviceRef.current = null
    mcpSeededRef.current = false
    setMcpServerIdsState([])
    const seed = agentSeed(DEFAULT_LAUNCH_AGENT, null)
    setAgent(DEFAULT_LAUNCH_AGENT)
    setModel(seed.model)
    setEffortValue(CLI_DEFAULT_EFFORT)
    setUltracode(seed.ultracode)
    setPlanMode(planModeOff ? false : seed.planMode)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // The selection, derived: request → pick → default machine (EXP-622) →
  // first candidate. A choice that drops out of the list (the machine went
  // offline) falls back by itself, and one that comes BACK is re-selected —
  // exactly what the old settle effect did, minus the ordering hazard.
  const deviceId = resolveLaunchDeviceId(devices, {
    requested: requestedDeviceId,
    picked: pickedDeviceId,
  })
  const device = devices.find((candidate) => candidate.deviceId === deviceId)
  const setDeviceId = (next: string) => {
    setPickedDeviceId(next)
    setRequestedDeviceId(null)
  }
  const unavailableRequestId =
    requestedDeviceId && requestedDeviceId !== deviceId
      ? requestedDeviceId
      : null

  // Switching the agent tab re-seeds model/effort/toggles to the SELECTED
  // DEVICE's defaults for that agent (EXP-437; static when it advertises
  // none), capability-clamped — the same reseed the desktop dialog does.
  const switchAgent = (next: string) => {
    if (next === agent) return
    setAgent(next)
    const seed = agentSeed(next, deviceAgentLaunchDefaults(device, next))
    setModel(seed.model)
    setEffortValue(seed.effort === `` ? CLI_DEFAULT_EFFORT : seed.effort)
    setUltracode(seed.ultracode)
    setPlanMode(planModeOff ? false : seed.planMode)
  }

  // EXP-437: seed the launch options from the selected device's advertised
  // per-agent defaults — once a device settles after open, and again on
  // every actual device change (the ref latch skips same-device re-polls).
  useEffect(() => {
    if (!open || !device) return
    if (seededDeviceRef.current === device.deviceId) return
    seededDeviceRef.current = device.deviceId
    const available = deviceAgentIds(device)
    const next =
      deviceDefaultAgent(device) ??
      (available.includes(agent)
        ? agent
        : (available[0] ?? DEFAULT_LAUNCH_AGENT))
    const seed = agentSeed(next, deviceAgentLaunchDefaults(device, next))
    setAgent(next)
    setModel(seed.model)
    setEffortValue(seed.effort === `` ? CLI_DEFAULT_EFFORT : seed.effort)
    setUltracode(seed.ultracode)
    setPlanMode(planModeOff ? false : seed.planMode)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device?.deviceId])

  // EXP-792: seed the MCP pick once the team's list is known — the saved
  // pick for the team (clamped to rows that still exist), else the rows the
  // team marked enabled-by-default. Ids that vanish from the list drop out.
  useEffect(() => {
    if (!open || !teamId || mcpServers === null) return
    if (mcpSeededRef.current) {
      const known = new Set(mcpServers.map((server) => server.id))
      setMcpServerIdsState((current) => {
        const kept = current.filter((id) => known.has(id))
        return kept.length === current.length ? current : kept
      })
      return
    }
    mcpSeededRef.current = true
    setMcpServerIdsState(
      preselectMcpServerIds(mcpServers, loadMcpServerPick(teamId))
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, teamId, mcpServers])

  const setMcpServerIds = (ids: string[]) => {
    setMcpServerIdsState(ids)
    if (teamId) saveMcpServerPick(teamId, ids)
  }
  const toggleMcpServer = (id: string) => {
    setMcpServerIds(
      mcpServerIds.includes(id)
        ? mcpServerIds.filter((current) => current !== id)
        : [...mcpServerIds, id]
    )
  }

  // EXP-201: only agents the chosen device advertised are offerable; a
  // device change re-clamps a now-unavailable selection.
  const availableAgents = deviceAgentIds(device)
  const availableAgentsKey = availableAgents.join(`,`)

  // EXP-825: the profiles the settled device reports for the picked agent.
  // The active one leads (it is what the machine runs by default); the pick
  // re-seeds to it whenever the (device, agent) pair changes, and clears when
  // the pair reports no profiles at all (nothing to send).
  const accountProfiles = useMemo(() => {
    const profiles = device?.agentAccounts?.[agent]?.profiles ?? []
    return profiles
      .filter((profile): profile is NonNullable<typeof profile> =>
        Boolean(profile?.id)
      )
      .map((profile) => ({
        id: profile.id,
        label:
          profile.label ||
          (profile.id === SYSTEM_PROFILE_ID ? `Default` : profile.id),
        active: profile.active === true,
        health: agentHealth(profile),
      }))
      .sort((left, right) => Number(right.active) - Number(left.active))
  }, [device, agent])
  const activeProfileId =
    accountProfiles.find((profile) => profile.active)?.id ??
    accountProfiles[0]?.id
  // `activeProfileId` is in the deps too: the profiles ride the device's
  // heartbeat, so they can land AFTER the device settled (and a machine that
  // switches its active login re-seeds the pick to it).
  useEffect(() => {
    if (!open) return
    setAccount(activeProfileId)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device?.deviceId, agent, activeProfileId])

  useEffect(() => {
    if (!open) return
    if (!availableAgents.includes(agent)) {
      switchAgent(availableAgents[0] ?? `claude`)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, availableAgentsKey, agent])

  const buildOptions = ({ resume = false }: { resume?: boolean } = {}) => ({
    agent,
    model,
    effort: effortValue === CLI_DEFAULT_EFFORT ? `` : effortValue,
    ultracode: ultracode && agentSupportsUltracode(agent),
    // A resumed session never re-enters plan mode (EXP-481, mirrors the
    // desktop launcher's clamp).
    planMode: planMode && agentSupportsPlanMode(agent) && !resume,
    ...(resume ? { resume: true } : {}),
    // EXP-792: omitted when nothing is picked — the server treats an absent
    // list and an empty one alike, and older relays never see the key.
    ...(mcpServerIds.length > 0 ? { mcpServerIds: [...mcpServerIds] } : {}),
    // EXP-825: the ambient login is the server's default — only a NAMED
    // profile rides out, and only one the device actually reported.
    ...(account &&
    account !== SYSTEM_PROFILE_ID &&
    accountProfiles.some((profile) => profile.id === account)
      ? { account }
      : {}),
  })

  return {
    device,
    deviceId,
    setDeviceId,
    requestDevice: setRequestedDeviceId,
    unavailableRequestId,
    availableAgents,
    agent,
    agentNotReady: deviceAgentNotReady(device, agent),
    switchAgent,
    model,
    setModel,
    effortValue,
    setEffortValue,
    ultracode,
    setUltracode,
    planMode,
    setPlanMode,
    mcpServerIds,
    setMcpServerIds,
    toggleMcpServer,
    accountProfiles,
    account,
    setAccount,
    buildOptions,
  }
}
