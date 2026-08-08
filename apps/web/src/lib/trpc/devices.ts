// EXP-403 registered devices: desktops and headless `exponential` daemon
// servers register themselves per user, heartbeat `last_seen_at`, and the
// agents UI polls `list` (tRPC — deliberately NOT an Electric shape; per-user
// machine state, not team product data). `list` merges the registry with live
// steer-relay presence so a row carries both durable identity (label, kind,
// last seen) and the live advertisement (online, agents, caps).
// EXP-432 bends the per-user rule exactly once: a server device may be SHARED
// with one team (`shared_team_id`, owner-toggled via `setShared`), and a
// team-scoped `list({teamId})` additionally returns teammates' shared rows so
// members can remote-start on them.
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, ne, inArray } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { devices, teamMembers, users } from "@/db/schema"
import { assertTeamMember } from "@/lib/team-membership"
import { versionPayload } from "@/lib/client-version"
import { endForeignHostedSessions } from "@/lib/coding-session-kill"
import {
  getSteerRelayConfig,
  relayGetDevices,
  type SteerDevice,
  type SteerLaunchDefaults,
} from "@/lib/steer"

// Mirrors the relay's online-frame bounds (steer-relay protocol.ts): the
// relay is a dumb pipe and the same strings land here via `register`.
const agentsInput = z.array(z.string().min(1).max(32)).max(16)
const capsInput = z.array(z.string().min(1).max(32)).max(16)
const deviceIdInput = z.string().min(1).max(128)

export type DeviceListEntry = {
  deviceId: string
  deviceLabel: string
  kind: `desktop` | `server`
  platform: string | null
  // EXP-432: the team this device is shared with (null = private). Set on
  // own rows too so the UI can badge them "Shared".
  sharedTeamId: string | null
  // EXP-432: set ONLY on teammates' shared rows — the device owner, for
  // attribution in lists/pickers. Absent on the caller's own rows.
  owner?: { id: string; name: string }
  // Runnable agents (installed AND signed in since EXP-409).
  agents: string[]
  // EXP-409: agents installed but signed out on the machine — live relay
  // presence only (not persisted: an offline row reads offline regardless).
  unauthedAgents: string[]
  caps: string[]
  // EXP-437: the machine's per-agent launch defaults — live relay presence
  // only (never persisted: you can only START on an online device, so the
  // seed source is exactly as fresh as it needs to be). Absent = old
  // desktop build; clients seed static contract defaults.
  launchDefaults?: SteerLaunchDefaults
  online: boolean
  // ISO timestamp of the last register/heartbeat; null for a relay-only
  // device that predates the registry (old desktop build).
  lastSeenAt: string | null
  registered: boolean
  // The client's marketing version as of its last register; null for old
  // builds that don't send one.
  version: string | null
  // An Update click is pending (set by requestUpdate, consumed when the
  // device re-registers after acting on it).
  updateRequested: boolean
  // EXP-411: the pending request is parked behind live coding sessions on
  // the machine — it applies once they close, and the rows say "Update
  // queued" instead of spinning forever.
  updateBlocked: boolean
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
        version: z.string().min(1).max(32).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const now = new Date()
      await ctx.db
        .insert(devices)
        .values({
          userId: ctx.session.user.id,
          deviceId: input.deviceId,
          label: input.label,
          kind: input.kind,
          platform: input.platform ?? null,
          agents: input.agents ?? [],
          caps: input.caps ?? [],
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
      return { ok: true }
    }),

  // Liveness bump. `ok: false` means the row is gone (removed from the UI
  // while the daemon ran) — the caller should re-register.
  // `updateRequested` piggybacks the web "Update" button to the daemon: it
  // checks for a new release, updates + restarts when one exists, and its
  // next register consumes the flag either way. While sessions are live the
  // daemon defers the update and reports the count back here
  // (`activeSessions`, EXP-411) so `list` can say "queued" instead of
  // letting the spinner run forever; pre-EXP-411 daemons omit the field and
  // the stored count stays untouched.
  heartbeat: authedProcedure
    .input(
      z.object({
        deviceId: deviceIdInput,
        activeSessions: z.number().int().min(0).max(1000).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const now = new Date()
      const updated = await ctx.db
        .update(devices)
        .set({
          lastSeenAt: now,
          updatedAt: now,
          ...(input.activeSessions === undefined
            ? {}
            : { activeSessions: input.activeSessions }),
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
        })
      return {
        ok: updated.length > 0,
        updateRequested: Boolean(updated[0]?.updateRequestedAt),
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

  // Optional `teamId` (EXP-432): additionally return teammates' server
  // devices shared with that team. Old clients call with no input and get
  // exactly the pre-EXP-432 own-devices behavior.
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid().optional() }).optional())
    .query(async ({ ctx, input }): Promise<{
    devices: DeviceListEntry[]
    // Informational CLIENT_LATEST_VERSION_* values (null when unset) — the
    // UI hints "update available" when a row's version compares below.
    latestVersions: { desktop: string | null; cli: string | null }
  }> => {
    const teamId = input?.teamId
    if (teamId) await assertTeamMember(ctx.session.user.id, teamId)

    const rows = await ctx.db
      .select()
      .from(devices)
      .where(eq(devices.userId, ctx.session.user.id))
      .orderBy(desc(devices.lastSeenAt))

    // Teammates' shared server devices (EXP-432). The team_members join
    // drops ghost shares whose owner has since left the team — the share
    // column survives membership changes, but an ex-member's box must
    // neither list nor start.
    const sharedRows = teamId
      ? await ctx.db
          .select({
            device: devices,
            ownerName: users.name,
          })
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
              ne(devices.userId, ctx.session.user.id)
            )
          )
          .orderBy(desc(devices.lastSeenAt))
      : []

    // Live relay presence is best-effort: a down relay must not blank the
    // registry — everything just reads offline. Shared rows need their
    // OWNER's presence bucket; fetch once per distinct owner.
    const config = getSteerRelayConfig()
    let live: SteerDevice[] = []
    const liveByOwner = new Map<string, Map<string, SteerDevice>>()
    if (config) {
      try {
        live = (await relayGetDevices(config, ctx.session.user.id)).devices
      } catch {
        live = []
      }
      const ownerIds = [...new Set(sharedRows.map((r) => r.device.userId))]
      await Promise.all(
        ownerIds.map(async (ownerId) => {
          try {
            const { devices: ownerLive } = await relayGetDevices(
              config,
              ownerId
            )
            liveByOwner.set(
              ownerId,
              new Map(ownerLive.map((d) => [d.deviceId, d]))
            )
          } catch {
            // That owner's rows read offline.
          }
        })
      )
    }
    const liveById = new Map(live.map((d) => [d.deviceId, d]))

    // Relay-connected rows are demonstrably alive RIGHT NOW — advance their
    // last_seen_at so a later disconnect shows "last seen" near the actual
    // disconnect, not the process start. (The desktop registers once per
    // control-channel start and never heartbeats; this write-on-observe
    // keeps its timestamp honest while anyone is watching.)
    const observedOnline = rows
      .filter((row) => liveById.has(row.deviceId))
      .map((row) => row.id)
    if (observedOnline.length > 0) {
      const now = new Date()
      await ctx.db
        .update(devices)
        .set({ lastSeenAt: now, updatedAt: now })
        .where(inArray(devices.id, observedOnline))
      for (const row of rows) {
        if (liveById.has(row.deviceId)) row.lastSeenAt = now
      }
    }

    const entries: DeviceListEntry[] = rows.map((row) => {
      const online = liveById.get(row.deviceId)
      liveById.delete(row.deviceId)
      return {
        deviceId: row.deviceId,
        // The REGISTRY label is authoritative for registered rows — a
        // rename must be visible immediately, online or not (the relay
        // still holds the label the device advertised at connect time).
        deviceLabel: row.label,
        kind: row.kind === `server` ? `server` : `desktop`,
        platform: row.platform,
        // The live advertisement is fresher than the registered snapshot —
        // startSession gates on exactly what the relay holds.
        agents: online?.agents ?? row.agents,
        unauthedAgents: online?.unauthedAgents ?? [],
        caps: online?.caps ?? row.caps,
        launchDefaults: online?.launchDefaults,
        online: Boolean(online),
        lastSeenAt: row.lastSeenAt.toISOString(),
        registered: true,
        version: row.version,
        updateRequested: Boolean(row.updateRequestedAt),
        updateBlocked: Boolean(row.updateRequestedAt) && row.activeSessions > 0,
        sharedTeamId: row.sharedTeamId,
      }
    })

    // Teammates' shared rows, appended AFTER own rows (stable UI grouping).
    // Same live-over-snapshot merge as own rows; no write-on-observe — a
    // server device heartbeats its own last_seen_at every 60s, and list()
    // must not write other users' rows.
    for (const { device: row, ownerName } of sharedRows) {
      const online = liveByOwner.get(row.userId)?.get(row.deviceId)
      entries.push({
        deviceId: row.deviceId,
        deviceLabel: row.label,
        kind: `server`,
        platform: row.platform,
        agents: online?.agents ?? row.agents,
        unauthedAgents: online?.unauthedAgents ?? [],
        caps: online?.caps ?? row.caps,
        launchDefaults: online?.launchDefaults,
        online: Boolean(online),
        lastSeenAt: row.lastSeenAt.toISOString(),
        registered: true,
        version: row.version,
        updateRequested: Boolean(row.updateRequestedAt),
        updateBlocked: Boolean(row.updateRequestedAt) && row.activeSessions > 0,
        sharedTeamId: row.sharedTeamId,
        owner: { id: row.userId, name: ownerName },
      })
    }

    // A connected device that never registered (desktop build predating the
    // registry): still show it, or remote start would regress on update lag.
    for (const d of liveById.values()) {
      entries.unshift({
        deviceId: d.deviceId,
        deviceLabel: d.deviceLabel,
        kind: `desktop`,
        platform: null,
        agents: d.agents ?? [`claude`],
        unauthedAgents: d.unauthedAgents ?? [],
        caps: d.caps ?? [],
        launchDefaults: d.launchDefaults,
        online: true,
        lastSeenAt: null,
        registered: false,
        version: null,
        updateRequested: false,
        updateBlocked: false,
        sharedTeamId: null,
      })
    }

    const payload = versionPayload() as Record<
      string,
      { latest: string | null }
    >
    return {
      devices: entries,
      latestVersions: {
        desktop: payload.desktop?.latest ?? null,
        cli: payload.cli?.latest ?? null,
      },
    }
  }),

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
      await ctx.db
        .update(devices)
        .set({ sharedTeamId: input.teamId, updatedAt: new Date() })
        .where(eq(devices.id, row.id))
      // EXP-445: withdrawing the share must end the runs it was the consent
      // for — otherwise a teammate's agent keeps working on this machine with
      // no client able to reach it. Ordered AFTER the column write on purpose:
      // once shared_team_id has moved, resolveStartAttribution refuses new
      // foreign attributions, so nothing can slip in behind the fan-out.
      // First share (null → team) and a same-team re-share end nothing.
      if (row.sharedTeamId !== null && row.sharedTeamId !== input.teamId) {
        await endForeignHostedSessions(ctx.session.user.id, row.sharedTeamId)
      }
      return { ok: true }
    }),

  rename: authedProcedure
    .input(
      z.object({ deviceId: deviceIdInput, label: z.string().min(1).max(255) })
    )
    .mutation(async ({ ctx, input }) => {
      await ctx.db
        .update(devices)
        .set({ label: input.label, updatedAt: new Date() })
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
      return { ok: true }
    }),

  // Drops the registry row only — a still-running daemon re-registers on its
  // next heartbeat miss, and a live relay connection is untouched.
  remove: authedProcedure
    .input(z.object({ deviceId: deviceIdInput }))
    .mutation(async ({ ctx, input }) => {
      await ctx.db
        .delete(devices)
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
      return { ok: true }
    }),
})
