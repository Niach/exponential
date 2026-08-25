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
 * (apps/desktop/crates/steer/{control_channel,publisher}.rs):
 *
 *   control socket    → one `online` frame, so `steer.myDevices` returns a
 *                       device and the Start-coding dialog opens with a real
 *                       picker. Inbound `start_session` is logged and ignored:
 *                       the screenshot opens the dialog and cancels.
 *   publisher socket  → `hello` + `activity_reset` + a scripted activity feed
 *                       for the seeded running session, ending on an UNANSWERED
 *                       question so the steering shot shows "Needs your input"
 *                       with tappable options.
 *
 * The relay replays its activity log to every viewer that joins later, so the
 * phone gets the whole feed on connect no matter when the capture reaches it.
 *
 * Usage (from apps/web, after `bun run seed:screenshots`, with the relay up via
 * `docker compose --profile steer up -d` and the same STEER_RELAY_* env the web
 * server runs with):
 *   STEER_RELAY_URL=ws://localhost:4002 STEER_RELAY_SECRET=dev-steer-secret \
 *     bun run screenshots:desktop
 *
 * Runs until Ctrl-C. Keep it running for the whole fastlane capture.
 */
import { and, eq, ne } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, devices, users } from "@/db/schema"
import {
  getSteerRelayConfig,
  mintSteerTicket,
  type SteerRelayConfig,
  type SteerTicketSeed,
} from "@/lib/steer"
import {
  DEMO_DEVICE_ID,
  DEMO_DEVICE_LABEL,
  DEMO_DEVICE_VERSION,
  DEMO_EMAIL,
  DEMO_FEED_QUESTION,
} from "./screenshot-demo"

// >1 agent so the sheet renders its agent pill strip; the caps are the ones a
// real desktop advertises (see crates/ui/src/steer_wiring.rs).
const AGENTS = [`claude`, `codex`, `pi`]
const CAPS = [`actions`, `action-inputs`, `fix-conflicts`, `chat`, `automations`]

// Under the relay's 90s publisher-idle timeout (hub.ts PUBLISHER_IDLE_TIMEOUT_MS).
const KEEPALIVE_MS = 30_000
const RECONNECT_MS = 2_000
// The clients hide sessions whose updated_at is older than the contract's
// 2h staleHours window. A capture run — never mind a re-run the next morning
// against the same seed — outlives that, and a stale row silently takes the
// "Coding now" line and the session link off the issue detail. A real desktop
// keeps its row warm while it runs; so does this one. 30s like the real
// desktop: the same tick refreshes the `devices` row, and that one has to
// stay inside the 90s online window with room for jitter (EXP-481).
const HEARTBEAT_MS = 30_000

// The scripted session: a plausible mid-run agent transcript for the showcase
// issue ("Reduce cold start below 800 ms"). Diff events REPLACE rather than
// append in the relay's replay log, so there is exactly one.
//
// Long on purpose. The feed is bottom-anchored, so the phone shows the tail
// while the 13" iPad renders roughly three times as much — a transcript sized
// for the phone leaves two thirds of the iPad shot empty.
const FEED: Array<Record<string, unknown>> = [
  {
    kind: `user_message`,
    text: `Cold start is at 1.4s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of the critical path.`,
  },
  {
    kind: `narration`,
    text: `On it. I'll measure before I change anything: a cold-start regression is usually one await on the wrong side of the first frame.`,
  },
  {
    kind: `narration`,
    text: `Reading the two entry points that run before the first frame, then I'll trace what each of them waits on.`,
  },
  { kind: `tool`, name: `Read`, detail: `app/src/main/.../ExponentialApp.kt` },
  { kind: `tool`, name: `Read`, detail: `app/src/main/.../MainActivity.kt` },
  {
    kind: `tool`,
    name: `Grep`,
    detail: `subscribeShapes( · 6 matches across 3 files`,
  },
  {
    kind: `subagent`,
    id: `sub-startup-trace`,
    agentType: `Explore`,
    status: `started`,
    detail: `Map every await on the launch path`,
  },
  {
    kind: `tool`,
    name: `Bash`,
    detail: `./gradlew :app:startupProfile --variant=release`,
    subagentId: `sub-startup-trace`,
  },
  {
    kind: `subagent`,
    id: `sub-startup-trace`,
    agentType: `Explore`,
    status: `completed`,
    detail: `Shape subscribe blocks the first frame for 610ms`,
  },
  {
    kind: `narration`,
    text: `Confirmed: the shape subscribe awaits its initial snapshot before the first frame is drawn, which is 610ms of the 1.4s. Deferring it to after first paint and painting from the cached board snapshot should get us under the 800ms target.`,
  },
  {
    kind: `tool`,
    name: `Edit`,
    detail: `ui/board/BoardScreen.kt · defer subscribe to after first paint`,
  },
  {
    kind: `diff`,
    diff: `diff --git a/app/src/main/java/com/exponential/app/ui/board/BoardScreen.kt b/app/src/main/java/com/exponential/app/ui/board/BoardScreen.kt
--- a/app/src/main/java/com/exponential/app/ui/board/BoardScreen.kt
+++ b/app/src/main/java/com/exponential/app/ui/board/BoardScreen.kt
@@ -41,9 +41,13 @@ fun BoardScreen(boardId: String) {
-    // Blocks the first frame: we await the initial snapshot before drawing.
-    val issues = remember { runBlocking { sync.subscribeIssues(boardId) } }
+    // Paint from the cached snapshot immediately, then subscribe once the
+    // first frame is on screen, and the shape catches us up in place.
+    val issues by sync.cachedIssues(boardId).collectAsState()
+    LaunchedEffect(boardId) {
+        withFrameNanos { }
+        sync.subscribeIssues(boardId)
+    }

     BoardList(issues = issues)
 }
diff --git a/app/src/main/java/com/exponential/app/data/sync/BoardSnapshotCache.kt b/app/src/main/java/com/exponential/app/data/sync/BoardSnapshotCache.kt
--- a/app/src/main/java/com/exponential/app/data/sync/BoardSnapshotCache.kt
+++ b/app/src/main/java/com/exponential/app/data/sync/BoardSnapshotCache.kt
@@ -28,6 +28,11 @@ class BoardSnapshotCache(private val store: DataStore) {
     fun read(boardId: String): List<Issue> = store.read(key(boardId))

+    /** Persisted on ON_STOP so the next cold launch has something to paint. */
+    fun persist(boardId: String, issues: List<Issue>) {
+        store.write(key(boardId), issues)
+    }
+
     private fun key(boardId: String) = "board-snapshot-$boardId"
 }`,
  },
  {
    kind: `narration`,
    text: `The cached snapshot needs to survive a cold process, so the board reducer now writes its last state to the on-disk cache when the app backgrounds. That is the other half of the fix. Without it the first launch after a reboot still paints empty.`,
  },
  { kind: `tool`, name: `Read`, detail: `data/sync/BoardSnapshotCache.kt` },
  {
    kind: `tool`,
    name: `Edit`,
    detail: `data/sync/BoardSnapshotCache.kt · persist on ON_STOP`,
  },
  {
    kind: `subagent`,
    id: `sub-regression-sweep`,
    agentType: `Explore`,
    status: `started`,
    detail: `Find every caller that assumed a synchronous subscribe`,
  },
  {
    kind: `tool`,
    name: `Grep`,
    detail: `subscribeIssues( · 4 call sites`,
    subagentId: `sub-regression-sweep`,
  },
  {
    kind: `tool`,
    name: `Read`,
    detail: `ui/issue/IssueListScreen.kt`,
    subagentId: `sub-regression-sweep`,
  },
  {
    kind: `subagent`,
    id: `sub-regression-sweep`,
    agentType: `Explore`,
    status: `completed`,
    detail: `3 call sites already tolerate an empty first emission; 1 needed a guard`,
  },
  {
    kind: `narration`,
    text: `One caller assumed the first emission was the full list and skipped its empty state. Guarded it so the board never flashes "No issues" between the cached paint and the live snapshot.`,
  },
  {
    kind: `tool`,
    name: `Bash`,
    detail: `./gradlew :app:testProductionDebugUnitTest · 214 passed`,
  },
  {
    kind: `tool`,
    name: `Bash`,
    detail: `./gradlew :app:startupProfile --variant=release · p50 740ms, p90 812ms`,
  },
  {
    kind: `narration`,
    text: `Cold start is down to 740ms on the Pixel 6a, so we're under target. The markdown editor is still eagerly loaded and worth another ~90ms. Want me to lazy-load it in this same PR?`,
  },
  {
    kind: `question`,
    id: `q-lazy-markdown`,
    text: DEMO_FEED_QUESTION,
    options: [
      {
        label: `Also lazy-load the markdown editor`,
        key: `1`,
        description: `~90ms more, touches the editor entry point`,
      },
      {
        label: `Open the PR with what we have`,
        key: `2`,
        description: `Target is met; keep the diff small`,
      },
    ],
  },
]

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
  // The seeded showcase session: the demo user's own RUNNING one, which is the
  // only session the apps let them watch (EXP-312, owner-only).
  const [session] = await db
    .select({ id: codingSessions.id, teamId: codingSessions.teamId })
    .from(codingSessions)
    .where(
      and(
        eq(codingSessions.userId, demo.id),
        eq(codingSessions.status, `running`)
      )
    )
    .limit(1)
  if (!session) {
    throw new Error(
      `no running coding session for ${DEMO_EMAIL} — run seed:screenshots first`
    )
  }
  return { userId: demo.id, sessionId: session.id, teamId: session.teamId }
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
    let keepalive: ReturnType<typeof setInterval> | undefined

    ws.onopen = () => {
      console.log(`[${opts.name}] connected`)
      opts.onOpen(send)
      // `resize` is the relay's deliberate parsed-then-ignored frame, and any
      // inbound message bumps the connection's liveness timestamp — so it is
      // the quietest keepalive available on a text-only protocol.
      keepalive = setInterval(() => send({ t: `resize`, cols: 120, rows: 32 }), KEEPALIVE_MS)
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
      if (keepalive) clearInterval(keepalive)
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
  const config = required(getSteerRelayConfig())
  const { userId, sessionId, teamId } = await resolveTarget()

  const stopControl = hold({
    name: `control`,
    config,
    seed: { kind: `control`, userId, deviceLabel: DEMO_DEVICE_LABEL },
    onOpen: (send) =>
      send({
        t: `online`,
        deviceId: DEMO_DEVICE_ID,
        deviceLabel: DEMO_DEVICE_LABEL,
        agents: AGENTS,
        caps: CAPS,
      }),
    onFrame: (frame) => {
      if (frame.t === `start_session`) {
        // A real desktop would launch an agent and insert the coding_sessions
        // row. The capture only opens the dialog and cancels, so acknowledging
        // it in the log is enough — and a stray tap can't corrupt the seed.
        console.log(`[control] ignoring start_session:`, JSON.stringify(frame))
      }
    },
  })

  const stopPublisher = hold({
    name: `publisher`,
    config,
    seed: { kind: `publisher`, userId, teamId, sessionId },
    onOpen: (send) => {
      send({ t: `hello`, sessionId })
      // Full-history re-publish, exactly like the desktop does on reconnect:
      // reset first so the relay's replay log never doubles.
      send({ t: `activity_reset` })
      for (const event of FEED) send({ t: `activity`, event })
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
  const touchDevice = () =>
    db
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
        caps: CAPS,
        lastSeenAt: new Date(),
        activeSessions: 1,
      })
      .onConflictDoUpdate({
        target: [devices.userId, devices.deviceId],
        set: {
          lastSeenAt: new Date(),
          label: DEMO_DEVICE_LABEL,
          version: DEMO_DEVICE_VERSION,
          isDefault: true,
          agents: AGENTS,
          caps: CAPS,
        },
      })
      .catch((err) => console.error(`[device heartbeat]`, err))
  await touchDevice()

  const heartbeat = setInterval(() => {
    void touchDevice()
    void db
      .update(codingSessions)
      .set({ updatedAt: new Date() })
      .where(
        and(
          eq(codingSessions.userId, userId),
          ne(codingSessions.status, `ended`)
        )
      )
      .catch((err) => console.error(`[heartbeat]`, err))
  }, HEARTBEAT_MS)

  const shutdown = () => {
    clearInterval(heartbeat)
    stopControl()
    stopPublisher()
    process.exit(0)
  }
  process.on(`SIGINT`, shutdown)
  process.on(`SIGTERM`, shutdown)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
