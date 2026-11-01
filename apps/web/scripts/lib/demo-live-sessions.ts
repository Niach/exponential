/**
 * The seeded LIVE sessions, as one re-assertable spec (EXP-913, EXP-1293).
 *
 * Five `sessions` rows are live on purpose: the cold-start thread's root and
 * its child "Profile the launch path" (the `thread`, `session` and `steering`
 * views; the child waits on a question), the APP-14 thread (busy), the FB-9
 * thread the Triage agent started (busy) and the private offline-queue spike.
 * A full five-lane refresh runs for hours, and a capture may stop or answer a
 * session, so the seed writes their state through this spec and the stub
 * (screenshot-desktop.ts) re-asserts it at boot and on every heartbeat: a row
 * a capture moved goes back to its seeded status, and `started_at` is
 * re-anchored to the current clock so "1h 00m" reads the same on the last lane
 * as on the first.
 */
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { sessions, teams } from "@/db/schema"
import { DEMO_SESSION_ID, DEMO_THREAD_IDS, TEAM_SLUG } from "../screenshot-demo"

export interface LiveSessionSpec {
  id: string
  status: `running` | `idle`
  /** Device-written turn state: the list spinner. */
  busy: boolean
  startedMinutesAgo: number
  /** The stub's feed for this row ends on an unanswered question
   *  (screenshot-desktop.ts) and the thread carries the question card, so
   *  the row says so the way a real device stamps it: `needs_input`. */
  asks?: boolean
}

export const DEMO_LIVE_SESSIONS: readonly LiveSessionSpec[] = [
  { id: DEMO_THREAD_IDS.coldStart, status: `running`, busy: false, startedMinutesAgo: 60 },
  { id: DEMO_SESSION_ID, status: `running`, busy: false, startedMinutesAgo: 52, asks: true },
  { id: DEMO_THREAD_IDS.groupByAssignee, status: `running`, busy: true, startedMinutesAgo: 25 },
  { id: DEMO_THREAD_IDS.sortReports, status: `running`, busy: true, startedMinutesAgo: 8 },
  { id: DEMO_THREAD_IDS.offlineSpike, status: `running`, busy: false, startedMinutesAgo: 40 },
]

/** A capture that opts out of the closing ask (SCREENSHOT_FEED_NO_QUESTION,
 *  or EXP-1501's SCREENSHOT_FEED_WRITING, which ends on a running turn) has
 *  nothing pending, so the row is not waiting either. */
export function demoNeedsInput(spec: LiveSessionSpec): boolean {
  return (
    spec.asks === true &&
    process.env.SCREENSHOT_FEED_NO_QUESTION !== `1` &&
    process.env.SCREENSHOT_FEED_WRITING !== `1`
  )
}

/**
 * Put the live rows back to their seeded state. Returns how many `sessions`
 * rows it found; 0 means the demo team is not seeded, which the caller
 * reports rather than this module guessing at a fix.
 */
export async function assertDemoLiveSessions(now: number = Date.now()): Promise<number> {
  const [team] = await db
    .select({ id: teams.id })
    .from(teams)
    .where(eq(teams.slug, TEAM_SLUG))
    .limit(1)
  if (!team) return 0

  let written = 0
  for (const spec of DEMO_LIVE_SESSIONS) {
    const rows = await db
      .update(sessions)
      .set({
        status: spec.status,
        busy: spec.busy,
        needsInput: demoNeedsInput(spec),
        startedAt: new Date(now - spec.startedMinutesAgo * 60_000),
        endedAt: null,
        lastTurnAt: new Date(now),
        updatedAt: new Date(now),
      })
      .where(and(eq(sessions.id, spec.id), eq(sessions.teamId, team.id)))
      .returning({ id: sessions.id })
    written += rows.length
  }

  return written
}

// ── EXP-1324: the Trace + Queue tail of "Profile the launch path" ──────────
// The scripted feed (screenshot-desktop.ts `FEED`) splices these in right
// before the closing narration, so the `session` view's tail reads like the
// approved draft: "Thought for 5s", "Ran ./gradlew :app:startupProfile" with
// its Output, "Ran the Explore agent" (1m 12s), the red "Failed: 1 of 12
// tests in StartupBenchmark", then the Queue. The native lanes read the same
// constants for their own demo feeds. `atFromEndMs` pins an event's `at`
// (epoch ms) to the capture clock, where a duration depends on two stamps.

/** What `./gradlew :app:startupProfile` printed (the Output block). */
export const DEMO_PROFILE_OUTPUT = [
  `> Task :app:startupProfile`,
  `Pixel 6a     p50 740 ms   p90 812 ms`,
  `Galaxy A54   p50 790 ms   p90 840 ms`,
  `BUILD SUCCESSFUL in 41s`,
].join(`\n`)

/** What the failing benchmark run printed (gradle's summary + its failure
 *  line, so `parseTestResults` reads 1 of 12 in StartupBenchmark). */
export const DEMO_BENCHMARK_OUTPUT = [
  `> Task :benchmark:connectedBenchmarkAndroidTest`,
  ``,
  `StartupBenchmark > coldStart FAILED`,
  `    java.lang.AssertionError: p90 812 ms > 800 ms`,
  `        at com.exponential.benchmark.StartupBenchmark.coldStart(StartupBenchmark.kt:42)`,
  ``,
  `12 tests completed, 1 failed`,
  ``,
  `FAILURE: Build failed with an exception.`,
].join(`\n`)

/** The Explore subagent's wall time on the badge: 1m 12s. */
export const DEMO_EXPLORE_MS = 72_000

/** The tail events, in feed order. Tool rows use the stub's shorthand
 *  (`output`/`failed` ride the settle, the toolKind comes from the name). */
export const DEMO_TRACE_FEED: readonly Record<string, unknown>[] = [
  { kind: `thought`, id: `th-launch-path`, state: `started`, atFromEndMs: 9 * 60_000 },
  {
    kind: `thought`,
    id: `th-launch-path`,
    state: `ended`,
    durationMs: 5_200,
    atFromEndMs: 9 * 60_000 - 5_200,
  },
  {
    kind: `tool`,
    name: `Bash`,
    detail: `./gradlew :app:startupProfile`,
    output: DEMO_PROFILE_OUTPUT,
  },
  {
    kind: `subagent`,
    id: `sub-galaxy-numbers`,
    agentType: `Explore`,
    title: `Collect the Galaxy A54 traces`,
    status: `started`,
    atFromEndMs: 7 * 60_000,
  },
  {
    kind: `tool`,
    name: `Read`,
    detail: `benchmark/StartupBenchmark.kt`,
    subagentId: `sub-galaxy-numbers`,
  },
  {
    kind: `subagent`,
    id: `sub-galaxy-numbers`,
    agentType: `Explore`,
    title: `Collect the Galaxy A54 traces`,
    status: `completed`,
    detail: `p90 sits 12 ms over target on the A54 only`,
    toolCalls: 6,
    atFromEndMs: 7 * 60_000 - DEMO_EXPLORE_MS,
  },
  {
    kind: `tool`,
    name: `Bash`,
    detail: `./gradlew :benchmark:connectedBenchmarkAndroidTest`,
    output: DEMO_BENCHMARK_OUTPUT,
    failed: true,
  },
]

/** The agent's task list: 2 completed, 1 in progress, 1 pending. */
export const DEMO_TASK_LIST = [
  { content: `Profile the cold start`, status: `completed` },
  { content: `Defer the snapshot cache warm-up`, status: `completed` },
  { content: `Measuring on the Galaxy A54`, status: `in_progress` },
  { content: `Open the PR with the numbers`, status: `pending` },
] as const

/** How long the background capture has been running at capture time. */
export const DEMO_BACKGROUND_ELAPSED_MS = 130_000

/** The CLI's background work: one shell task, started 2m 10s ago. */
export function demoBackgroundTasks(now: number = Date.now()) {
  return [
    {
      id: `bg-web-captures`,
      kind: `shell`,
      description: `Rerun the web and web-phone captures`,
      startedAt: now - DEMO_BACKGROUND_ELAPSED_MS,
    },
  ]
}
