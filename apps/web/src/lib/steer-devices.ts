import { contract } from "@exp/domain-contract"

import type { Device, SyncedDeviceWorktree, User } from "@/db/schema"
import { parseVersionTuple } from "./client-version"
import type { AgentLaunchDefaults } from "./coding-launch-prefs"

// The caller's machines (EXP-403). The ONE source is the synced `devices`
// shape: `steerDeviceFromRow`/`composeDeviceList` below turn its rows into
// this shape, and online-ness derives from `last_seen_at` freshness against
// the contract window — never from relay presence (EXP-639 retired the
// `devices.list` merge and the relay-presence rows it also had to fit).

export interface SteerDevice {
  /** EXP-481: the synced devices row id — joins `device_worktrees` rows. */
  rowId?: string
  deviceId: string
  deviceLabel: string
  /** EXP-201/EXP-409: the RUNNABLE agent CLIs on the device (installed AND
   * signed in) — empty means the machine can run nothing right now. */
  agents?: string[]
  /** EXP-409: agents installed but signed out on the machine — never
   * offered in pickers; shown with a "sign in" hint instead. */
  unauthedAgents?: string[]
  /** EXP-253: launch capabilities beyond issue coding (e.g. `actions`);
   * absent = an older desktop with none. */
  caps?: string[]
  /** EXP-403 registry fields. */
  kind?: `desktop` | `server`
  /** The machine's OS as of its last register; null for old builds. */
  platform?: string | null
  online?: boolean
  /** ISO timestamp of the last register/heartbeat; null for relay-only rows. */
  lastSeenAt?: string | null
  registered?: boolean
  /** Marketing version as of the last register; null for old builds. */
  version?: string | null
  /** A web "Update" click is pending on the device. */
  updateRequested?: boolean
  /** EXP-411: the pending update is parked behind live coding sessions —
   * the daemon applies it once they close. */
  updateBlocked?: boolean
  /** EXP-432: the team this device is shared with (null/absent = private). */
  sharedTeamId?: string | null
  /** EXP-432: set only on teammates' shared rows — the device owner. Absent
   * on the caller's own rows. */
  owner?: { id: string; name: string }
  /** EXP-437: the machine's per-agent launch defaults from its live
   * presence — the Start-coding dialog seeds its options from the selected
   * device. Absent = old desktop build (or offline row); seed statically. */
  launchDefaults?: DeviceLaunchDefaults
  /** EXP-622: the caller's default machine — pickers prefill it over the
   * first candidate. Set only on the caller's OWN rows: the flag lives on
   * the device row and is its owner's preference, so a teammate's shared
   * server never prefills off it. */
  isDefault?: boolean
}

/** EXP-437: a device's launch-defaults advertisement — `agents` keyed by
 * contract `codingAgent` id, covering only the machine's RUNNABLE agents. */
export interface DeviceLaunchDefaults {
  defaultAgent?: string
  agents?: Record<string, AgentLaunchDefaults>
}

/** EXP-437: the device's configured default agent, clamped to what it can
 * actually run — `null` when it advertises none (older build) or the
 * configured default is not runnable there. */
export function deviceDefaultAgent(
  device: SteerDevice | undefined
): string | null {
  const candidate = device?.launchDefaults?.defaultAgent
  if (!candidate) return null
  return deviceAgentIds(device).includes(candidate) ? candidate : null
}

/** EXP-437: the device's advertised defaults for one agent — `null` when it
 * advertises none (the caller seeds statically via `agentSeed(agent, null)`). */
export function deviceAgentLaunchDefaults(
  device: SteerDevice | undefined,
  agent: string
): AgentLaunchDefaults | null {
  return device?.launchDefaults?.agents?.[agent] ?? null
}

/** EXP-622: the caller's default machine among `devices`, or `null` when
 * none of them is flagged. Callers pass an already capability-filtered
 * CANDIDATE list, so an offline or incapable default simply drops out and
 * the caller falls back to its first candidate. */
export function defaultDeviceId(devices: SteerDevice[]): string | null {
  return devices.find((device) => device.isDefault === true)?.deviceId ?? null
}

/** EXP-432: whether the row is one of the caller's own machines (teammates'
 * shared rows carry `owner`; own rows never do). */
export function deviceIsMine(device: SteerDevice): boolean {
  return !device.owner
}

/** Whether the device is startable right now (`steerDeviceFromRow` stamps
 * `online` from `last_seen_at` freshness). */
export function deviceIsOnline(device: SteerDevice): boolean {
  return device.online !== false
}

/** Agents the device can run — empty means the machine can run nothing right
 * now (EXP-409: every installed agent is installed but signed out). */
export function deviceAgentIds(device: SteerDevice | undefined): string[] {
  return (device?.agents ?? []).filter((a) =>
    contract.codingAgent.values.includes(a)
  )
}

/** EXP-409: agents installed but signed out on the device. */
export function deviceUnauthedAgentIds(device: SteerDevice | undefined): string[] {
  return (device?.unauthedAgents ?? []).filter((a) =>
    contract.codingAgent.values.includes(a)
  )
}

/** EXP-409: online but with nothing runnable — every installed agent is
 * signed out. Such a machine renders greyed out with a "sign in on that
 * machine" reason instead of an agent picker. */
export function deviceHasRunnableAgent(device: SteerDevice): boolean {
  return deviceAgentIds(device).length > 0
}

/** EXP-530: only devices advertising this capability evaluate action
 * triggers locally — older builds would accept the binding and never fire.
 * Offline-but-capable devices stay pickable (they catch up on reconnect). */
export function deviceCanRunAutomations(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`automations`)
}

/** EXP-637: resuming an ENDED run (its worktree, its agent transcript) is a
 * launch path of its own — distinct from EXP-481's `resume`, which resumes an
 * issue's live worktree. Runs lists hide Resume on devices without it. */
export function deviceCanResumeRun(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`resume-run`)
}

/** EXP-481: whether a devices row reads "online" — `last_seen_at` within the
 * contract window of `now` (devices heartbeat ~30s; the window is three
 * missed beats). A negative age (server stamp ahead of the client clock)
 * clamps to online — skew must never mark a beating machine offline. */
export function deviceRowIsOnline(
  lastSeenAt: Date | string,
  now: Date
): boolean {
  const seen =
    typeof lastSeenAt === `string` ? new Date(lastSeenAt) : lastSeenAt
  const age = now.getTime() - seen.getTime()
  if (Number.isNaN(age)) return false
  return age <= contract.device.onlineWindowSeconds * 1000
}

/** EXP-481: a synced devices row → the shared `SteerDevice` shape every
 * picker/list consumer already speaks. `online` is stamped from
 * `last_seen_at` freshness; `owner` is set iff the row belongs to someone
 * else (a teammate's shared server). */
export function steerDeviceFromRow(
  row: Device,
  opts: { now: Date; currentUserId: string; ownerName?: string }
): SteerDevice {
  const updateRequested = row.updateRequestedAt !== null
  return {
    rowId: row.id,
    deviceId: row.deviceId,
    deviceLabel: row.label,
    kind: row.kind === `server` ? `server` : `desktop`,
    platform: row.platform,
    agents: row.agents,
    unauthedAgents: row.unauthedAgents,
    caps: row.caps,
    launchDefaults: row.launchDefaults ?? undefined,
    online: deviceRowIsOnline(row.lastSeenAt, opts.now),
    lastSeenAt: new Date(row.lastSeenAt).toISOString(),
    registered: true,
    version: row.version,
    updateRequested,
    updateBlocked: updateRequested && row.activeSessions > 0,
    sharedTeamId: row.sharedTeamId,
    // EXP-622: a default belongs to the row's OWNER — never surface a
    // teammate's shared server as the caller's default.
    isDefault: row.userId === opts.currentUserId && row.isDefault,
    ...(row.userId === opts.currentUserId
      ? {}
      : { owner: { id: row.userId, name: opts.ownerName ?? `` } }),
  }
}

/** EXP-481: compose the device list from synced rows — own rows first, then
 * servers shared with `teamId`. Within
 * each group online machines lead, sorted by label so heartbeats can't
 * reorder them (EXP-623); offline rows don't beat, so last-seen desc is
 * stable there. */
export function composeDeviceList(
  rows: Device[],
  usersById: Map<string, Pick<User, `id` | `name`>>,
  now: Date,
  currentUserId: string,
  teamId?: string
): SteerDevice[] {
  const stableOrder = (a: Device, b: Device) => {
    const aOnline = deviceRowIsOnline(a.lastSeenAt, now)
    const bOnline = deviceRowIsOnline(b.lastSeenAt, now)
    if (aOnline !== bOnline) return aOnline ? -1 : 1
    if (!aOnline) {
      const byLastSeen =
        new Date(b.lastSeenAt).getTime() - new Date(a.lastSeenAt).getTime()
      if (byLastSeen !== 0) return byLastSeen
    }
    const byLabel = a.label
      .toLocaleLowerCase()
      .localeCompare(b.label.toLocaleLowerCase())
    if (byLabel !== 0) return byLabel
    return a.deviceId.localeCompare(b.deviceId)
  }
  const own = rows
    .filter((row) => row.userId === currentUserId)
    .sort(stableOrder)
  const shared = teamId
    ? rows
        .filter(
          (row) =>
            row.userId !== currentUserId &&
            row.sharedTeamId === teamId &&
            row.kind === `server`
        )
        .sort(stableOrder)
    : []
  return [...own, ...shared].map((row) =>
    steerDeviceFromRow(row, {
      now,
      currentUserId,
      ownerName: usersById.get(row.userId)?.name,
    })
  )
}

/** EXP-481: the synced worktree row that makes "Resume previous session"
 * offerable for (device, issue, agent) — same device row, matching issue
 * identifier (case-insensitive), and either no recorded agents (pre-marker
 * worktree: any agent may resume) or the chosen agent among them. */
export function resumeWorktree(
  worktrees: SyncedDeviceWorktree[],
  deviceRowId: string | undefined,
  issueIdentifier: string,
  agent: string
): SyncedDeviceWorktree | null {
  if (!deviceRowId) return null
  const wanted = issueIdentifier.toLowerCase()
  return (
    worktrees.find(
      (worktree) =>
        worktree.deviceRowId === deviceRowId &&
        worktree.issueIdentifier?.toLowerCase() === wanted &&
        (worktree.agents === null ||
          worktree.agents.length === 0 ||
          worktree.agents.includes(agent))
    ) ?? null
  )
}

/** The device's reported version compares below the platform's
 * `CLIENT_LATEST_VERSION_*` value. Unknown or unparsable on either side =
 * false (EXP-420: never claim an update exists without evidence). */
export function deviceUpdateAvailable(
  version: string | null | undefined,
  latest: string | null | undefined
): boolean {
  if (!version || !latest) return false
  const have = parseVersionTuple(version)
  const want = parseVersionTuple(latest)
  if (!have || !want) return false
  for (let i = 0; i < 3; i++) {
    if (have[i] !== want[i]) return have[i] < want[i]
  }
  return false
}

/** EXP-420: the machine row's Update button renders only for an online
 * registered server that is actually behind the known latest CLI version —
 * or that has an update already in flight (so "Updating…"/"Queued" stays
 * visible until the re-register carries the new version). Latest unknown
 * (`CLIENT_LATEST_VERSION_CLI` unset) = no button. */
export function showDeviceUpdateButton(
  device: SteerDevice,
  latest: string | null | undefined
): boolean {
  return (
    device.kind === `server` &&
    deviceIsOnline(device) &&
    device.registered === true &&
    (deviceUpdateAvailable(device.version, latest) ||
      device.updateRequested === true)
  )
}
