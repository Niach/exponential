// EXP-403 registered devices: desktops and headless `exponential` daemon
// servers register themselves per user and heartbeat `last_seen_at`.
// Since EXP-481 the registry is SERVER-AUTHORITATIVE device state and an
// Electric shape (devices + device_worktrees; see routes/api/shapes/):
// `launch_defaults` is the canonical copy of a machine's agent defaults (its
// local settings.json converges), `device_worktrees` mirrors its worktree
// inventory, and `device_commands` queues owner→device work (worktree
// remove/prune) delivered on the heartbeat plus a best-effort relay
// `check_in` nudge. Clients read the shapes and derive online-ness from
// last_seen_at freshness (EXP-639 retired the `list` procedure). Since
// EXP-485 `register` is the SOLE agents/caps/unauthedAgents/acpAgents writer
// (the relay online frame no longer advertises them).
// EXP-432 bends the per-user rule exactly once: a server device may be SHARED
// with teams (`shared_team_ids`, owner-toggled via `setShared`; FEED-33 made
// it a set) so their members can remote-start on it.
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import {
  and,
  arrayContains,
  asc,
  desc,
  eq,
  inArray,
  ne,
  sql,
} from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import {
  router,
  authedProcedure,
  generateTxId,
  type Context,
} from "@/lib/trpc"
import {
  automations,
  deviceAgentAccountsSchema,
  deviceAgentHealthValues,
  deviceAgentUsageSchema,
  deviceCommands,
  deviceLaunchDefaultsSchema,
  devices,
  deviceWorktrees,
  MAX_AGENT_PROFILES,
  teamMembers,
  users,
  type DeviceAgentAccount,
  type DeviceAgentAccounts,
  type DeviceAgentHealth,
  type DeviceAgentLaunchDefaults,
  type DeviceAgentProfileEntry,
  type DeviceAgentUsage,
  type DeviceAgentUsageMap,
  type DeviceLaunchDefaults,
  type DeviceUsageWindow,
} from "@/db/schema"
import { assertTeamMember, getTeamMember } from "@/lib/team-membership"
import { versionPayload } from "@/lib/client-version"
import { endForeignHostedSessions } from "@/lib/coding-session-kill"
import {
  agentAllowsBlankModel,
  agentEffortValues,
  agentModelValues,
  agentSupportsPlanMode,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"
import { applyMcpOauthCommandCompletion } from "@/lib/mcp-oauth/flows"
import {
  applyReadinessReport,
  mcpReadinessEntriesSchema,
} from "@/lib/mcp-oauth/readiness"

// Mirrors the relay's online-frame bounds (steer-relay protocol.ts): the
// relay is a dumb pipe and the same strings land here via `register`. Caps
// are free strings the executor names (coding doctor.rs DEVICE_CAPS +
// ACTION_CAPS); the ones this router gates on: `agent-login`,
// `account-switch` (EXP-849: honours `account` on a live-run resume), `mcp`
// (EXP-792: runs `mcp_oauth_*` and reports readiness), `agent-usage-refresh`
// (EXP-747 C4), `update-now` (FEED-36: runs `update_now`) and
// `account-remove` (EXP-862: runs `agent_profile_remove`). The daemon
// advertises 18 today (11 build + 7 action caps), so the ceiling sits at 24
// with headroom, not AT the count.
const agentsInput = z.array(z.string().min(1).max(32)).max(16)
const capsInput = z.array(z.string().min(1).max(32)).max(24)

/** EXP-765: the longest `agent_login_code` a requester may hand a machine.
 * claude's authorization codes are ~80 chars of `code#state`; the cap only
 * has to keep a paste of the wrong thing from becoming a 2000-char payload. */
const AGENT_LOGIN_CODE_MAX = 512
const deviceIdInput = z.string().min(1).max(128)
const codingAgentValues = contract.codingAgent.values as [string, ...string[]]

// Heartbeats deliver at most this many pending commands per cycle — rows stay
// `pending` until completeCommand, so a missed cycle redelivers for free.
const COMMANDS_PER_HEARTBEAT = 32

// EXP-481: field-wise vocabulary clamp for launch defaults. ALWAYS clamps
// (never rejects) so version skew — a device or client with a newer/older
// model vocabulary — degrades a single field instead of failing the whole
// write; UI clients pre-clamp via agentSeed anyway. Unknown agents, invalid
// models/efforts, and capability-masked toggles are dropped.
function clampLaunchDefaults(
  input: z.infer<typeof deviceLaunchDefaultsSchema>
): DeviceLaunchDefaults {
  const agentIds = contract.codingAgent.values as readonly string[]
  const out: DeviceLaunchDefaults = {}
  if (input.defaultAgent && agentIds.includes(input.defaultAgent)) {
    out.defaultAgent = input.defaultAgent
  }
  if (input.agents) {
    const agents: Record<string, DeviceAgentLaunchDefaults> = {}
    for (const [agent, d] of Object.entries(input.agents)) {
      if (!agentIds.includes(agent) || !d) continue
      const entry: DeviceAgentLaunchDefaults = {}
      if (
        typeof d.model === `string` &&
        (agentModelValues(agent).includes(d.model) ||
          (d.model === `` && agentAllowsBlankModel(agent)))
      ) {
        entry.model = d.model
      }
      if (
        typeof d.effort === `string` &&
        (d.effort === `` || agentEffortValues(agent).includes(d.effort))
      ) {
        entry.effort = d.effort
      }
      // `typeof === boolean`, not `!== undefined`: the nullish schema lets
      // 0.14.10's explicit-null toggles through (EXP-495) and stored jsonb
      // must stay null-free (native clients parse it off the devices shape).
      if (typeof d.ultracode === `boolean` && agentSupportsUltracode(agent)) {
        entry.ultracode = d.ultracode
      }
      if (typeof d.planMode === `boolean` && agentSupportsPlanMode(agent)) {
        entry.planMode = d.planMode
      }
      if (Object.keys(entry).length > 0) agents[agent] = entry
    }
    if (Object.keys(agents).length > 0) out.agents = agents
  }
  return out
}

// EXP-849: the three agent-ID LISTS `register` writes, under the same
// ALWAYS-CLAMP contract as the launch defaults above. A fleet still on an
// older build keeps heart-beating a RETIRED id (`pi`, dropped from contract
// `codingAgent`) and must keep registering — minus that id. Register is the
// sole writer of `agents`/`acp_agents`/`unauthed_agents`, so this is the one
// place that keeps every picker, chip and tab on the four clients
// contract-clean; the stored jsonb never carries a name a client has no
// vocabulary for.
function clampAgentIds(input: readonly string[] | undefined): string[] {
  const agentIds = contract.codingAgent.values as readonly string[]
  return (input ?? []).filter((agent) => agentIds.includes(agent))
}

// EXP-484 bounds: the device reports at most one entry per contract agent,
// and a window list a bar can actually render.
const MAX_STATUS_AGENTS = 3
const MAX_USAGE_WINDOWS = 10
const MAX_USAGE_KEY = 64
const MAX_USAGE_LABEL = 32
const MAX_ACCOUNT_EMAIL = 320
const MAX_ACCOUNT_PLAN = 64
const MAX_PROFILE_ID = 64
const MAX_PROFILE_LABEL = 64

// ISO-normalize a device-reported timestamp; anything unparsable degrades to
// null (the presentation layer treats a missing stamp as unknown, never as
// "now").
function isoStampOrNull(value: unknown): string | null {
  if (typeof value !== `string` || value.length === 0) return null
  const at = new Date(value)
  return Number.isNaN(at.getTime()) ? null : at.toISOString()
}

// EXP-484: same contract as clampLaunchDefaults — ALWAYS clamp, never reject.
// A machine whose agent vocabulary drifts (or whose nullish fields arrive as
// explicit null, EXP-495) must lose a field, not its whole heartbeat. Agents
// outside contract `codingAgent` are dropped; stored copies are null-free.
export function clampAgentAccounts(
  input: z.infer<typeof deviceAgentAccountsSchema>
): DeviceAgentAccounts {
  const agentIds = contract.codingAgent.values as readonly string[]
  const out: DeviceAgentAccounts = {}
  for (const [agent, account] of Object.entries(input)) {
    if (!agentIds.includes(agent) || !account) continue
    if (Object.keys(out).length >= MAX_STATUS_AGENTS) break
    const entry: DeviceAgentAccount = { signedIn: account.signedIn === true }
    if (typeof account.email === `string` && account.email.length > 0) {
      entry.email = account.email.slice(0, MAX_ACCOUNT_EMAIL)
    }
    if (typeof account.plan === `string` && account.plan.length > 0) {
      entry.plan = account.plan.slice(0, MAX_ACCOUNT_PLAN)
    }
    const checkedAt = isoStampOrNull(account.checkedAt)
    if (checkedAt) entry.checkedAt = checkedAt
    const health = clampAgentHealth(account.health)
    if (health) entry.health = health
    // EXP-792 (EXP-747 B5): the device's profiles for this agent,
    // ≤`MAX_AGENT_PROFILES`. A profile without an id is dropped (nothing
    // could address it); the top-level fields above stay the ACTIVE profile
    // for older clients.
    const profiles: DeviceAgentProfileEntry[] = []
    for (const profile of account.profiles ?? []) {
      if (!profile || typeof profile.id !== `string` || !profile.id) continue
      if (profiles.length >= MAX_AGENT_PROFILES) break
      const item: DeviceAgentProfileEntry = {
        id: profile.id.slice(0, MAX_PROFILE_ID),
        signedIn: profile.signedIn === true,
      }
      if (typeof profile.label === `string` && profile.label.length > 0) {
        item.label = profile.label.slice(0, MAX_PROFILE_LABEL)
      }
      if (typeof profile.email === `string` && profile.email.length > 0) {
        item.email = profile.email.slice(0, MAX_ACCOUNT_EMAIL)
      }
      if (typeof profile.plan === `string` && profile.plan.length > 0) {
        item.plan = profile.plan.slice(0, MAX_ACCOUNT_PLAN)
      }
      if (profile.active === true) item.active = true
      const profileCheckedAt = isoStampOrNull(profile.checkedAt)
      if (profileCheckedAt) item.checkedAt = profileCheckedAt
      const profileHealth = clampAgentHealth(profile.health)
      if (profileHealth) item.health = profileHealth
      // EXP-849: the device collects no usage numbers for this login (past
      // its own probe cap). Kept so the clients can caption the row instead
      // of rendering its absent bars as zero; false is simply absent.
      if (profile.unmonitored === true) item.unmonitored = true
      if (profile.usage) {
        item.usage = clampUsageEntry(profile.usage, new Date())
      }
      profiles.push(item)
    }
    if (profiles.length > 0) entry.profiles = profiles
    out[agent] = entry
  }
  return out
}

// EXP-849: the account/profile health vocabulary — one of the four values or
// nothing at all. A value this build has no name for is DROPPED (the clients
// then fall back to deriving health from `signedIn`), never a rejection: the
// whole point of the clamp is that a newer device keeps its heartbeat.
function clampAgentHealth(value: unknown): DeviceAgentHealth | null {
  return typeof value === `string` &&
    (deviceAgentHealthValues as readonly string[]).includes(value)
    ? (value as DeviceAgentHealth)
    : null
}

// EXP-484: as above for the usage windows. `percent` rounds and clamps to
// 0-100 (a bar can't render 137%), key/label truncate, `resetsAt` is ISO or
// null, and a window without a key or label is dropped. `fetchedAt` falls
// back to `now` — the device is reporting what it just read, and the
// presentation layer's freshness rule keys on it.
export function clampAgentUsage(
  input: z.infer<typeof deviceAgentUsageSchema>,
  now: Date
): DeviceAgentUsageMap {
  const agentIds = contract.codingAgent.values as readonly string[]
  const out: DeviceAgentUsageMap = {}
  for (const [agent, usage] of Object.entries(input)) {
    if (!agentIds.includes(agent) || !usage) continue
    if (Object.keys(out).length >= MAX_STATUS_AGENTS) break
    out[agent] = clampUsageEntry(usage, now)
  }
  return out
}

// One agent's usage read, the same bounds whether it rides the top-level
// `agent_usage` map or a profile entry (EXP-792).
function clampUsageEntry(
  usage: NonNullable<z.infer<typeof deviceAgentUsageSchema>[string]>,
  now: Date
): DeviceAgentUsage {
  const windows: DeviceUsageWindow[] = []
  for (const window of usage.windows ?? []) {
    if (!window) continue
    if (windows.length >= MAX_USAGE_WINDOWS) break
    const key =
      typeof window.key === `string` ? window.key.slice(0, MAX_USAGE_KEY) : ``
    const label =
      typeof window.label === `string`
        ? window.label.slice(0, MAX_USAGE_LABEL)
        : ``
    if (key.length === 0 || label.length === 0) continue
    const raw = typeof window.percent === `number` ? window.percent : 0
    const percent = Number.isFinite(raw)
      ? Math.min(100, Math.max(0, Math.round(raw)))
      : 0
    windows.push({
      key,
      label,
      percent,
      resetsAt: isoStampOrNull(window.resetsAt),
    })
  }
  return {
    fetchedAt: isoStampOrNull(usage.fetchedAt) ?? now.toISOString(),
    stale: usage.stale === true,
    windows,
  }
}

// Best-effort, fire-and-forget: persisted state is the durable path, the
// nudge only kills heartbeat-pickup latency for online devices. Exported for
// the EXP-792 MCP OAuth relays (mcp-servers.ts, the anonymous callback).
export function nudgeDevice(ownerId: string, deviceId: string): void {
  const config = getSteerRelayConfig()
  if (!config) return
  void relayPostNudge(config, ownerId, deviceId).catch(() => {})
}

/** EXP-862: command kinds whose queued row is a WISH, not an act — asking
 * twice asks for the same end state, so a duplicate reuses the pending row
 * instead of failing the mutation with "That command is already queued". */
const IDEMPOTENT_COMMAND_KINDS: ReadonlySet<string> = new Set([
  `agent_usage_refresh`,
  `worktree_prune`,
  `agent_profile_use`,
  `agent_profile_remove`,
])

/** EXP-862: the ambient login's profile id — the agent CLI's own config dir,
 * which Exponential never created and never deletes. Blank counts as the
 * ambient login too (the device reads a missing `account` that way). */
function isSystemProfileId(profileId: string | undefined): boolean {
  const id = (profileId ?? ``).trim()
  return id.length === 0 || id === `system`
}

/** EXP-862: whether the device's last heartbeat reported `profileId` as one of
 * `agent`'s logins. A removal is destructive on the machine, so its target has
 * to be a row the requester could actually see. */
function deviceReportsProfile(
  accounts: DeviceAgentAccounts | null,
  agent: string,
  profileId: string
): boolean {
  return (accounts?.[agent]?.profiles ?? []).some(
    (profile) => profile.id === profileId
  )
}

/** FEED-36: `update_now` needs a daemon that knows the kind — an older build
 * would leave the row pending forever, so both queue paths refuse instead. */
function assertUpdateNowCap(row: { caps: string[] | null }): void {
  if (!(row.caps ?? []).includes(`update-now`)) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This machine's daemon doesn't support Update now yet`,
    })
  }
}

/**
 * The ONE owner→device enqueue (EXP-481): one pending command per (device,
 * kind, payload) — a double-click must not queue the same prune twice — then
 * the insert and the relay nudge. `createCommand` surfaces a duplicate as
 * CONFLICT; `requestUpdate({endSessions})` reuses the pending row instead
 * (FEED-36: a second "Update now" click is the same wish, not an error).
 */
async function queueDeviceCommand(
  db: Context[`db`],
  args: {
    deviceRowId: string
    userId: string
    deviceId: string
    kind: string
    payload: Record<string, string>
    onDuplicate: `conflict` | `reuse`
  }
): Promise<{ id: string }> {
  const [dup] = await db
    .select({ id: deviceCommands.id })
    .from(deviceCommands)
    .where(
      and(
        eq(deviceCommands.deviceRowId, args.deviceRowId),
        eq(deviceCommands.kind, args.kind),
        eq(deviceCommands.status, `pending`),
        sql`${deviceCommands.payload} = ${JSON.stringify(args.payload)}::jsonb`
      )
    )
    .limit(1)
  if (dup) {
    if (args.onDuplicate === `reuse`) return { id: dup.id }
    throw new TRPCError({
      code: `CONFLICT`,
      message: `That command is already queued`,
    })
  }

  const [command] = await db
    .insert(deviceCommands)
    .values({
      deviceRowId: args.deviceRowId,
      userId: args.userId,
      kind: args.kind,
      payload: args.payload,
    })
    .returning({ id: deviceCommands.id })
  nudgeDevice(args.userId, args.deviceId)
  return { id: command.id }
}

// ISO-or-null of a nullable timestamp — the CAS stamp wire form (equality
// compare on the echoed string; never `>`, no clock-skew semantics).
function stampOf(value: Date | null): string | null {
  return value ? value.toISOString() : null
}

// EXP-639: the ONE "which device rows may this user see" read — their own
// registrations (any kind), plus the SERVER devices teammates shared with
// `teamId` (EXP-432). The team_members join drops ghost shares whose owner has
// since left the team: `shared_team_ids` survives membership changes, but an
// ex-member's box must neither list nor run. Owner names ride along for the
// shared rows only — own rows never render one.
export async function visibleDeviceRows(
  db: Context[`db`],
  userId: string,
  teamId?: string
): Promise<{
  rows: Array<typeof devices.$inferSelect>
  ownerNames: Map<string, { id: string; name: string }>
}> {
  const own = await db.select().from(devices).where(eq(devices.userId, userId))
  if (!teamId) return { rows: own, ownerNames: new Map() }
  const shared = await db
    .select({ device: devices, ownerName: users.name })
    .from(devices)
    .innerJoin(users, eq(users.id, devices.userId))
    .innerJoin(
      teamMembers,
      and(
        eq(teamMembers.userId, devices.userId),
        eq(teamMembers.teamId, teamId)
      )
    )
    .where(
      and(
        arrayContains(devices.sharedTeamIds, [teamId]),
        eq(devices.kind, `server`),
        ne(devices.userId, userId)
      )
    )
  return {
    rows: [...own, ...shared.map((row) => row.device)],
    ownerNames: new Map(
      shared.map((row) => [
        row.device.userId,
        { id: row.device.userId, name: row.ownerName },
      ])
    ),
  }
}

/** FEED-33: the share set after a `devices.setShared` call — sorted + deduped
 * (the array is shape data, so a stable order keeps a no-op write a no-op).
 * `shared` moves ONE team in or out of the set; `teamId: null` with
 * `shared: false` clears it. */
export function nextSharedTeamIds(
  current: readonly string[],
  input: { teamId: string | null; shared: boolean }
): string[] {
  let next: string[]
  if (input.shared) {
    next = input.teamId ? [...current, input.teamId] : [...current]
  } else {
    next = input.teamId
      ? current.filter((teamId) => teamId !== input.teamId)
      : []
  }
  return [...new Set(next)].sort()
}

export const devicesRouter = router({
  // Upsert on (user, deviceId): control-channel start (desktop) and daemon
  // start (CLI) both call this, refreshing kind/platform/agents/caps. The
  // `label` only SEEDS the row — a re-register must not stomp a user's
  // rename with the hostname default (explicit renames, incl. the daemon's
  // `--label`, go through `rename`).
  register: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        label: z.string().min(1).max(255),
        kind: z.enum([`desktop`, `server`]),
        platform: z.string().min(1).max(64).optional(),
        agents: agentsInput.optional(),
        caps: capsInput.optional(),
        // EXP-481: persisted since the shapes landed (the Rust clients sent
        // it all along; the old zod silently stripped it).
        unauthedAgents: agentsInput.optional(),
        // EXP-749: the subset of `agents` this build can drive over ACP.
        // ABSENT WRITES NULL on purpose — register is the sole writer, so an
        // older build re-registering must return the row to "unknown" (every
        // runnable agent assumed ACP-ready) rather than keep a newer build's
        // stale list.
        acpAgents: agentsInput.optional(),
        // EXP-481: the device's local defaults, applied ONLY as a first-ever
        // seed (row column NULL) — after that the server copy is
        // authoritative and the setLaunchDefaults CAS decides.
        launchDefaults: deviceLaunchDefaultsSchema.optional(),
        // EXP-484: the machine's per-agent sign-in status from its doctor
        // probe. ABSENT means the probe resolved no agent CLI at all (no
        // install, or a probe that failed open), never "signed out" — so it
        // leaves the column untouched on conflict rather than blanking a
        // good answer on one flaky pass.
        agentAccounts: deviceAgentAccountsSchema.optional(),
        version: z.string().min(1).max(32).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const now = new Date()
      const [row] = await ctx.db
        .insert(devices)
        .values({
          userId: ctx.session.user.id,
          deviceId: input.deviceId,
          label: input.label,
          kind: input.kind,
          platform: input.platform ?? null,
          agents: clampAgentIds(input.agents),
          caps: input.caps ?? [],
          unauthedAgents: clampAgentIds(input.unauthedAgents),
          acpAgents: input.acpAgents ? clampAgentIds(input.acpAgents) : null,
          launchDefaults: input.launchDefaults
            ? clampLaunchDefaults(input.launchDefaults)
            : null,
          launchDefaultsUpdatedAt: input.launchDefaults ? now : null,
          agentAccounts: input.agentAccounts
            ? clampAgentAccounts(input.agentAccounts)
            : null,
          version: input.version ?? null,
          lastSeenAt: now,
        })
        .onConflictDoUpdate({
          target: [devices.userId, devices.deviceId],
          set: {
            kind: input.kind,
            platform: input.platform ?? null,
            agents: clampAgentIds(input.agents),
            caps: input.caps ?? [],
            unauthedAgents: clampAgentIds(input.unauthedAgents),
            acpAgents: input.acpAgents ? clampAgentIds(input.acpAgents) : null,
            // Seed-only-when-NULL: a re-register must never stomp
            // server-side edits (the device converges via heartbeat instead).
            ...(input.launchDefaults
              ? {
                  launchDefaults: sql`COALESCE(${devices.launchDefaults}, ${JSON.stringify(clampLaunchDefaults(input.launchDefaults))}::jsonb)`,
                  launchDefaultsUpdatedAt: sql`COALESCE(${devices.launchDefaultsUpdatedAt}, ${now.toISOString()}::timestamptz)`,
                }
              : {}),
            // Absent = the probe had nothing to say (see the input doc):
            // keep whatever the last reporting register wrote.
            ...(input.agentAccounts
              ? { agentAccounts: clampAgentAccounts(input.agentAccounts) }
              : {}),
            version: input.version ?? null,
            // Registering CONSUMES a pending Update click: the daemon
            // re-registers after acting on the request (whether or not a
            // newer build actually existed). `active_sessions` stays
            // heartbeat-owned — a doctor-driven re-register can fire while
            // sessions are live, and zeroing it here would lie (EXP-411).
            updateRequestedAt: null,
            lastSeenAt: now,
            updatedAt: now,
          },
        })
        .returning({
          launchDefaults: devices.launchDefaults,
          launchDefaultsUpdatedAt: devices.launchDefaultsUpdatedAt,
        })
      // Return the (post-seed) server copy so the device converges
      // immediately instead of waiting a heartbeat.
      return {
        ok: true,
        launchDefaults: row?.launchDefaults ?? null,
        launchDefaultsUpdatedAt: stampOf(row?.launchDefaultsUpdatedAt ?? null),
      }
    }),

  // Liveness bump. `ok: false` means the row is gone (removed from the UI
  // while the daemon ran) — the caller should re-register.
  // `updateRequested` piggybacks the web "Update" button to the daemon: it
  // checks for a new release, updates + restarts when one exists, and its
  // next register consumes the flag either way. While sessions are live the
  // daemon defers the update and reports the count back here
  // (`activeSessions`, EXP-411) so `list` can say "queued" instead of
  // letting the spinner run forever.
  // EXP-481: the heartbeat is also the device's WORK PULL — one round trip
  // carries pending commands and (when the device's converged stamp differs)
  // the authoritative launch defaults. A relay `check_in` nudge just means
  // "heartbeat now". Both fields are REQUIRED since EXP-485 — the pre-EXP-411
  // and pre-EXP-481 daemon tolerances retired with the fleet.
  heartbeat: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        activeSessions: z.number().int().min(0).max(1000),
        // The launch-defaults stamp the device last converged to (null =
        // never).
        defaultsSyncedAt: z.string().datetime().nullable(),
        // EXP-484: the collector's latest read, sent only when it CHANGED
        // (the device compares against what it last shipped) — so absent
        // means UNCHANGED and leaves the columns alone; it can never mean
        // "cleared". `agent_usage_at` moves with `agent_usage` only, and is
        // deliberately not a convergence trigger for anything.
        agentAccounts: deviceAgentAccountsSchema.optional(),
        agentUsage: deviceAgentUsageSchema.optional(),
        // EXP-792: the device's MCP readiness per server, sent only when it
        // CHANGED (same absent-means-unchanged contract as the two above).
        mcpReadiness: mcpReadinessEntriesSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const now = new Date()
      const updated = await ctx.db
        .update(devices)
        .set({
          lastSeenAt: now,
          updatedAt: now,
          activeSessions: input.activeSessions,
          ...(input.agentAccounts
            ? { agentAccounts: clampAgentAccounts(input.agentAccounts) }
            : {}),
          ...(input.agentUsage
            ? {
                agentUsage: clampAgentUsage(input.agentUsage, now),
                agentUsageAt: now,
              }
            : {}),
        })
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .returning({
          id: devices.id,
          updateRequestedAt: devices.updateRequestedAt,
          launchDefaults: devices.launchDefaults,
          launchDefaultsUpdatedAt: devices.launchDefaultsUpdatedAt,
        })
      const row = updated[0]
      if (!row) return { ok: false, updateRequested: false }

      if (input.mcpReadiness) {
        await applyReadinessReport(
          ctx.db,
          {
            userId: ctx.session.user.id,
            deviceRowId: row.id,
            entries: input.mcpReadiness,
          },
          now
        )
      }

      const pending = await ctx.db
        .select({
          id: deviceCommands.id,
          kind: deviceCommands.kind,
          payload: deviceCommands.payload,
        })
        .from(deviceCommands)
        .where(
          and(
            eq(deviceCommands.deviceRowId, row.id),
            eq(deviceCommands.status, `pending`)
          )
        )
        .orderBy(asc(deviceCommands.createdAt))
        .limit(COMMANDS_PER_HEARTBEAT)

      const serverStamp = stampOf(row.launchDefaultsUpdatedAt)
      const wantsDefaults = input.defaultsSyncedAt !== serverStamp
      return {
        ok: true,
        updateRequested: Boolean(row.updateRequestedAt),
        commands: pending,
        ...(wantsDefaults
          ? {
              launchDefaults: row.launchDefaults,
              launchDefaultsUpdatedAt: serverStamp,
            }
          : {}),
      }
    }),

  // The web "Update" button (EXP-403): flag the device; its next heartbeat
  // picks the request up. Own-user only via the where clause. FEED-36: a
  // queued update waits for the machine's live sessions (the daemon ends
  // idle ones after 2h); `endSessions: true` additionally queues an
  // `update_now` command, which ends EVERY live session there and restarts
  // on the new version. The cap is checked BEFORE the flag lands so a refusal
  // has no half-applied side effect the UI would then misreport as queued.
  requestUpdate: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        endSessions: z.boolean().default(false),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const ownRow = and(
        eq(devices.userId, ctx.session.user.id),
        eq(devices.deviceId, input.deviceId)
      )
      let row: { id: string; caps: string[] | null } | undefined
      if (input.endSessions) {
        const [found] = await ctx.db
          .select({ id: devices.id, caps: devices.caps })
          .from(devices)
          .where(ownRow)
          .limit(1)
        if (!found) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
        }
        assertUpdateNowCap(found)
        row = found
      }
      const now = new Date()
      const updated = await ctx.db
        .update(devices)
        .set({ updateRequestedAt: now, updatedAt: now })
        .where(ownRow)
        .returning({ id: devices.id })
      if (updated.length === 0) return { ok: false }
      if (row) {
        await queueDeviceCommand(ctx.db, {
          deviceRowId: row.id,
          userId: ctx.session.user.id,
          deviceId: input.deviceId,
          kind: `update_now`,
          payload: {},
          onDuplicate: `reuse`,
        })
      }
      return { ok: true }
    }),

  // EXP-481: edit a device's server-authoritative launch defaults. Owner-only
  // by construction (own-row where clause). Two caller classes:
  //  - UI edits (web/mobile/IDE settings) OMIT `expectedUpdatedAt` —
  //    unconditional last-write-wins between humans, offline device included
  //    (it converges on its next heartbeat).
  //  - DEVICE pushes (local settings.json edits) ALWAYS send
  //    `expectedUpdatedAt` (the stamp they last converged to; null = "server
  //    has none") — a stale stamp gets `conflict: true` plus the current
  //    server copy to adopt. Server wins offline-concurrent races,
  //    deterministically.
  setLaunchDefaults: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        launchDefaults: deviceLaunchDefaultsSchema,
        expectedUpdatedAt: z.string().datetime().nullable().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({
          id: devices.id,
          launchDefaults: devices.launchDefaults,
          launchDefaultsUpdatedAt: devices.launchDefaultsUpdatedAt,
        })
        .from(devices)
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .limit(1)
      if (!row) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
      }
      const serverStamp = stampOf(row.launchDefaultsUpdatedAt)
      if (
        input.expectedUpdatedAt !== undefined &&
        (input.expectedUpdatedAt ?? null) !== serverStamp
      ) {
        return {
          ok: false as const,
          conflict: true as const,
          launchDefaults: row.launchDefaults,
          launchDefaultsUpdatedAt: serverStamp,
          txid: null,
        }
      }
      const clamped = clampLaunchDefaults(input.launchDefaults)
      const now = new Date()
      const txid = await ctx.db.transaction(async (tx) => {
        const id = await generateTxId(tx)
        await tx
          .update(devices)
          .set({
            launchDefaults: clamped,
            launchDefaultsUpdatedAt: now,
            updatedAt: now,
          })
          .where(eq(devices.id, row.id))
        return id
      })
      nudgeDevice(ctx.session.user.id, input.deviceId)
      return {
        ok: true as const,
        launchDefaults: clamped,
        launchDefaultsUpdatedAt: now.toISOString(),
        txid,
      }
    }),

  // EXP-481: the device reports its FULL current worktree inventory;
  // diff-upserted so unchanged rows produce NO Electric op (a device may
  // re-report every heartbeat and stay sync-quiet at steady state).
  reportWorktrees: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        worktrees: z
          .array(
            z.object({
              repoFullName: z.string().min(1).max(255),
              branch: z.string().min(1).max(255),
              issueIdentifier: z.string().min(1).max(64).optional(),
              agents: agentsInput.optional(),
              // Unknown future vocabulary degrades to `unknown`, never a
              // failed report.
              dirty: z
                .enum([`clean`, `untracked`, `tracked`, `unknown`])
                .catch(`unknown`),
              busy: z.boolean(),
            })
          )
          .max(256),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({ id: devices.id })
        .from(devices)
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .limit(1)
      if (!row) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
      }

      // Dedupe by (repo, branch) then sort — deterministic upsert/lock order.
      // The key separator is a NUL, written as an ESCAPE: a literal control
      // byte in the source makes the whole file binary to grep/diff.
      const byKey = new Map<string, (typeof input.worktrees)[number]>()
      for (const wt of input.worktrees) {
        byKey.set(`${wt.repoFullName}\u0000${wt.branch}`, wt)
      }
      const reported = [...byKey.entries()]
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([, wt]) => wt)

      const now = new Date()
      await ctx.db.transaction(async (tx) => {
        for (const wt of reported) {
          // EXP-849: the same ALWAYS-CLAMP contract as `register`'s agent
          // lists — a worktree reported by a machine below the version floor
          // must not park a RETIRED id in the synced row (the clients have no
          // name or icon for one). Absent stays NULL; a list that clamps
          // empty stays an empty list, like `clampAgentIds` everywhere else.
          const agents = wt.agents ? clampAgentIds(wt.agents) : null
          await tx
            .insert(deviceWorktrees)
            .values({
              deviceRowId: row.id,
              // user_id/shared_team_ids are trigger-populated; the schema
              // marks user_id NOT NULL so satisfy the type with the caller
              // (the BEFORE INSERT trigger overwrites from the devices row).
              userId: ctx.session.user.id,
              repoFullName: wt.repoFullName,
              branch: wt.branch,
              issueIdentifier: wt.issueIdentifier ?? null,
              agents,
              dirty: wt.dirty,
              busy: wt.busy,
              reportedAt: now,
            })
            .onConflictDoUpdate({
              target: [
                deviceWorktrees.deviceRowId,
                deviceWorktrees.repoFullName,
                deviceWorktrees.branch,
              ],
              set: {
                issueIdentifier: wt.issueIdentifier ?? null,
                agents,
                dirty: wt.dirty,
                busy: wt.busy,
                reportedAt: now,
              },
              // The change guard: an identical re-report must not touch the
              // row (no Electric op, no reported_at churn).
              setWhere: sql`${deviceWorktrees.issueIdentifier} IS DISTINCT FROM ${wt.issueIdentifier ?? null} OR ${deviceWorktrees.agents} IS DISTINCT FROM ${agents === null ? null : JSON.stringify(agents)}::jsonb OR ${deviceWorktrees.dirty} IS DISTINCT FROM ${wt.dirty} OR ${deviceWorktrees.busy} IS DISTINCT FROM ${wt.busy}`,
            })
        }
        if (reported.length === 0) {
          await tx
            .delete(deviceWorktrees)
            .where(eq(deviceWorktrees.deviceRowId, row.id))
        } else {
          await tx
            .delete(deviceWorktrees)
            .where(
              and(
                eq(deviceWorktrees.deviceRowId, row.id),
                sql`(${deviceWorktrees.repoFullName}, ${deviceWorktrees.branch}) NOT IN (${sql.join(
                  reported.map((wt) => sql`(${wt.repoFullName}, ${wt.branch})`),
                  sql`, `
                )})`
              )
            )
        }
      })
      return { ok: true }
    }),

  // EXP-481: queue owner→device work. The device picks it up on its next
  // heartbeat (nudged immediately when online); an offline device runs it on
  // return — deliberately durable. EXP-484 adds `agent_login`: the device
  // drives the agent CLI's own login flow and completes the row EARLY with
  // the sign-in URL, which the requester polls for via `getCommand`. EXP-765
  // adds `agent_login_code`: claude's link hands the browser an authorization
  // code the CLI on the machine is still waiting for, and this is how the
  // requester hands it back — the device types it into that login PTY.
  // EXP-747 C4 adds `agent_usage_refresh`: force one profile's usage
  // collection past the shared TTL (never past the rate-limit floor).
  // FEED-36 adds `update_now` (payload {}): end every live session on the
  // machine and restart on the queued update, cap-gated on `update-now`. The
  // EXP-792 `mcp_oauth_start`/`mcp_oauth_code` kinds are queued INTERNALLY
  // only (mcpServers.beginOAuth, the anonymous callback) and never accepted
  // here — a caller could otherwise relay an arbitrary code to a device.
  createCommand: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        kind: z.enum([
          `worktree_remove`,
          `worktree_prune`,
          `agent_login`,
          `agent_login_code`,
          `agent_usage_refresh`,
          // EXP-849: make an already-signed-in profile the agent's ACTIVE
          // login on that machine. NON-DESTRUCTIVE: no logout, no login, no
          // credential is touched — the machine just points the agent at that
          // profile and re-heartbeats `agent_accounts`. (Never `codex
          // logout`: that revokes the account server-wide.)
          `agent_profile_use`,
          // EXP-862: delete THIS machine's copy of an agent login (its
          // profile dir and its index row). The ACCOUNT is untouched: the
          // device never runs `codex logout` (that revokes the account
          // server-wide), it only forgets the credential it holds.
          `agent_profile_remove`,
          `update_now`,
        ]),
        repoFullName: z.string().min(1).max(255).optional(),
        branch: z.string().min(1).max(255).optional(),
        // EXP-484 `agent_login` inputs (ignored by the other kinds): which
        // agent CLI to sign in, and whether to sign the current account out
        // first (Switch account).
        agent: z.enum(codingAgentValues).optional(),
        switch: z.boolean().optional(),
        // EXP-765 `agent_login_code`: the code the browser showed. Trimmed;
        // the cap is generous because it is opaque to us, and one line
        // because it is typed into a PTY as one line.
        code: z.string().trim().min(1).max(AGENT_LOGIN_CODE_MAX).optional(),
        // EXP-747 C4 `agent_usage_refresh`: which profile to re-read
        // (`system` = the ambient login). EXP-827: `agent_login` takes it
        // too (sign into that EXISTING profile), or `newProfileLabel` to
        // create a profile on the machine and sign into it — never both.
        profileId: z.string().min(1).max(64).optional(),
        newProfileLabel: z.string().trim().min(1).max(64).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({
          id: devices.id,
          caps: devices.caps,
          // EXP-862: `agent_profile_remove` is destructive on the device, so
          // its target must be a login the machine actually reported.
          agentAccounts: devices.agentAccounts,
        })
        .from(devices)
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .limit(1)
      if (!row) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
      }

      let payload: Record<string, string> = {}
      if (input.kind === `worktree_remove`) {
        if (!input.repoFullName || !input.branch) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `worktree_remove needs repoFullName and branch`,
          })
        }
        // Prevent garbage payloads: the target must be a currently-reported
        // worktree (the shape the issuing UI rendered from).
        const [wt] = await ctx.db
          .select({ id: deviceWorktrees.id })
          .from(deviceWorktrees)
          .where(
            and(
              eq(deviceWorktrees.deviceRowId, row.id),
              eq(deviceWorktrees.repoFullName, input.repoFullName),
              eq(deviceWorktrees.branch, input.branch)
            )
          )
          .limit(1)
        if (!wt) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `That worktree is no longer reported by the device`,
          })
        }
        payload = { repoFullName: input.repoFullName, branch: input.branch }
      }

      if (input.kind === `agent_login`) {
        if (!input.agent) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `agent_login needs an agent`,
          })
        }
        // The executor lives in the desktop app and the daemon, both of
        // which declare `agent-login` unconditionally (EXP-672 keeps the cap
        // as the contract): a row without it would leave the command pending
        // forever, so refuse instead of queueing one.
        if (!(row.caps ?? []).includes(`agent-login`)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That device does not declare the agent-login capability`,
          })
        }
        if (input.profileId && input.newProfileLabel) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `agent_login takes profileId or newProfileLabel, not both`,
          })
        }
        payload = {
          agent: input.agent,
          switch: input.switch === true ? `true` : `false`,
          ...(input.profileId ? { profileId: input.profileId } : {}),
          ...(input.newProfileLabel
            ? { newProfileLabel: input.newProfileLabel }
            : {}),
        }
      }

      if (input.kind === `agent_login_code`) {
        if (!input.agent || !input.code) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `agent_login_code needs an agent and a code`,
          })
        }
        // The device types this verbatim into a waiting PTY, so ANY control
        // byte is refused, not just newlines: `\x03`/`\x1b[A` would kill or
        // confuse the login while the command still reports CODE_ENTERED, and
        // a NUL additionally breaks the jsonb dedupe query below.
        if (/[\x00-\x1f\x7f]/.test(input.code)) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `The code must be a single line of printable characters`,
          })
        }
        payload = { agent: input.agent, code: input.code }
      }

      // EXP-849: "Use this account here" — same payload shape as
      // `agent_usage_refresh` (agent + profile). Two caps, because the
      // command needs both halves: `agent-login` to drive the agent's own
      // login state at all, and `account-switch` for the profile machinery
      // itself. A build missing either would leave the row pending forever.
      if (input.kind === `agent_profile_use`) {
        if (!input.agent || !input.profileId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `agent_profile_use needs an agent and a profileId`,
          })
        }
        const caps = row.caps ?? []
        if (!caps.includes(`agent-login`) || !caps.includes(`account-switch`)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That machine runs an older Exponential app that cannot switch agent accounts. Update it first.`,
          })
        }
        payload = { agent: input.agent, profileId: input.profileId }
      }

      // EXP-862: "Remove account" — the machine deletes its own copy of that
      // login. Two caps, like `agent_profile_use`: `agent-login` to drive the
      // machine's logins at all, `account-remove` for this command itself. A
      // build missing either would leave the row pending forever.
      if (input.kind === `agent_profile_remove`) {
        if (!input.agent || !input.profileId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `agent_profile_remove needs an agent and a profileId`,
          })
        }
        // The ambient login is the agent CLI's own, not ours to delete: it
        // has no profile dir to remove, and `codex logout` is never in this
        // path. The device refuses it too; refusing here keeps the round
        // trip off a machine that could only say no.
        if (isSystemProfileId(input.profileId)) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `That machine's own agent login cannot be removed from Exponential`,
          })
        }
        const caps = row.caps ?? []
        if (!caps.includes(`agent-login`) || !caps.includes(`account-remove`)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That machine runs an older Exponential app that cannot remove agent accounts. Update it first.`,
          })
        }
        if (!deviceReportsProfile(row.agentAccounts, input.agent, input.profileId)) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `That account is no longer reported by the device`,
          })
        }
        payload = { agent: input.agent, profileId: input.profileId }
      }

      if (input.kind === `agent_usage_refresh`) {
        if (!input.agent || !input.profileId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `agent_usage_refresh needs an agent and a profileId`,
          })
        }
        if (!(row.caps ?? []).includes(`agent-usage-refresh`)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That device does not declare the agent-usage-refresh capability`,
          })
        }
        payload = { agent: input.agent, profileId: input.profileId }
      }

      if (input.kind === `update_now`) assertUpdateNowCap(row)

      return queueDeviceCommand(ctx.db, {
        deviceRowId: row.id,
        userId: ctx.session.user.id,
        deviceId: input.deviceId,
        kind: input.kind,
        payload,
        // EXP-862: a second click on an IDEMPOTENT command is the same wish,
        // not an error — the queued row is reused instead of surfacing "That
        // command is already queued" as a failed mutation. A sign-in, a code
        // hand-off and a worktree removal keep the CONFLICT: those are acts,
        // and a repeat says something the requester needs to hear.
        onDuplicate: IDEMPOTENT_COMMAND_KINDS.has(input.kind)
          ? `reuse`
          : `conflict`,
      })
    }),

  // EXP-481: the device reports a command's outcome. Only pending rows
  // transition; a duplicate complete (heartbeat redelivery races the first
  // completion) is tolerated with ok:false rather than an error. EXP-792:
  // an `mcp_oauth_*` completion additionally advances the flow row the
  // command belongs to (lib/mcp-oauth/flows.ts) — the web dialog polls the
  // flow, never the command.
  completeCommand: authedProcedure
    .input(
      z.object({
        commandId: z.string().uuid(),
        ok: z.boolean(),
        message: z.string().max(2000).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const updated = await ctx.db
        .update(deviceCommands)
        .set({
          status: input.ok ? `done` : `failed`,
          result: input.message ?? null,
          completedAt: new Date(),
        })
        .where(
          and(
            eq(deviceCommands.id, input.commandId),
            eq(deviceCommands.userId, ctx.session.user.id),
            eq(deviceCommands.status, `pending`)
          )
        )
        .returning({
          id: deviceCommands.id,
          kind: deviceCommands.kind,
          payload: deviceCommands.payload,
        })
      const command = updated[0]
      if (command?.kind?.startsWith(`mcp_oauth_`)) {
        await applyMcpOauthCommandCompletion(ctx.db, {
          kind: command.kind,
          payload: command.payload ?? {},
          ok: input.ok,
          message: input.message,
        })
      }
      return { ok: updated.length > 0 }
    }),

  // EXP-481: the issuing UI's poll target while a command is in flight (the
  // material outcome additionally arrives via the device_worktrees shape when
  // the device re-reports).
  getCommand: authedProcedure
    .input(z.object({ commandId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const [command] = await ctx.db
        .select({
          id: deviceCommands.id,
          kind: deviceCommands.kind,
          payload: deviceCommands.payload,
          status: deviceCommands.status,
          result: deviceCommands.result,
          completedAt: deviceCommands.completedAt,
          createdAt: deviceCommands.createdAt,
        })
        .from(deviceCommands)
        .where(
          and(
            eq(deviceCommands.id, input.commandId),
            eq(deviceCommands.userId, ctx.session.user.id)
          )
        )
        .limit(1)
      if (!command) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Command not found` })
      }
      return command
    }),

  // EXP-481: recent command history for the device-settings view — keeps the
  // UI honest about failed prunes.
  listCommands: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        limit: z.number().int().min(1).max(50).default(20),
      })
    )
    .query(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({ id: devices.id })
        .from(devices)
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .limit(1)
      if (!row) return { commands: [] }
      const commands = await ctx.db
        .select({
          id: deviceCommands.id,
          kind: deviceCommands.kind,
          payload: deviceCommands.payload,
          status: deviceCommands.status,
          result: deviceCommands.result,
          completedAt: deviceCommands.completedAt,
          createdAt: deviceCommands.createdAt,
        })
        .from(deviceCommands)
        .where(eq(deviceCommands.deviceRowId, row.id))
        .orderBy(desc(deviceCommands.createdAt))
        .limit(input.limit)
      return { commands }
    }),

  // EXP-485: the informational CLIENT_LATEST_VERSION_* hint on its own, so a
  // client wanting the "update available" nudge no longer pays for the whole
  // registry (the devices shape delivers the rows). Same {desktop, cli}
  // shape as `list`'s envelope field, so every client reuses its decoder.
  latestVersions: authedProcedure.query(
    (): { desktop: string | null; cli: string | null } => {
      const payload = versionPayload() as Record<
        string,
        { latest: string | null }
      >
      return {
        desktop: payload.desktop?.latest ?? null,
        cli: payload.cli?.latest ?? null,
      }
    }
  ),

  // EXP-432: share/unshare one of the caller's SERVER devices with teams
  // they belong to. Sharing is the consent that lets teammates remote-start
  // on the box — the resulting sessions run under the owner's daemon but
  // belong to the requesting teammate (coding-sessions
  // `resolveStartAttribution`). FEED-33: the share is a SET of teams and
  // `shared` moves ONE team in or out of it — what every client's per-team
  // switch sends. `{teamId: null, shared: false}` clears the whole set.
  setShared: authedProcedure
    .input(
      z
        .object({
          deviceId: deviceIdInput,
          teamId: z.string().uuid().nullable(),
          shared: z.boolean(),
        })
        .refine((v) => v.shared === false || v.teamId !== null, {
          message: `teamId is required to add a share`,
        })
    )
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({
          id: devices.id,
          kind: devices.kind,
          sharedTeamIds: devices.sharedTeamIds,
        })
        .from(devices)
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .limit(1)
      if (!row) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
      }
      if (row.kind !== `server`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Only server machines can be shared`,
        })
      }
      const current = row.sharedTeamIds ?? []
      const next = nextSharedTeamIds(current, input)
      const added = next.filter((teamId) => !current.includes(teamId))
      const revoked = current.filter((teamId) => !next.includes(teamId))
      for (const teamId of added) {
        await assertTeamMember(ctx.session.user.id, teamId)
      }
      // EXP-530 follow-up: an automation bound to this device by one of a
      // REVOKED team's owners keeps firing on the device owner's credentials
      // once the share is withdrawn (there is no server scheduler — the
      // device self-selects automations off Electric), and the toggle is
      // owner-only, so the machine's owner cannot stop it. Withdrawing the
      // share disables those automations in the same transaction as the
      // column write. Skipped when the device owner is an OWNER of that
      // team: then the bindings are plausibly their own, and they keep the
      // toggle to undo them.
      const disarm: string[] = []
      for (const teamId of revoked) {
        const member = await getTeamMember(ctx.session.user.id, teamId)
        if (member?.role !== `owner`) disarm.push(teamId)
      }
      const txid = await ctx.db.transaction(async (tx) => {
        const id = await generateTxId(tx)
        await tx
          .update(devices)
          .set({ sharedTeamIds: next, updatedAt: new Date() })
          .where(eq(devices.id, row.id))
        if (disarm.length > 0) {
          await tx
            .update(automations)
            .set({ enabled: false, updatedAt: new Date() })
            .where(
              and(
                inArray(automations.teamId, disarm),
                eq(automations.deviceId, input.deviceId),
                // Already-off rows stay untouched (no needless Electric op).
                eq(automations.enabled, true)
              )
            )
        }
        return id
      })
      // EXP-445: withdrawing a share must end the runs it was the consent
      // for — otherwise a teammate's agent keeps working on this machine
      // with no client able to reach it. Ordered AFTER the column write on
      // purpose: once shared_team_ids has moved, resolveStartAttribution
      // refuses new foreign attributions, so nothing can slip in behind the
      // fan-out. Adding a team and a no-op toggle end nothing. Device-scoped
      // (EXP-560): only THIS machine's foreign runs die — the owner's other
      // same-team shares keep theirs.
      for (const teamId of revoked) {
        await endForeignHostedSessions(
          ctx.session.user.id,
          teamId,
          input.deviceId
        )
      }
      return { ok: true, txid }
    }),

  rename: authedProcedure
    .input(
      z.object({ deviceId: deviceIdInput, label: z.string().min(1).max(255) })
    )
    .mutation(async ({ ctx, input }) => {
      const txid = await ctx.db.transaction(async (tx) => {
        const id = await generateTxId(tx)
        await tx
          .update(devices)
          .set({ label: input.label, updatedAt: new Date() })
          .where(
            and(
              eq(devices.userId, ctx.session.user.id),
              eq(devices.deviceId, input.deviceId)
            )
          )
        return id
      })
      return { ok: true, txid }
    }),

  // EXP-622: mark one of the caller's OWN machines as their default — the
  // row every device picker prefills once several machines are candidates
  // (start coding, action runs, the automations "Runs on" binding). At most
  // one true row per user: setting one clears the rest in the same
  // transaction, so no client ever has to break a tie. A teammate's shared
  // server can never be the caller's default (the flag lives on the row and
  // belongs to its owner) — which is also why every client reads it only on
  // rows whose `user_id` is theirs.
  setDefault: authedProcedure
    .input(z.object({ deviceId: deviceIdInput, isDefault: z.boolean() }))
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({ id: devices.id })
        .from(devices)
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .limit(1)
      if (!row) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
      }
      const txid = await ctx.db.transaction(async (tx) => {
        const id = await generateTxId(tx)
        const now = new Date()
        if (input.isDefault) {
          // Already-false rows stay untouched — no needless Electric op
          // (the setShared idiom).
          await tx
            .update(devices)
            .set({ isDefault: false, updatedAt: now })
            .where(
              and(
                eq(devices.userId, ctx.session.user.id),
                ne(devices.id, row.id),
                eq(devices.isDefault, true)
              )
            )
        }
        await tx
          .update(devices)
          .set({ isDefault: input.isDefault, updatedAt: now })
          .where(eq(devices.id, row.id))
        return id
      })
      return { ok: true, txid }
    }),

  // Drops the registry row only — a still-running daemon re-registers on its
  // next heartbeat miss, and a live relay connection is untouched. Worktrees
  // + queued commands go with it (FK cascade).
  remove: authedProcedure
    .input(z.object({ deviceId: deviceIdInput }))
    .mutation(async ({ ctx, input }) => {
      const txid = await ctx.db.transaction(async (tx) => {
        const id = await generateTxId(tx)
        await tx
          .delete(devices)
          .where(
            and(
              eq(devices.userId, ctx.session.user.id),
              eq(devices.deviceId, input.deviceId)
            )
          )
        return id
      })
      return { ok: true, txid }
    }),
})
