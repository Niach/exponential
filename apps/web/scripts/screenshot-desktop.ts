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
 *   eNet node         → VAPP-43: the relay carries no session content any
 *                       more; every viewer reads a session over eNet from the
 *                       device running it. So a real eNet node
 *                       (apps/desktop/crates/steer/examples/enet_feed_stub.rs,
 *                       lib/enet-feed-stub.ts) is registered as the demo
 *                       machine's install and serves a scripted activity feed
 *                       for the seeded running session per
 *                       fixtures/steer-enet.json (join → activity_reset, the
 *                       replay, activity_synced, then live activity), ending
 *                       on an UNANSWERED question so the steering shot shows
 *                       "Needs your input" with tappable options. It needs
 *                       cargo and reaches viewers through relay.exponential.at.
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
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { comments, devices, issues, sessions, users } from "@/db/schema"
import {
  getSteerRelayConfig,
  mintSteerTicket,
  type SteerRelayConfig,
  type SteerTicketSeed,
} from "@/lib/steer"
import {
  assertDemoLiveSessions,
  DEMO_TASK_LIST,
  DEMO_TRACE_FEED,
  demoBackgroundTasks,
} from "./lib/demo-live-sessions"
import { reclockDemoRows } from "./lib/demo-reclock"
import { startEnetFeedStub } from "./lib/enet-feed-stub"
import { assertLocalDatabase } from "./lib/local-db-guard"
import {
  DEMO_AGENT_COMMENT_ID,
  DEMO_DEVICE_ID,
  DEMO_DEVICE_LABEL,
  DEMO_DEVICE_VERSION,
  DEMO_EMAIL,
  DEMO_FEED_QUESTION,
  DEMO_STEERED_SESSION_ID,
  DEMO_THREAD_ID,
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
// VAPP-168: the feed's live tail lands one frame per second after the stub
// starts (minutes before any capture reaches the steering view).
const LIVE_EVERY_MS = 1_000

// The scripted session: a plausible mid-run agent transcript for the showcase
// issue ("Reduce cold start below 800 ms"). Diff events REPLACE rather than
// append in a device's replay journal, so there is exactly one.
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
  // EXP-948: Exponential's own tool calls never fold into a generic tool run;
  // consecutive calls of ONE tool group under their own caption ("Read 3
  // issues"), which is what these three rows show in the steering shot.
  {
    kind: `tool`,
    name: `mcp__exponential__exponential_issues_get`,
    detail: `APP-5`,
  },
  {
    kind: `tool`,
    name: `mcp__exponential__exponential_issues_get`,
    detail: `APP-14`,
  },
  {
    kind: `tool`,
    name: `mcp__exponential__exponential_issues_get`,
    detail: `APP-16`,
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
    // EXP-916: the per-call patch rides the settle (`tool_update.diff`), so the
    // transcript's edited-files card has a real row to open.
    diff: `--- a/app/src/main/java/com/exponential/app/ui/board/BoardScreen.kt
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
`,
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
    diff: `--- a/app/src/main/java/com/exponential/app/data/sync/BoardSnapshotCache.kt
+++ b/app/src/main/java/com/exponential/app/data/sync/BoardSnapshotCache.kt
@@ -58,6 +58,12 @@ class BoardSnapshotCache(private val dir: File) {
     fun read(boardId: String): List<Issue>? = file(boardId).takeIf { it.exists() }?.let(::decode)
+
+    /** EXP-905: the last reduced state survives a cold process. */
+    fun persistOnStop(owner: LifecycleOwner, boardId: String, latest: () -> List<Issue>) {
+        owner.lifecycle.addObserver(LifecycleEventObserver { _, event ->
+            if (event == Lifecycle.Event.ON_STOP) write(boardId, latest())
+        })
+    }
`,
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
  // EXP-1324: subagents and the task list are Trace lines / THE Queue now,
  // so the `work-card` view photographs the one Work card the transcript
  // keeps: a workflow's (EXP-850 §3), patched onto its `Workflow` row.
  {
    kind: `tool`,
    id: `wf-launch-review`,
    name: `Workflow`,
    detail: `Review the cold-start diff`,
  },
  {
    kind: `workflow`,
    id: `wf-launch-review`,
    name: `cold-start-review`,
    description: `Review the cold-start diff`,
    status: `completed`,
    phases: [
      { index: 1, title: `Review` },
      { index: 2, title: `Verify` },
    ],
    agents: [
      { index: 1, label: `review:correctness`, phaseIndex: 1, state: `done`, model: `opus`, durationMs: 41_000, toolCalls: 9 },
      { index: 2, label: `review:performance`, phaseIndex: 1, state: `done`, model: `opus`, durationMs: 52_000, toolCalls: 12 },
      { index: 3, label: `verify:benchmarks`, phaseIndex: 2, state: `done`, model: `sonnet`, durationMs: 64_000, toolCalls: 7 },
    ],
    summary: `No blocking findings; the A54 p90 sits 12 ms over target`,
  },
  // EXP-1279: a code-mode `exec` call, the `script-card` view. The settle
  // carries the call log the code-mode server recorded (wire doc §1b in
  // apps/steer-relay/STEER-WIRE-EXP-850.md), so the card groups it by tool.
  {
    kind: `tool`,
    name: `mcp__codemode__exec`,
    toolKind: `script`,
    script: {
      count: 13,
      elapsedMs: 2_140,
      calls: [
        ...[`APP-5`, `APP-9`, `APP-10`, `APP-14`, `APP-16`, `APP-17`].map((subject, index) => ({
          tool: `exponential.exponential_issues_get`,
          subject,
          ok: true,
          ms: 180 + index * 20,
        })),
        ...[`APP-5`, `APP-9`, `APP-10`, `APP-14`, `APP-16`, `APP-17`].map((subject, index) => ({
          tool: `exponential.exponential_comments_list`,
          subject,
          ok: true,
          ms: 140 + index * 15,
        })),
        { tool: `exponential.exponential_issues_pr_files`, subject: `APP-9`, ok: false, ms: 95 },
      ],
      source: `const ids = ["APP-5", "APP-9", "APP-10", "APP-14", "APP-16", "APP-17"]
const issues = await Promise.all(
  ids.map((id) => tools.exponential.exponential_issues_get({ id }))
)
const threads = await Promise.all(
  ids.map((issueId) => tools.exponential.exponential_comments_list({ issueId }))
)
const files = await tools.exponential.exponential_issues_pr_files({ issueId: "APP-9" })
return issues.map((issue, i) => \`\${issue.identifier} \${threads[i].length} comments\`)`,
    },
  },
  // EXP-1241: the agent's comment on the image-cache issue, the
  // `comment-call-card` view. `preview` is filled in at startup from the
  // seeded row (DEMO_AGENT_COMMENT_ID), so the card renders the real comment.
  {
    kind: `tool`,
    name: `mcp__exponential__exponential_comments_create`,
    agentComment: true,
  },
  // EXP-1324: the Trace tail (a thought, a command with its Output, an
  // Explore subagent, a failing benchmark run), shared with the natives.
  ...DEMO_TRACE_FEED,
  {
    kind: `narration`,
    text: `Cold start is down to 740ms on the Pixel 6a, so we're under target.\n\n| Device | Before | After |\n| --- | --- | --- |\n| Pixel 6a | 1.4 s | 740 ms |\n| Galaxy A54 | 1.6 s | 790 ms |\n\nThe markdown editor is still eagerly loaded and worth another ~90ms. Want me to lazy-load it in this same PR?`,
  },
  // EXP-877: the transcript composer's footer reads two latest-wins slots —
  // `config_state` (the `model` option = the alias in force, `currentMode` =
  // the "Plan mode" caption) and `usage` (the context ring's percent). Both
  // ride the replay after the log, so a joining viewer paints them at once.
  {
    kind: `config_state`,
    options: [{ id: `model`, label: `Model`, value: `fable` }],
    currentMode: `plan`,
    modes: [
      { id: `default`, label: `Default` },
      { id: `plan`, label: `Plan` },
    ],
  },
  { kind: `usage`, contextUsed: 122_000, contextSize: 200_000, costUsd: 1.42 },
  // EXP-1051: the breakdown behind the usage popover's segmented bar — the
  // same latest-wins slot family as `usage`; conversation/free are derived
  // by every client from the meter above.
  {
    kind: `context_layout`,
    segments: [
      { key: `base`, tokens: 21_000, source: `measured` },
      { key: `tools`, tokens: 2_400, source: `estimated` },
      { key: `playbook`, tokens: 1_500, source: `estimated` },
      { key: `team`, tokens: 800, source: `estimated` },
      { key: `project`, tokens: 9_800, source: `estimated`, detail: `CLAUDE.md` },
      { key: `task`, tokens: 600, source: `estimated` },
    ],
  },
  // EXP-927/1321: the agent's own task list and the CLI's background work —
  // THE Queue at the bottom ("3 to do" open, "2 completed" folded). The
  // background task's `startedAt` is stamped at publish time (2m 10s ago).
  { kind: `task_list`, entries: DEMO_TASK_LIST },
  { kind: `background_tasks`, demoBackgroundTasks: true },
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

// EXP-785: a `tool` row needs its ACP `id` and contract `toolKind` (the hub
// drops it otherwise, `event.id: invalid_type`), and settles through a
// `tool_update` folded in by that id. The feed above stays readable by name;
// the wire shape is stamped here. `seq` numbers every published event.
const TOOL_KIND_BY_NAME: Record<string, string> = {
  Read: `read`,
  Grep: `search`,
  Glob: `search`,
  Bash: `execute`,
  Edit: `edit`,
  Write: `edit`,
}

/** The settle of the scripted comment call: the seeded comment and its issue
 *  (`resolveAgentComment`), as the engine's preview mapper would name them. */
interface AgentComment {
  detail: string
  preview: Record<string, unknown>
}

// EXP-1284: the scripted feed is stamped on a timeline that ENDS at capture
// time (`at`, epoch ms; without it every client stamps the whole feed on
// receipt), and the demo thread's Guide entries are re-stamped into that
// window just before the closing question, so the thread page reads in real
// order: the prompt, the work, the sections the session filed, the question.
const FEED_WINDOW_MS = 25 * 60_000
const FEED_END_GAP_MS = 60_000
const GUIDE_BEFORE_END_MS = [4 * 60_000, 3 * 60_000]

function feedTimeline(count: number, now: number): (index: number) => number {
  const start = now - FEED_WINDOW_MS
  const span = FEED_WINDOW_MS - FEED_END_GAP_MS
  return (index) => Math.round(start + (span * index) / Math.max(count - 1, 1))
}

/** Re-stamps the steered session's Guide entries on DEMO_THREAD_ID (the
 *  root's `guide`, EXP-1293) inside the feed's window (oldest first).
 *  Best-effort: a missing thread (an older seed) leaves the capture as it
 *  was. */
async function restampDemoGuide(now: number): Promise<void> {
  try {
    const [thread] = await db
      .select({ guide: sessions.guide })
      .from(sessions)
      .where(eq(sessions.id, DEMO_THREAD_ID))
      .limit(1)
    if (!Array.isArray(thread?.guide) || thread.guide.length === 0) return
    let next = 0
    const guide = thread.guide.map((entry) => {
      if (entry.sessionId !== DEMO_STEERED_SESSION_ID) return entry
      const before =
        GUIDE_BEFORE_END_MS[Math.min(next++, GUIDE_BEFORE_END_MS.length - 1)]!
      return { ...entry, at: now - FEED_END_GAP_MS - before }
    })
    await db.update(sessions).set({ guide }).where(eq(sessions.id, DEMO_THREAD_ID))
  } catch (err) {
    console.warn(`[feed] could not re-stamp the demo thread's Guide:`, err)
  }
}

/** The scripted feed as `activity {event, seq}` frames, stamped on a timeline
 *  ending now: what the eNet stub records and replays (VAPP-168). */
function feedFrames(agentComment: AgentComment | null): Record<string, unknown>[] {
  const frames: Record<string, unknown>[] = []
  const send = (frame: Record<string, unknown>) => frames.push(frame)
  let seq = 0
  let toolIndex = 0
  const now = Date.now()
  const at = feedTimeline(FEED.length, now)
  void restampDemoGuide(now)
  // EXP-877: a pending question hides the composer (EXP-820), so a capture of
  // the composer FOOTER (model picker, Plan mode, context ring) opts out of
  // the closing ask. Capture-only; the catalog's `steering` keeps the ask.
  const skipQuestion = process.env.SCREENSHOT_FEED_NO_QUESTION === `1`
  for (const [index, entry] of FEED.entries()) {
    if (skipQuestion && entry.kind === `question`) continue
    // Without the seeded comment (an older seed) the call is left out rather
    // than published as a card naming a comment nobody can open.
    if (entry.agentComment === true && !agentComment) continue
    const { agentComment: _marker, atFromEndMs, ...rest } = entry as typeof entry & {
      atFromEndMs?: number
    }
    // EXP-1324: a stamp pinned to the capture clock (a thought's or a
    // subagent's edges, whose durations read off two stamps).
    const stamp = typeof atFromEndMs === `number` ? now - atFromEndMs : at(index)
    if (rest.demoBackgroundTasks === true) {
      send({
        t: `activity`,
        event: { kind: `background_tasks`, tasks: demoBackgroundTasks(now), at: stamp },
        seq: seq++,
      })
      continue
    }
    const raw: Record<string, unknown> =
      entry.agentComment === true && agentComment
        ? { ...rest, detail: agentComment.detail, preview: agentComment.preview }
        : rest
    if (raw.kind !== `tool`) {
      send({ t: `activity`, event: { ...raw, at: stamp }, seq: seq++ })
      continue
    }
    const id = typeof raw.id === `string` ? raw.id : `stub-tool-${++toolIndex}`
    const toolKind =
      typeof raw.toolKind === `string`
        ? raw.toolKind
        : (TOOL_KIND_BY_NAME[String(raw.name)] ?? `other`)
    // EXP-916: an edit's patch is a `tool_update` field, never on the `tool`
    // row itself — the same wire shape the engine's mapper produces. So are
    // an Exponential call's `preview` (EXP-846) and a script's log (EXP-1279).
    // EXP-1324: so are a command's `output` and a failed settle.
    const { diff, preview, script, output, failed, ...call } = raw as typeof raw & {
      diff?: string
      preview?: unknown
      script?: unknown
      output?: string
      failed?: boolean
    }
    send({ t: `activity`, event: { ...call, id, toolKind, at: stamp }, seq: seq++ })
    send({
      t: `activity`,
      event: {
        kind: `tool_update`,
        id,
        status: failed === true ? `failed` : `completed`,
        ...(diff ? { diff } : {}),
        ...(output ? { output } : {}),
        ...(preview ? { preview } : {}),
        ...(script ? { script } : {}),
      },
      seq: seq++,
    })
  }
  return frames
}

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
 * The seeded comment the scripted `exponential_comments_create` call names
 * (EXP-1259 `comment-call-card`): its id, its body as the ref's title (cut to
 * the preview cap like the engine does) and its issue, by identifier.
 */
/** The engine clamps every preview string to this (contract `expToolPreview`). */
const TOOL_PREVIEW_TEXT_MAX = contract.expToolPreview.textMax

async function resolveAgentComment(): Promise<AgentComment | null> {
  const [row] = await db
    .select({ id: comments.id, body: comments.body, issueId: issues.id, identifier: issues.identifier })
    .from(comments)
    .innerJoin(issues, eq(issues.id, comments.issueId))
    .where(eq(comments.id, DEMO_AGENT_COMMENT_ID))
    .limit(1)
  if (!row) {
    console.warn(
      `[feed] no seeded agent comment (${DEMO_AGENT_COMMENT_ID}) — the comment call is left out; re-run seed:screenshots`
    )
    return null
  }
  const identifier = row.identifier ?? row.issueId
  const title = row.body.slice(0, TOOL_PREVIEW_TEXT_MAX)
  return {
    detail: identifier,
    preview: {
      refs: [
        { kind: `comment`, id: row.id, title },
        { kind: `issue`, id: row.issueId, identifier },
      ],
    },
  }
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
  const agentComment = await resolveAgentComment()

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

  // VAPP-168: the feed is served over eNet by the install of the machine the
  // session runs on (the devices row above). Everything up to the agent's
  // closing message is recorded before any viewer joins; that message, the
  // footer slots and the question then land live, so a viewer that joined
  // early sees the feed grow like a running session's.
  const frames = feedFrames(agentComment)
  const closing = frames.reduce<number>(
    (last, frame, index) =>
      (frame.event as { kind?: string } | undefined)?.kind === `narration` ? index : last,
    0
  )
  const feedStub = await startEnetFeedStub({
    userId,
    deviceId: DEMO_DEVICE_ID,
    label: DEMO_DEVICE_LABEL,
    feed: {
      sessionId,
      journal: frames.slice(0, closing),
      live: frames.slice(closing),
      liveEveryMs: LIVE_EVERY_MS,
    },
  })

  console.log(`
Screenshot desktop online:
  device      ${DEMO_DEVICE_LABEL} (${AGENTS.join(`, `)})
  session     ${sessionId}
  relay       ${config.url}
  eNet        ${feedStub.endpointId}

Leave this running for the whole fastlane capture. Ctrl-C to stop.
`)

  const heartbeat = setInterval(() => {
    void touchDevice()
    void feedStub.touch().catch((err) => console.error(`[enet heartbeat]`, err))
    void assertDemoLiveSessions().catch((err) => console.error(`[heartbeat]`, err))
    // EXP-913: keep the seeded "X ago" labels at their seeded age while the
    // lanes run (lib/demo-reclock.ts).
    void reclockDemoRows().catch((err) => console.error(`[reclock]`, err))
  }, HEARTBEAT_MS)

  const shutdown = () => {
    clearInterval(heartbeat)
    stopControl()
    feedStub.stop()
    process.exit(0)
  }
  process.on(`SIGINT`, shutdown)
  process.on(`SIGTERM`, shutdown)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
