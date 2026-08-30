import { useEffect, useRef, useState } from "react"
import {
  agentSeed,
  agentSupportsPlanMode,
  agentSupportsUltracode,
  DEFAULT_LAUNCH_AGENT,
  type CodingLaunchPrefs,
} from "@/lib/coding-launch-prefs"
import {
  defaultDeviceId,
  deviceAgentIds,
  deviceAgentLaunchDefaults,
  deviceDefaultAgent,
  type SteerDevice,
} from "@/lib/steer-devices"
import { CLI_DEFAULT_EFFORT } from "@/components/launch-dialog/launch-options-pane"

// EXP-615: the launch-options cluster every start-coding surface shares —
// device settle, the EXP-437 device-seeded agent/model/effort/toggle state,
// the EXP-201 agent clamp, and the resolved `StartCodingOptions` payload.
// Extracted verbatim from the launch dialog (which carried it since EXP-257)
// so the create-action dialog stops duplicating it; the CANDIDATE device list
// stays with the caller, which is the only one that knows the capability gates
// a tab or a builtin needs.

export interface LaunchOptions {
  /** The settled device (undefined while the candidate list is empty). */
  device: SteerDevice | undefined
  deviceId: string | null
  setDeviceId: (deviceId: string) => void
  /** Agents the settled device advertised (EXP-201). */
  availableAgents: string[]
  agent: string
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
  /** The capability-clamped payload for `steer.startSession`. */
  buildOptions: (args?: { resume?: boolean }) => CodingLaunchPrefs
}

export function useLaunchOptions({
  open,
  devices,
  initialDeviceId,
}: {
  open: boolean
  /** The caller's CANDIDATE devices, already capability-filtered. */
  devices: SteerDevice[]
  /** Device to pre-select on open — wins over the first candidate. */
  initialDeviceId?: string
}): LaunchOptions {
  const [agent, setAgent] = useState<string>(DEFAULT_LAUNCH_AGENT)
  const [model, setModel] = useState(``)
  const [effortValue, setEffortValue] = useState(CLI_DEFAULT_EFFORT)
  const [ultracode, setUltracode] = useState(false)
  const [planMode, setPlanMode] = useState(false)
  const [deviceId, setDeviceId] = useState<string | null>(null)
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
    setDeviceId(initialDeviceId ?? null)
    seededDeviceRef.current = null
    const seed = agentSeed(DEFAULT_LAUNCH_AGENT, null)
    setAgent(DEFAULT_LAUNCH_AGENT)
    setModel(seed.model)
    setEffortValue(CLI_DEFAULT_EFFORT)
    setUltracode(seed.ultracode)
    setPlanMode(seed.planMode)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // Settle the device on open + whenever the candidate list changes (tab
  // switch, action selection, a desktop connecting mid-dialog); a still-valid
  // current choice is kept, else the caller's default machine (EXP-622), else
  // the first candidate.
  const fallbackDeviceId =
    defaultDeviceId(devices) ?? devices[0]?.deviceId ?? null
  useEffect(() => {
    if (!open) return
    setDeviceId((current) =>
      current && devices.some((d) => d.deviceId === current)
        ? current
        : fallbackDeviceId
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, devices])

  const device =
    devices.find((candidate) => candidate.deviceId === deviceId) ??
    devices.find((candidate) => candidate.deviceId === fallbackDeviceId)

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
    setPlanMode(seed.planMode)
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
    setPlanMode(seed.planMode)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device?.deviceId])

  // EXP-201: only agents the chosen device advertised are offerable; a
  // device change re-clamps a now-unavailable selection.
  const availableAgents = deviceAgentIds(device)
  const availableAgentsKey = availableAgents.join(`,`)
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
  })

  return {
    device,
    deviceId,
    setDeviceId,
    availableAgents,
    agent,
    switchAgent,
    model,
    setModel,
    effortValue,
    setEffortValue,
    ultracode,
    setUltracode,
    planMode,
    setPlanMode,
    buildOptions,
  }
}
