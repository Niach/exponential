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
// EXP-485 `register` is the SOLE agents/caps/unauthedAgents writer (the relay
// online frame no longer advertises them).
// EXP-432 bends the per-user rule exactly once: a server device may be SHARED
// with one team (`shared_team_id`, owner-toggled via `setShared`) so members
// can remote-start on it.
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, ne, sql } from "drizzle-orm"
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
  deviceAgentUsageSchema,
  deviceCommands,
  deviceLaunchDefaultsSchema,
  devices,
  deviceWorktrees,
  teamMembers,
  users,
  type DeviceAgentAccount,
  type DeviceAgentAccounts,
  type DeviceAgentLaunchDefaults,
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
  agentSupportsSkipPermissions,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

// Mirrors the relay's online-frame bounds (steer-relay protocol.ts): the
// relay is a dumb pipe and the same strings land here via `register`.
const agentsInput = z.array(z.string().min(1).max(32)).max(16)
const capsInput = z.array(z.string().min(1).max(32)).max(16)
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
      if (
        typeof d.skipPermissions === `boolean` &&
        agentSupportsSkipPermissions(agent)
      ) {
        entry.skipPermissions = d.skipPermissions
      }
      if (Object.keys(entry).length > 0) agents[agent] = entry
    }
    if (Object.keys(agents).length > 0) out.agents = agents
  }
  return out
}

// EXP-484 bounds: the device reports at most one entry per contract agent,
// and a window list a bar can actually render.
const MAX_STATUS_AGENTS = 3
const MAX_USAGE_WINDOWS = 10
const MAX_USAGE_KEY = 64
const MAX_USAGE_LABEL = 32
const MAX_ACCOUNT_EMAIL = 320
const MAX_ACCOUNT_PLAN = 64

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
    out[agent] = entry
  }
  return out
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
    const entry: DeviceAgentUsage = {
      fetchedAt: isoStampOrNull(usage.fetchedAt) ?? now.toISOString(),
      stale: usage.stale === true,
      windows,
    }
    out[agent] = entry
  }
  return out
}

// Best-effort, fire-and-forget: persisted state is the durable path, the
// nudge only kills heartbeat-pickup latency for online devices.
function nudgeDevice(ownerId: string, deviceId: string): void {
  const config = getSteerRelayConfig()
  if (!config) return
  void relayPostNudge(config, ownerId, deviceId).catch(() => {})
}

// ISO-or-null of a nullable timestamp — the CAS stamp wire form (equality
// compare on the echoed string; never `>`, no clock-skew semantics).
function stampOf(value: Date | null): string | null {
  return value ? value.toISOString() : null
}

// EXP-639: the ONE "which device rows may this user see" read — their own
// registrations (any kind), plus the SERVER devices teammates shared with
// `teamId` (EXP-432). The team_members join drops ghost shares whose owner has
// since left the team: `shared_team_id` survives membership changes, but an
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
        eq(devices.sharedTeamId, teamId),
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
        // EXP-481: the device's local defaults, applied ONLY as a first-ever
        // seed (row column NULL) — after that the server copy is
        // authoritative and the setLaunchDefaults CAS decides.
        launchDefaults: deviceLaunchDefaultsSchema.optional(),
        // EXP-484: the machine's per-agent sign-in status from its doctor
        // probe. ABSENT (an older build) leaves the column untouched on
        // conflict — never zeroed to "signed out".
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
          agents: input.agents ?? [],
          caps: input.caps ?? [],
          unauthedAgents: input.unauthedAgents ?? [],
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
            agents: input.agents ?? [],
            caps: input.caps ?? [],
            unauthedAgents: input.unauthedAgents ?? [],
            // Seed-only-when-NULL: a re-register must never stomp
            // server-side edits (the device converges via heartbeat instead).
            ...(input.launchDefaults
              ? {
                  launchDefaults: sql`COALESCE(${devices.launchDefaults}, ${JSON.stringify(clampLaunchDefaults(input.launchDefaults))}::jsonb)`,
                  launchDefaultsUpdatedAt: sql`COALESCE(${devices.launchDefaultsUpdatedAt}, ${now.toISOString()}::timestamptz)`,
                }
              : {}),
            // Absent = an older build with no collector: keep whatever the
            // last reporting register wrote.
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
        // (the device compares against what it last shipped) — absent leaves
        // the columns alone. `agent_usage_at` moves with `agent_usage` only,
        // and is deliberately not a convergence trigger for anything.
        agentAccounts: deviceAgentAccountsSchema.optional(),
        agentUsage: deviceAgentUsageSchema.optional(),
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
  // picks the request up. Own-user only via the where clause.
  requestUpdate: authedProcedure
    .input(z.object({ deviceId: deviceIdInput }))
    .mutation(async ({ ctx, input }) => {
      const now = new Date()
      const updated = await ctx.db
        .update(devices)
        .set({ updateRequestedAt: now, updatedAt: now })
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .returning({ id: devices.id })
      return { ok: updated.length > 0 }
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
      const byKey = new Map<string, (typeof input.worktrees)[number]>()
      for (const wt of input.worktrees) {
        byKey.set(`${wt.repoFullName} ${wt.branch}`, wt)
      }
      const reported = [...byKey.entries()]
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([, wt]) => wt)

      const now = new Date()
      await ctx.db.transaction(async (tx) => {
        for (const wt of reported) {
          const agents = wt.agents ?? null
          await tx
            .insert(deviceWorktrees)
            .values({
              deviceRowId: row.id,
              // user_id/shared_team_id are trigger-populated; the schema
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
  // the sign-in URL, which the requester polls for via `getCommand`.
  createCommand: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        kind: z.enum([`worktree_remove`, `worktree_prune`, `agent_login`]),
        repoFullName: z.string().min(1).max(255).optional(),
        branch: z.string().min(1).max(255).optional(),
        // EXP-484 `agent_login` inputs (ignored by the other kinds): which
        // agent CLI to sign in, and whether to sign the current account out
        // first (Switch account).
        agent: z.enum(codingAgentValues).optional(),
        switch: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({ id: devices.id, caps: devices.caps })
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
        // pi signs in through its own interactive prompt with no device-code
        // flow to hand back — local only, by design.
        if (input.agent === `pi`) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `pi has no remote sign-in`,
          })
        }
        // The executor lives in the desktop app and the daemon; an older
        // build would leave the row pending forever.
        if (!(row.caps ?? []).includes(`agent-login`)) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `That machine's app is too old to sign an agent in remotely`,
          })
        }
        payload = {
          agent: input.agent,
          switch: input.switch === true ? `true` : `false`,
        }
      }

      // One pending command per (device, kind, payload) — a double-click must
      // not queue the same prune twice.
      const [dup] = await ctx.db
        .select({ id: deviceCommands.id })
        .from(deviceCommands)
        .where(
          and(
            eq(deviceCommands.deviceRowId, row.id),
            eq(deviceCommands.kind, input.kind),
            eq(deviceCommands.status, `pending`),
            sql`${deviceCommands.payload} = ${JSON.stringify(payload)}::jsonb`
          )
        )
        .limit(1)
      if (dup) {
        throw new TRPCError({
          code: `CONFLICT`,
          message: `That command is already queued`,
        })
      }

      const [command] = await ctx.db
        .insert(deviceCommands)
        .values({
          deviceRowId: row.id,
          userId: ctx.session.user.id,
          kind: input.kind,
          payload,
        })
        .returning({ id: deviceCommands.id })
      nudgeDevice(ctx.session.user.id, input.deviceId)
      return { id: command.id }
    }),

  // EXP-481: the device reports a command's outcome. Only pending rows
  // transition; a duplicate complete (heartbeat redelivery races the first
  // completion) is tolerated with ok:false rather than an error.
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
        .returning({ id: deviceCommands.id })
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

  // EXP-432: share/unshare one of the caller's SERVER devices with a team
  // they belong to (teamId: null clears the share). Sharing is the consent
  // that lets teammates remote-start on the box — the resulting sessions run
  // under the owner's daemon but belong to the requesting teammate
  // (coding-sessions `resolveStartAttribution`).
  setShared: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        teamId: z.string().uuid().nullable(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({
          id: devices.id,
          kind: devices.kind,
          sharedTeamId: devices.sharedTeamId,
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
      if (input.teamId) {
        await assertTeamMember(ctx.session.user.id, input.teamId)
      }
      // EXP-530 follow-up: an automation bound to this device by one of the
      // OLD team's owners keeps firing on the device owner's credentials once
      // the share is withdrawn (there is no server scheduler — the device
      // self-selects automations off Electric), and the toggle is owner-only,
      // so the machine's owner cannot stop it. Withdrawing the share disables
      // those automations in the same transaction as the column write. Skipped
      // when the device owner is an OWNER of that team: then the bindings are
      // plausibly their own, and they keep the toggle to undo them.
      const revoked = row.sharedTeamId
      const disableAutomations =
        revoked !== null &&
        revoked !== input.teamId &&
        (await getTeamMember(ctx.session.user.id, revoked))?.role !== `owner`
      const txid = await ctx.db.transaction(async (tx) => {
        const id = await generateTxId(tx)
        await tx
          .update(devices)
          .set({ sharedTeamId: input.teamId, updatedAt: new Date() })
          .where(eq(devices.id, row.id))
        if (disableAutomations && revoked) {
          await tx
            .update(automations)
            .set({ enabled: false, updatedAt: new Date() })
            .where(
              and(
                eq(automations.teamId, revoked),
                eq(automations.deviceId, input.deviceId),
                // Already-off rows stay untouched (no needless Electric op).
                eq(automations.enabled, true)
              )
            )
        }
        return id
      })
      // EXP-445: withdrawing the share must end the runs it was the consent
      // for — otherwise a teammate's agent keeps working on this machine with
      // no client able to reach it. Ordered AFTER the column write on purpose:
      // once shared_team_id has moved, resolveStartAttribution refuses new
      // foreign attributions, so nothing can slip in behind the fan-out.
      // First share (null → team) and a same-team re-share end nothing.
      // Device-scoped (EXP-560): only THIS machine's foreign runs die — the
      // owner's other same-team shares keep theirs.
      if (row.sharedTeamId !== null && row.sharedTeamId !== input.teamId) {
        await endForeignHostedSessions(
          ctx.session.user.id,
          row.sharedTeamId,
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
