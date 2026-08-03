// EXP-403 registered devices: desktops and headless `exponential` daemon
// servers register themselves per user, heartbeat `last_seen_at`, and the
// agents UI polls `list` (tRPC — deliberately NOT an Electric shape; per-user
// machine state, not team product data). `list` merges the registry with live
// steer-relay presence so a row carries both durable identity (label, kind,
// last seen) and the live advertisement (online, agents, caps).
import { z } from "zod"
import { and, desc, eq, inArray } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { devices } from "@/db/schema"
import {
  getSteerRelayConfig,
  relayGetDevices,
  type SteerDevice,
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
  agents: string[]
  caps: string[]
  online: boolean
  // ISO timestamp of the last register/heartbeat; null for a relay-only
  // device that predates the registry (old desktop build).
  lastSeenAt: string | null
  registered: boolean
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
          lastSeenAt: now,
        })
        .onConflictDoUpdate({
          target: [devices.userId, devices.deviceId],
          set: {
            kind: input.kind,
            platform: input.platform ?? null,
            agents: input.agents ?? [],
            caps: input.caps ?? [],
            lastSeenAt: now,
            updatedAt: now,
          },
        })
      return { ok: true }
    }),

  // Liveness bump. `ok: false` means the row is gone (removed from the UI
  // while the daemon ran) — the caller should re-register.
  heartbeat: authedProcedure
    .input(z.object({ deviceId: deviceIdInput }))
    .mutation(async ({ ctx, input }) => {
      const now = new Date()
      const updated = await ctx.db
        .update(devices)
        .set({ lastSeenAt: now, updatedAt: now })
        .where(
          and(
            eq(devices.userId, ctx.session.user.id),
            eq(devices.deviceId, input.deviceId)
          )
        )
        .returning({ id: devices.id })
      return { ok: updated.length > 0 }
    }),

  list: authedProcedure.query(async ({ ctx }): Promise<{
    devices: DeviceListEntry[]
  }> => {
    const rows = await ctx.db
      .select()
      .from(devices)
      .where(eq(devices.userId, ctx.session.user.id))
      .orderBy(desc(devices.lastSeenAt))

    // Live relay presence is best-effort: a down relay must not blank the
    // registry — everything just reads offline.
    const config = getSteerRelayConfig()
    let live: SteerDevice[] = []
    if (config) {
      try {
        live = (await relayGetDevices(config, ctx.session.user.id)).devices
      } catch {
        live = []
      }
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
        caps: online?.caps ?? row.caps,
        online: Boolean(online),
        lastSeenAt: row.lastSeenAt.toISOString(),
        registered: true,
      }
    })

    // A connected device that never registered (desktop build predating the
    // registry): still show it, or remote start would regress on update lag.
    for (const d of liveById.values()) {
      entries.unshift({
        deviceId: d.deviceId,
        deviceLabel: d.deviceLabel,
        kind: `desktop`,
        platform: null,
        agents: d.agents ?? [`claude`],
        caps: d.caps ?? [],
        online: true,
        lastSeenAt: null,
        registered: false,
      })
    }

    return { devices: entries }
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
