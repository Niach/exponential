import { contract } from "@exp/domain-contract"

import type { Device, SyncedDeviceWorktree, User } from "@/db/schema"
import { parseVersionTuple } from "./client-version"
import type { AgentLaunchDefaults } from "./coding-launch-prefs"

// The caller's machines (EXP-403). Since EXP-481 the primary source is the
// synced `devices` shape (`steerDeviceFromRow`/`composeDeviceList` below —
// online-ness derives from `last_seen_at` freshness client-side); the legacy
// `devices.list` merge shape still fits, so every existing consumer keeps
// compiling. Rows straight off the relay (`steer.myDevices`) also fit: the
// registry fields are optional and absent-`online` reads as online.

export interface SteerDevice {
  /** EXP-481: the synced devices row id — joins `device_worktrees` rows.
   * Absent on legacy `devices.list` / relay-presence rows. */
  rowId?: string
  deviceId: string
  deviceLabel: string
  /** Relay presence timestamp — absent on registry-only (offline) rows. */
  connectedAt?: number
  /** EXP-201: agent CLIs installed on the device; absent = claude-only.
   * Since EXP-409: RUNNABLE (installed AND signed in) — explicitly empty
   * means the machine can run nothing right now. */
  agents?: string[]
  /** EXP-409: agents installed but signed out on the machine — never
   * offered in pickers; shown with a "sign in" hint instead. */
  unauthedAgents?: string[]
  /** EXP-253: launch capabilities beyond issue coding (e.g. `actions`);
   * absent = an older desktop with none. */
  caps?: string[]
  /** EXP-403 registry fields (devices.list). */
  kind?: `desktop` | `server`
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

/** EXP-432: whether the row is one of the caller's own machines (teammates'
 * shared rows carry `owner`; own rows never do). */
export function deviceIsMine(device: SteerDevice): boolean {
  return !device.owner
}

/** Whether the device is startable right now. Rows without the field come
 * straight off the relay presence map and are online by construction. */
export function deviceIsOnline(device: SteerDevice): boolean {
  return device.online !== false
}

/** Agents the device can run — an ABSENT advertisement means claude-only
 * (pre-EXP-201 sender), but an explicitly EMPTY one means the machine can
 * run nothing right now (EXP-409: every installed agent is signed out). */
export function deviceAgentIds(device: SteerDevice | undefined): string[] {
  if (!device?.agents) return [`claude`]
  return device.agents.filter((a) => contract.codingAgent.values.includes(a))
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

/** Only desktops advertising this capability have an action launch path —
 * older builds can run claude but not actions (`steer.startSession` enforces
 * the same server-side). */
export function deviceCanRunActions(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`actions`)
}

/** Builtin or inputs-carrying action runs additionally need this capability
 * (EXP-257) — an older desktop would silently drop the inputs field and run
 * a valueless prompt (`steer.startSession` enforces the same server-side). */
export function deviceCanRunActionInputs(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`action-inputs`)
}

/** The builtin "Fix merge conflicts" run needs this capability (EXP-259) —
 * `steer.startSession` rejects the builtin without it, so filter such desktops
 * out of the picker instead of failing after submit (EXP-323). */
export function deviceCanFixConflicts(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`fix-conflicts`)
}

/** EXP-615: the hidden chat builtin needs its own launch path on the device —
 * an older desktop would treat the id as a real action and fail its fetch. */
export function deviceCanChat(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`chat`)
}

/** EXP-530: only devices advertising this capability evaluate action
 * triggers locally — older builds would accept the binding and never fire.
 * Offline-but-capable devices stay pickable (they catch up on reconnect). */
export function deviceCanRunAutomations(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`automations`)
}

/** EXP-481: remote resume needs this capability — an older build would
 * silently drop the flag and start fresh (`steer.startSession` gates on the
 * persisted row's caps server-side). */
export function deviceCanResume(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`resume`)
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
    ...(row.userId === opts.currentUserId
      ? {}
      : { owner: { id: row.userId, name: opts.ownerName ?? `` } }),
  }
}

/** EXP-481: compose the device list from synced rows — own rows first, then
 * servers shared with `teamId` (the legacy `devices.list` grouping). Within
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
