/**
 * VAPP-168: the scripted steering feed over eNet. Since VAPP-43 the steer
 * relay refuses content frames, and every viewer (web, desktop, iOS,
 * Android) reads a session from the DEVICE running it, over eNet's chat
 * channel (packages/domain-contract/fixtures/steer-enet.json).
 *
 * So the store-shot stand-in desktop runs a real eNet node,
 * `apps/desktop/crates/steer/examples/enet_feed_stub.rs`, serving the scripted
 * feed, and registers that node's endpoint as an `installs` row of the demo
 * machine: `installs.forSession` names it to every viewer, and every
 * teammate's allowlist (`installs.list`) holds it. The row is written straight
 * into the local database (the stub has no signed-in identity to call
 * `installs.register` with); the caller already refused a non-local one.
 */
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { and, eq, isNull, ne } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, installs } from "@/db/schema"

/** The node's identity survives restarts, so one `installs` row is reused. */
const IDENTITY_FILE = join(tmpdir(), `exp-screenshot-enet-identity`)
const FEED_FILE = join(tmpdir(), `exp-screenshot-enet-feed.json`)
// A first run compiles the example (the steer crate's tree); later runs are
// incremental.
const READY_TIMEOUT_MS = 15 * 60_000
const DESKTOP_DIR = resolve(import.meta.dir, `../../../desktop`)

export interface EnetFeed {
  sessionId: string
  /** `activity {event, seq}` frames recorded before any viewer joins. */
  journal: Record<string, unknown>[]
  /** Appended one per `liveEveryMs` after start, pushed to joined viewers. */
  live: Record<string, unknown>[]
  liveEveryMs: number
}

export interface EnetFeedStub {
  endpointId: string
  /** Re-stamps the install's `last_seen_at` (the heartbeat). */
  touch: () => Promise<void>
  stop: () => void
}

export async function startEnetFeedStub(opts: {
  userId: string
  deviceId: string
  label: string
  feed: EnetFeed
}): Promise<EnetFeedStub> {
  await Bun.write(FEED_FILE, JSON.stringify(opts.feed))
  const child = Bun.spawn({
    cmd: [
      `cargo`,
      `run`,
      `--quiet`,
      `-p`,
      `steer`,
      `--example`,
      `enet_feed_stub`,
      `--`,
      IDENTITY_FILE,
      FEED_FILE,
    ],
    cwd: DESKTOP_DIR,
    stdout: `pipe`,
    stderr: `inherit`,
    stdin: `ignore`,
  })

  const ready = await new Promise<{ endpointId: string; x25519Public: string }>(
    (resolveReady, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`the eNet feed stub was not online within ${READY_TIMEOUT_MS / 60_000} min`)),
        READY_TIMEOUT_MS
      )
      void child.exited.then((code) => {
        clearTimeout(timer)
        reject(new Error(`the eNet feed stub exited (${code}) before it came online`))
      })
      void (async () => {
        const reader = child.stdout.getReader()
        const decoder = new TextDecoder()
        let buffered = ``
        for (;;) {
          const { done, value } = await reader.read()
          if (done) return
          buffered += decoder.decode(value, { stream: true })
          const lines = buffered.split(`\n`)
          buffered = lines.pop() ?? ``
          for (const line of lines) {
            const match = line.match(/^READY ([0-9a-f]{64}) (\S+)$/)
            if (match) {
              clearTimeout(timer)
              resolveReady({ endpointId: match[1]!, x25519Public: match[2]! })
            } else if (line) {
              console.log(`[enet] ${line}`)
            }
          }
        }
      })()
    }
  )

  const [machine] = await db
    .select({ id: devices.id })
    .from(devices)
    .where(and(eq(devices.userId, opts.userId), eq(devices.deviceId, opts.deviceId)))
    .limit(1)
  if (!machine) {
    child.kill()
    throw new Error(`no devices row ${opts.deviceId} for the demo user: register it before the stub`)
  }
  const now = new Date()
  await db
    .insert(installs)
    .values({
      userId: opts.userId,
      kind: `desktop`,
      deviceId: machine.id,
      label: opts.label,
      platform: `macos`,
      endpointId: ready.endpointId,
      x25519Public: ready.x25519Public,
      lastSeenAt: now,
    })
    .onConflictDoUpdate({
      target: installs.endpointId,
      set: {
        userId: opts.userId,
        deviceId: machine.id,
        label: opts.label,
        x25519Public: ready.x25519Public,
        lastSeenAt: now,
        revokedAt: null,
      },
    })
  // An earlier run's node (a wiped tmpdir) would be dialled first when newer.
  await db
    .update(installs)
    .set({ revokedAt: now })
    .where(
      and(
        eq(installs.deviceId, machine.id),
        ne(installs.endpointId, ready.endpointId),
        isNull(installs.revokedAt)
      )
    )

  return {
    endpointId: ready.endpointId,
    touch: async () => {
      await db
        .update(installs)
        .set({ lastSeenAt: new Date() })
        .where(eq(installs.endpointId, ready.endpointId))
    },
    stop: () => child.kill(),
  }
}
