/**
 * A stand-in desktop for the store-screenshot capture runs (EXP-393).
 *
 * Three of the eight store shots — the Start-coding dialog, the live steering
 * feed, and the "Coding now" row on the issue detail — only exist when the
 * instance HAS a steer relay and a desktop is online on it. Without one,
 * `steer.config` reports disabled and the issue detail renders "Live steering
 * is unavailable on this instance." right in the middle of a screenshot we are
 * shipping to the App Store.
 *
 * So instead of faking anything in the apps, this speaks the real relay wire
 * protocol (apps/steer-relay/src/protocol.ts) the way the real desktop does
 * (apps/desktop/crates/steer/control_channel.rs):
 *
 *   control socket    → one `online` frame, so the relay holds the device
 *                       present for remote starts (the picker itself reads
 *                       the synced `devices` row heartbeated below).
 *                       Inbound `start_session` is logged and ignored:
 *                       the screenshot opens the dialog and cancels.
 *
 * VAPP-43: the relay carries no session content any more (viewers read a
 * session over eNet from the device that runs it), so this stub no longer
 * publishes a scripted activity feed; the seeded session stays warm only.
 *
 * Usage (from apps/web, after `bun run seed:screenshots`, with the relay up via
 * `docker compose --profile steer up -d` and the same STEER_RELAY_* env the web
 * server runs with):
 *   STEER_RELAY_URL=ws://localhost:4002 STEER_RELAY_SECRET=dev-steer-secret \
 *     bun run screenshots:desktop
 *
 * Runs until Ctrl-C. Keep it running for the whole fastlane capture.
 */
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, sessions, users } from "@/db/schema"
import {
  getSteerRelayConfig,
  mintSteerTicket,
  type SteerRelayConfig,
  type SteerTicketSeed,
} from "@/lib/steer"
import { assertDemoLiveSessions } from "./lib/demo-live-sessions"
import { reclockDemoRows } from "./lib/demo-reclock"
import { assertLocalDatabase } from "./lib/local-db-guard"
import {
  DEMO_DEVICE_ID,
  DEMO_DEVICE_LABEL,
  DEMO_DEVICE_VERSION,
  DEMO_EMAIL,
  DEMO_STEERED_SESSION_ID,
  demoAgentReport,
} from "./screenshot-demo"

// >1 agent so the sheet renders its agent pill strip; the caps are the ones a
// real desktop advertises (see crates/ui/src/steer_wiring.rs). Since EXP-485
// the agents advertisement rides the registered devices ROW, not the relay
// online frame.
const AGENTS = [`claude`, `codex`]
// `resume-run` (EXP-637) is advertised by every real desktop and is what makes
// the seed's ended runs resumable — without it "Recent automated runs"
// renders, but the Resume button never does (EXP-663). `agent-login`
// (EXP-484) is what puts the Login / Switch account button on each agent's
// row in machine settings — a real desktop declares it unconditionally.
const CAPS = [
  `actions`,
  `action-inputs`,
  `fix-conflicts`,
  `chat`,
  `triggers`,
  `automations`,
  `resume-run`,
  `agent-login`,
]

const RECONNECT_MS = 2_000
// The clients hide sessions whose updated_at is older than the contract's
// 2h staleHours window. A capture run — never mind a re-run the next morning
// against the same seed — outlives that, and a stale row silently takes the
// "Coding now" line and the session link off the issue detail. A real desktop
// keeps its row warm while it runs; so does this one. 30s like the real
// desktop: the same tick refreshes the `devices` row, and that one has to
// stay inside the 90s online window with room for jitter (EXP-481).
// EXP-913: "warm" means re-ASSERTED, not just touched — the server's staleness
// sweep may already have deleted a row (Mira's was never heartbeated, and a
// rerun boots long after the last stub died), so every tick upserts the seeded
// live rows back (lib/demo-live-sessions.ts).
const HEARTBEAT_MS = 30_000

function required(config: SteerRelayConfig | null): SteerRelayConfig {
  if (config) return config
  console.error(
    `STEER_RELAY_URL + STEER_RELAY_SECRET must both be set (and must match the relay and the web server). See docker-compose.yaml --profile steer.`
  )
  process.exit(1)
}

async function resolveTarget() {
  const [demo] = await db
    .select({ id: users.id })
    .from(users)
    .where(eq(users.email, DEMO_EMAIL))
    .limit(1)
  if (!demo) {
    throw new Error(`demo user ${DEMO_EMAIL} not found — run seed:screenshots first`)
  }
  // The seeded steered session (EXP-1293: the cold-start thread's child): the
  // demo user's own RUNNING one, which is the only session the apps let them
  // watch (EXP-312, owner-only). Re-asserted first (EXP-913): a capture may
  // have stopped it, and the legacy ticket row's sweep deletes it once no stub
  // has heartbeated it for 2h, which is exactly the gap before a rerun.
  await assertDemoLiveSessions()
  const [session] = await db
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        eq(sessions.id, DEMO_STEERED_SESSION_ID),
        eq(sessions.userId, demo.id),
        eq(sessions.status, `running`)
      )
    )
    .limit(1)
  if (!session) {
    throw new Error(
      `no running session ${DEMO_STEERED_SESSION_ID} for ${DEMO_EMAIL} — run seed:screenshots first`
    )
  }
  return { userId: demo.id, sessionId: session.id }
}

/**
 * Hold one relay socket open, re-dialling with a FRESH ticket after every drop
 * (tickets carry a 60s connect window, so a stored one cannot be reused).
 */
function hold(opts: {
  name: string
  config: SteerRelayConfig
  seed: SteerTicketSeed
  onOpen: (send: (frame: Record<string, unknown>) => void) => void
  onFrame?: (frame: Record<string, unknown>) => void
}) {
  let closed = false
  const dial = () => {
    if (closed) return
    const minted = mintSteerTicket(opts.config, opts.seed)
    if (`disabled` in minted) return
    // The minted `url` is used as-is — never reconstruct it (crates/api/src/steer.rs).
    const ws = new WebSocket(minted.url)
    const send = (frame: Record<string, unknown>) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(frame))
    }

    ws.onopen = () => {
      console.log(`[${opts.name}] connected`)
      opts.onOpen(send)
    }
    ws.onmessage = (event) => {
      if (typeof event.data !== `string`) return
      try {
        opts.onFrame?.(JSON.parse(event.data) as Record<string, unknown>)
      } catch {
        // The relay only speaks JSON; anything else is not ours to handle.
      }
    }
    ws.onerror = () => {}
    ws.onclose = (event) => {
      if (closed) return
      console.log(`[${opts.name}] closed (${event.code}) — reconnecting`)
      setTimeout(dial, RECONNECT_MS)
    }
  }
  dial()
  return () => {
    closed = true
  }
}

async function main() {
  // EXP-913: this stub WRITES — it upserts the demo device row on every beat
  // and re-clocks the seeded rows — so it refuses a database that is not
  // obviously the local dev one, exactly like `seed:screenshots` does.
  assertLocalDatabase(
    `screenshots:desktop upserts the demo device + MCP readiness rows and SHIFTS the seeded timestamps.`
  )
  const config = required(getSteerRelayConfig())
  const { userId, sessionId } = await resolveTarget()

  const stopControl = hold({
    name: `control`,
    config,
    seed: { kind: `control`, userId },
    onOpen: (send) =>
      send({
        t: `online`,
        deviceId: DEMO_DEVICE_ID,
        deviceLabel: DEMO_DEVICE_LABEL,
        caps: CAPS,
      }),
    onFrame: (frame) => {
      if (frame.t === `start_session`) {
        // A real desktop would launch an agent and claim the `sessions`
        // row. The capture only opens the dialog and cancels, so acknowledging
        // it in the log is enough — and a stray tap can't corrupt the seed.
        console.log(`[control] ignoring start_session:`, JSON.stringify(frame))
      }
    },
  })

  console.log(`
Screenshot desktop online:
  device      ${DEMO_DEVICE_LABEL} (${AGENTS.join(`, `)})
  session     ${sessionId}
  relay       ${config.url}

Leave this running for the whole fastlane capture. Ctrl-C to stop.
`)

  // EXP-481 moved "is a desktop online?" off relay presence and onto the
  // synced `devices` rows (`last_seen_at` within contract
  // `device.onlineWindowSeconds`, 90s). Without a registry row the mobile
  // Start-coding circle renders its no-device state and the dialog never
  // opens (EXP-580) — so register like a real desktop's heartbeat would and
  // keep the row fresh alongside the session heartbeat.
  //
  // EXP-733: the same tick re-stamps the per-agent account + usage report
  // (`demoAgentReport`) the way a real heartbeat carries it. Every client
  // gates its Agents section, the session usage strip and the mobile Usage
  // sheet on these two columns — and the usage cards only render while
  // `fetchedAt` is under 15 minutes old, so a one-time write at boot would
  // photograph fresh cards on the first lane and an "as of 20 minutes ago"
  // line on the last.
  const touchDevice = () => {
    const now = new Date()
    const report = demoAgentReport(now)
    return db
      .insert(devices)
      .values({
        userId,
        deviceId: DEMO_DEVICE_ID,
        label: DEMO_DEVICE_LABEL,
        kind: `desktop`,
        platform: `macos`,
        // A real desktop reports its marketing version on every register, and
        // marks itself the owner's default machine once they pick it — without
        // both, the machine row renders bare (no version pill, no default star).
        version: DEMO_DEVICE_VERSION,
        isDefault: true,
        agents: AGENTS,
        // The natives' machine picker reads the ACP-runnable list; without it
        // every composer shot says "Not ready on Alex's MacBook Pro".
        acpAgents: AGENTS,
        caps: CAPS,
        agentAccounts: report.agentAccounts,
        agentUsage: report.agentUsage,
        agentUsageAt: now,
        lastSeenAt: now,
        activeSessions: 1,
      })
      .onConflictDoUpdate({
        target: [devices.userId, devices.deviceId],
        set: {
          lastSeenAt: now,
          label: DEMO_DEVICE_LABEL,
          version: DEMO_DEVICE_VERSION,
          isDefault: true,
          agents: AGENTS,
          acpAgents: AGENTS,
          caps: CAPS,
          agentAccounts: report.agentAccounts,
          agentUsage: report.agentUsage,
          agentUsageAt: now,
        },
      })
      .catch((err) => console.error(`[device heartbeat]`, err))
  }
  await touchDevice()
  await reclockDemoRows().catch((err) => console.error(`[reclock]`, err))

  const heartbeat = setInterval(() => {
    void touchDevice()
    void assertDemoLiveSessions().catch((err) => console.error(`[heartbeat]`, err))
    // EXP-913: keep the seeded "X ago" labels at their seeded age while the
    // lanes run (lib/demo-reclock.ts).
    void reclockDemoRows().catch((err) => console.error(`[reclock]`, err))
  }, HEARTBEAT_MS)

  const shutdown = () => {
    clearInterval(heartbeat)
    stopControl()
    process.exit(0)
  }
  process.on(`SIGINT`, shutdown)
  process.on(`SIGTERM`, shutdown)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
