/**
 * Seed a deterministic, good-looking demo team for automated store
 * screenshots (fastlane snapshot on iOS, screengrab on Android).
 *
 * The mobile UI tests log in as the demo user against a LOCAL backend
 * (simulator: http://localhost:5173, emulator: http://10.0.2.2:5173) and
 * walk the main screens — this script guarantees what they see: a busy
 * board, a rich issue with markdown + comments, an inbox with unread
 * notifications, and issues assigned to the demo user.
 *
 * Idempotent: re-running tears down the demo team AND the demo users,
 * then rebuilds everything, so relative dates ("due in 2 days", "3h ago")
 * always look fresh. The USER ids and the coding-session ids are pinned
 * (EXP-913): an avatar's hue hashes its user id, so fresh ids re-rolled every
 * avatar per seed. Reused user-scoped shapes are fine since the capture stack
 * serves the built app, whose shape responses carry the cursor headers
 * (capture-all.ts `assertShapesSyncable`), so clients follow the delete +
 * re-insert like any other change.
 *
 * Seeding is only half of it: three of the eight shots (Start-coding dialog,
 * live steering, the "Coding now" row) need a steer relay with a desktop online
 * on it, which `screenshot-desktop.ts` provides — run that next and leave it
 * running for the capture.
 *
 * The teardown is destructive, so the script refuses a DATABASE_URL that is
 * not obviously local (see assertLocalDatabase) unless `--allow-remote`.
 *
 * Usage (from apps/web, local dev env with password signup enabled):
 *   bun run seed:screenshots
 *   bun run screenshots:desktop   # second shell, stays up during the capture
 */
import { and, eq, inArray, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { storeOauthTokens } from "@/lib/mcp-oauth/credentials"
import {
  accounts,
  actions,
  apikeys,
  attachments,
  codingSessions,
  comments,
  devices,
  issueDrafts,
  issueEvents,
  issueLabels,
  issueRelations,
  issues,
  issueStatuses,
  issueSubscribers,
  labels,
  mcpServers,
  notifications,
  boards,
  repositories,
  teamInvites,
  users,
  teamMembers,
  teams,
  threadItems,
  threads,
  widgetConfigs,
  widgetSubmissions,
} from "@/db/schema"
import type { ActionTrigger, CodingSessionResult } from "@exp/db-schema/domain"
import { auth } from "@/lib/auth"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
} from "@/lib/storage/issue-attachments"
import { generateWidgetKey } from "@/lib/widget/key"
import { BUILTIN_CHAT_NAME } from "@/lib/builtin-actions"
import { assertDemoLiveSessions, DEMO_LIVE_SESSIONS } from "./lib/demo-live-sessions"
import { DEMO_CLOCK_ANCHOR } from "./lib/demo-reclock"
import { parseFreezeNow } from "./lib/freeze-now"
import { assertLocalDatabase } from "./lib/local-db-guard"
import {
  DEMO_API_KEYS,
  DEMO_ATTACHMENT_DATES,
  DEMO_ACTION_ID,
  DEMO_DEVICE_ID,
  DEMO_SERVER_DEVICE_ID,
  DEMO_DEVICE_LABEL,
  DEMO_DEVICE_VERSION,
  DEMO_EMAIL,
  DEMO_INVITE_TOKEN,
  DEMO_PENDING_INVITE_EXPIRY,
  DEMO_PINNED_PAST_DATES,
  DEMO_NAME,
  DEMO_PASSWORD,
  DEMO_DUE_DATES,
  DEMO_SERVER_VERSION,
  DEMO_SHOWCASE_COMMENT_HOURS_AGO,
  DEMO_SESSION_IDS,
  DEMO_TRIGGER_IDS,
  DEMO_TIMEZONE,
  DEMO_USER_ID,
  DEMO_BLOCKS,
  DEMO_STEERED_SESSION_ID,
  DEMO_THREAD_ID,
  DEMO_THREAD_RUNS_ID,
  DEMO_THREAD_RUNS_TITLE,
  EMPTY_BOARD_SLUG,
  NEWCOMER_EMAIL,
  NEWCOMER_NAME,
  NEWCOMER_PASSWORD,
  NEWCOMER_USER_ID,
  STARTER_EMAIL,
  STARTER_NAME,
  STARTER_PASSWORD,
  STARTER_USER_ID,
  STARTER_TEAM_NAME,
  STARTER_TEAM_SLUG,
  DEMO_WIDGET_ISSUE_ID,
  WIDGET_REPORT_TITLE,
  WIDGET_REPORTER_EMAIL,
  WIDGET_REPORTER_NAME,
  TEAM_SLUG,
} from "./screenshot-demo"
import { escapeReporterText } from "@/lib/reporter-text"

const TEAMMATES = [
  { id: `demo-mira`, name: `Mira Chen`, email: `mira@acme.dev` },
  { id: `demo-jonas`, name: `Jonas Weber`, email: `jonas@acme.dev` },
  { id: `demo-sofia`, name: `Sofia Almeida`, email: `sofia@acme.dev` },
] as const

// The Review screenshot shows a REAL diff: `issues.prFiles` fetches the changed
// files from GitHub, so the fictional acme/mobile-app PR URLs render an empty
// file list. Safe to point one issue at a public PR because the Changes screen
// displays neither the repo name nor the PR number — only the branch
// (`exp/APP-14`), the state capsule, the file counts and the patches
// (ChangesView.swift / ChangesScreen.kt) — so the demo fiction survives intact.
//
// Pinned to a repo the Exponential GitHub App is NOT installed on: prFiles then
// resolves no installation token, never reaches the team link-gate, and falls
// back to the unauthenticated public-repo path (lib/integrations/github-pr.ts).
// Now in Android is Google's Apache-2.0 sample app — public, stable, and its
// Kotlin diffs suit a mobile-app board. Override with SCREENSHOT_PR_URL.
const REVIEW_PR_URL =
  process.env.SCREENSHOT_PR_URL?.trim() ||
  `https://github.com/android/nowinandroid/pull/2117`
const REVIEW_PR_NUMBER = Number(REVIEW_PR_URL.match(/\/pull\/(\d+)/)?.[1])
if (!REVIEW_PR_NUMBER) {
  throw new Error(`SCREENSHOT_PR_URL must end in /pull/<number>: ${REVIEW_PR_URL}`)
}

// EXP-1204: the SECOND real PR — the one a seeded chat run opened for itself
// (`run-changes`, EXP-1194: an issue-less run's diff, read through
// `codingSessions.prFiles`). Two things that path does and `issues.prFiles`
// does not, and that pick the repo:
// - it insists the PR's repo is a `repositories` row OF THE TEAM, so the seed
//   registers the repo below (its name shows only on Settings → Repositories;
//   the Changes screens print branch, state and files, never the repo);
// - a team repo feeds `repositories.openPulls`, which lists EVERY open PR of
//   that GitHub repo in the Reviews queue as an external group. So the repo
//   must have no open PRs — and never grow one: an ARCHIVED repo. android/kotlin
//   is the archived KSP prototype (Apache-2.0, 0 open PRs for good); #125 is a
//   three-file Kotlin fix small enough for a phone. Must differ from
//   REVIEW_PR_URL: every client hides a run whose PR URL an open issue carries.
// Override with SCREENSHOT_RUN_PR_URL (keep the archived-repo rule).
const RUN_PR_URL =
  process.env.SCREENSHOT_RUN_PR_URL?.trim() ||
  `https://github.com/android/kotlin/pull/125`
const RUN_PR_MATCH = RUN_PR_URL.match(/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)$/)
if (!RUN_PR_MATCH) {
  throw new Error(
    `SCREENSHOT_RUN_PR_URL must be https://github.com/<owner>/<repo>/pull/<number>: ${RUN_PR_URL}`
  )
}
const RUN_PR_REPO = RUN_PR_MATCH[1]!
const RUN_PR_NUMBER = Number(RUN_PR_MATCH[2])
if (RUN_PR_URL === REVIEW_PR_URL) {
  throw new Error(`SCREENSHOT_RUN_PR_URL must name a different PR than SCREENSHOT_PR_URL`)
}

// Opt-in frozen clock (SCREENSHOT_FREEZE_NOW) — see lib/freeze-now.ts for why
// the capture pipeline deliberately does NOT set it.
const frozenNow = parseFreezeNow(process.env.SCREENSHOT_FREEZE_NOW)
if (frozenNow !== undefined) {
  console.log(`clock frozen at ${new Date(frozenNow).toISOString()} (SCREENSHOT_FREEZE_NOW)`)
}
const now = frozenNow ?? Date.now()
const daysAgo = (d: number) => new Date(now - d * 86_400_000)
const hoursAgo = (h: number) => new Date(now - h * 3_600_000)

/**
 * Attachment ids for the STORAGE settings view, minted up front so the seeded
 * markdown can embed them — exactly what the upload path does
 * (lib/storage/issue-attachment-upload.ts mints the id first, then derives the
 * storage key and the canonical `/api/attachments/{id}` URL from it).
 *
 * Only the storage manager photographs these rows (it renders a type icon and
 * the byte total, never the blob), so no object is uploaded. That is also why
 * the embedded ones live on quiet backlog issues rather than the showcase
 * issue: nothing that gets captured renders one of these images.
 */
const ATTACHMENT_IDS = {
  cacheHitRate: crypto.randomUUID(),
  cacheSketch: crypto.randomUUID(),
  voiceoverLabels: crypto.randomUUID(),
  contrastSweep: crypto.randomUUID(),
  auditPdf: crypto.randomUUID(),
  sysdiagnose: crypto.randomUUID(),
}

/**
 * A password identity under a PINNED user id (EXP-913). `signUpEmail` mints its
 * own id, so the seed writes the two rows it would have — the user and its
 * `credential` account — itself, hashing through Better Auth's own context so
 * sign-in verifies exactly as for a signed-up account. Also independent of
 * AUTH_SIGNUP_ENABLED, which only gates the public endpoint.
 */
async function createPasswordUser(spec: {
  id: string
  name: string
  email: string
  password: string
  onboardingCompletedAt: Date | null
}): Promise<string> {
  const context = await auth.$context
  const createdAt = daysAgo(60)
  await db.insert(users).values({
    id: spec.id,
    name: spec.name,
    email: spec.email,
    emailVerified: true,
    onboardingCompletedAt: spec.onboardingCompletedAt,
    timezone: DEMO_TIMEZONE,
    createdAt,
    updatedAt: createdAt,
  })
  await db.insert(accounts).values({
    id: `${spec.id}-credential`,
    accountId: spec.id,
    providerId: `credential`,
    userId: spec.id,
    password: await context.password.hash(spec.password),
    createdAt,
    updatedAt: createdAt,
  })
  return spec.id
}

async function ensureDemoUser(): Promise<string> {
  // Verified + onboarded so the apps go straight to the main UI.
  return createPasswordUser({
    id: DEMO_USER_ID,
    name: DEMO_NAME,
    email: DEMO_EMAIL,
    password: DEMO_PASSWORD,
    onboardingCompletedAt: daysAgo(30),
  })
}

/**
 * The team-less second identity (EXP-566). Verified so the app lets it in, but
 * `onboardingCompletedAt` stays NULL and it joins nothing — that is precisely
 * what makes `/onboarding` and `/invite/$token` render instead of redirecting.
 */
async function ensureNewcomerUser(): Promise<string> {
  return createPasswordUser({
    id: NEWCOMER_USER_ID,
    name: NEWCOMER_NAME,
    email: NEWCOMER_EMAIL,
    password: NEWCOMER_PASSWORD,
    onboardingCompletedAt: null,
  })
}

/**
 * The STARTER (EXP-725): like the newcomer, but the owner of ONE board-less
 * team, so the wizard resumes past the team step — the invite and devices
 * steps' capture identity. No boards (a board would complete onboarding), no
 * invites (they take seats and the invite shot must show the control), no
 * devices (the prune script clears what a desktop launch registers).
 */
async function ensureStarterUser(): Promise<string> {
  const id = await createPasswordUser({
    id: STARTER_USER_ID,
    name: STARTER_NAME,
    email: STARTER_EMAIL,
    password: STARTER_PASSWORD,
    onboardingCompletedAt: null,
  })
  // teardown already dropped a stale seed's team by its unique slug.
  const [team] = await db
    .insert(teams)
    .values({ name: STARTER_TEAM_NAME, slug: STARTER_TEAM_SLUG })
    .returning({ id: teams.id })
  await db
    .insert(teamMembers)
    .values({ teamId: team!.id, userId: id, role: `owner` })
  return id
}

async function ensureTeammates(): Promise<Record<string, string>> {
  const ids: Record<string, string> = {}
  for (const mate of TEAMMATES) {
    await db
      .insert(users)
      .values({
        id: mate.id,
        name: mate.name,
        email: mate.email,
        emailVerified: true,
        onboardingCompletedAt: daysAgo(60),
      })
      .onConflictDoNothing({ target: users.email })
    const [row] = await db
      .select()
      .from(users)
      .where(eq(users.email, mate.email))
      .limit(1)
    if (!row) throw new Error(`teammate ${mate.email} missing`)
    ids[mate.id] = row.id
  }
  return ids
}

// This script DESTROYS data before it writes any: `teardown` drops the whole
// `TEAM_SLUG` team, the demo users and the personal teams they solely own. That
// is only ever correct against a local dev database — the shared guard
// (`scripts/lib/local-db-guard.ts`, EXP-913) refuses anything else.
const SEED_CONSEQUENCE = `seed:screenshots DELETES the "${TEAM_SLUG}" team, the demo users and their personal teams.`

async function teardown() {
  const [ws] = await db
    .select()
    .from(teams)
    .where(eq(teams.slug, TEAM_SLUG))
    .limit(1)
  if (ws) {
    // boards.repository_id is ON DELETE RESTRICT — drop boards before the
    // team cascade reaches repositories.
    await db.delete(boards).where(eq(boards.teamId, ws.id))
    await db.delete(teams).where(eq(teams.id, ws.id))
  }

  // Recreate the demo users each run (under their pinned ids — see the
  // header). Drop teams where a demo user is the sole member
  // first (their auto-created personal teams would otherwise pile up).
  // The starter's team goes by slug too: the sole-member rule below misses a
  // member-less leftover, and the unique slug would then collide on reseed.
  await db.delete(teams).where(eq(teams.slug, STARTER_TEAM_SLUG))
  const emails = [
    DEMO_EMAIL,
    NEWCOMER_EMAIL,
    STARTER_EMAIL,
    ...TEAMMATES.map((m) => m.email),
  ]
  const demoUsers = await db
    .select({ id: users.id })
    .from(users)
    .where(
      or(
        inArray(users.email, emails),
        inArray(users.id, [DEMO_USER_ID, NEWCOMER_USER_ID, STARTER_USER_ID])
      )
    )
  const ids = demoUsers.map((u) => u.id)
  if (ids.length === 0) return
  const orphaned = await db
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .groupBy(teamMembers.teamId)
    .having(
      sql`count(*) = 1 and bool_and(${teamMembers.userId} in ${ids})`
    )
  for (const { teamId } of orphaned) {
    await db.delete(boards).where(eq(boards.teamId, teamId))
    await db.delete(teams).where(eq(teams.id, teamId))
  }
  await db.delete(users).where(inArray(users.id, ids))
}

/** EXP-1284: Jonas's laptop, the machine `thread-runs` Run 3 runs on. */
const JONAS_LAPTOP_DEVICE_ID = `screenshot-demo-jonas-laptop`
const JONAS_LAPTOP_LABEL = `Jonas's ThinkPad`
const SERVER_LABEL = `Acme build server`

/** A Guide text entry (EXP-933/1154), stamped like a `sessions_guide` write. */
function guideText(
  topic: string,
  text: string,
  at: Date,
  files?: string[],
  /** EXP-1284 (review): the session that wrote the entry — stamped
   *  server-side on every real write; a session block shows only its own. */
  sessionId?: string
): CodingSessionResult {
  return {
    topic,
    label: null,
    attachmentId: null,
    width: null,
    height: null,
    text,
    ...(files ? { files } : {}),
    at: at.getTime(),
    ...(sessionId ? { sessionId } : {}),
  }
}

/**
 * EXP-1284: the demo THREADS. Lists show threads, not bare runs, so every
 * seeded run must sit in one; two are planted by hand under pinned ids for the
 * `thread` and `thread-runs` views (see DEMO_THREAD_ID), and every other run
 * is placed afterwards exactly the way the migration backfill
 * (0166_threads.sql) placed existing runs.
 *
 * The threads' NEW issues go on Product Feedback, never Mobile App: a new
 * APP-* row would land on every board, search, my-issues and switcher shot,
 * while Product Feedback is the live board no view photographs. A filed issue
 * cannot be an old one restamped either: "Filed and linked" means the issue
 * was created at most `THREAD_FILED_WINDOW_MS` before the link, and every
 * seeded APP-* issue is days older than the showcase run. None of their
 * titles says "cold start" (the `search` view types it).
 *
 * Item `team_id`/`board_id` are trigger-derived for issue items; they are
 * written anyway, like `linkIssueToThread` does.
 */
async function seedThreads(ctx: {
  teamId: string
  demoId: string
  jonas: string
  sofia: string
  feedbackBoardId: string
  labelIds: Record<string, string>
  showcase: typeof issues.$inferSelect
  linkedByShowcase: typeof issues.$inferSelect
  linkedByServerChild: typeof issues.$inferSelect
}) {
  const { teamId, demoId, jonas, sofia, feedbackBoardId } = ctx
  const minutesAgo = (m: number) => new Date(now - m * 60_000)
  const ids = DEMO_SESSION_IDS

  // Thread 2's person-linked issue, Run 1's filed one, then thread 1's filed
  // one: inserted in this order so they number FB-1, FB-2, FB-3.
  const feedbackIssue = async (spec: {
    title: string
    description: string
    creatorId: string
    assigneeId?: string
    label: string
    createdAt: Date
    sortOrder: number
  }) => {
    const [row] = await db
      .insert(issues)
      .values({
        boardId: feedbackBoardId,
        teamId,
        title: spec.title,
        description: spec.description,
        status: `backlog`,
        priority: `medium`,
        creatorId: spec.creatorId,
        assigneeId: spec.assigneeId,
        sortOrder: spec.sortOrder,
        createdAt: spec.createdAt,
      })
      .returning()
    await db.insert(issueLabels).values({
      issueId: row.id,
      labelId: ctx.labelIds[spec.label],
      teamId,
      boardId: feedbackBoardId,
    })
    return row
  }

  // ── Thread 2 (`thread-runs`): Jonas's "Offline sync plan" ──────────────
  // Timeline: he opens the thread on a customer report (FB-1) and starts a
  // chat (Run 1), whose agent files FB-2; hours later he resumes it (Run 2),
  // and its agent starts one child per issue on two machines (Runs 3 and 4,
  // still live); Run 4's agent links the older APP-9 it builds on.
  const t0 = minutesAgo(300)
  const offlineReport = await feedbackIssue({
    title: `Edits made offline disappear after reconnecting`,
    description: `Changed an issue's status on the train, the app showed it saved, and once I was back online the old status came back.`,
    creatorId: sofia,
    assigneeId: jonas,
    label: `Bug`,
    createdAt: daysAgo(4),
    sortOrder: 0,
  })
  const conflictRules = await feedbackIssue({
    title: `Conflict rules for replaying queued offline edits`,
    description: `When a queued edit replays onto an issue somebody else changed meanwhile, keep their value per field and tell the user which of their edits lost.`,
    creatorId: jonas,
    assigneeId: jonas,
    label: `Feature`,
    createdAt: minutesAgo(278),
    sortOrder: 10,
  })
  const runsGuide = [
    guideText(
      `Summary`,
      `Planned offline sync for issue edits: edits queue on the device and replay in order once sync resumes, and a field somebody else changed meanwhile keeps their value. Filed #${conflictRules.identifier} for the conflict rules and split the build into two parallel runs.`,
      minutesAgo(46),
      undefined,
      ids.threadRunsResume
    ),
    guideText(
      `Edit queue`,
      `A persistent queue in the sync layer records every edit with the value it was based on. Replay drops an edit whose base no longer matches and raises a toast instead of overwriting the newer value.`,
      minutesAgo(45),
      [`src/sync/edit-queue.ts`, `src/sync/replay.ts`, `src/sync/replay.test.ts`],
      ids.threadRunsResume
    ),
  ]
  const chatBranch = `exp/chat-${ids.threadRunsChat.slice(0, 8)}`
  await db.insert(codingSessions).values([
    // Run 1: the ended chat that started it all (a person's start, so no
    // `started_reason`; the reserved `Chat` snapshot + agent title).
    {
      id: ids.threadRunsChat,
      teamId,
      userId: jonas,
      actionName: BUILTIN_CHAT_NAME,
      agentTitle: `Plan offline sync for issue edits`,
      deviceId: JONAS_LAPTOP_DEVICE_ID,
      deviceLabel: JONAS_LAPTOP_LABEL,
      agent: `claude`,
      status: `ended`,
      endedBy: `user`,
      branch: chatBranch,
      createdAt: minutesAgo(299),
      startedAt: minutesAgo(299),
      endedAt: minutesAgo(265),
      updatedAt: minutesAgo(265),
    },
    // Run 2: its RESUME (same worktree and branch), ended once it had
    // started the two children. Holds the Guide's compat mirror (EXP-1258):
    // it is the run that wrote it.
    {
      id: ids.threadRunsResume,
      teamId,
      userId: jonas,
      actionName: BUILTIN_CHAT_NAME,
      agentTitle: `Plan offline sync for issue edits`,
      resumedFromId: ids.threadRunsChat,
      deviceId: JONAS_LAPTOP_DEVICE_ID,
      deviceLabel: JONAS_LAPTOP_LABEL,
      agent: `claude`,
      status: `ended`,
      endedBy: `user`,
      branch: chatBranch,
      results: runsGuide,
      createdAt: minutesAgo(90),
      startedAt: minutesAgo(90),
      endedAt: minutesAgo(40),
      updatedAt: minutesAgo(40),
    },
    // Runs 3 and 4: LIVE agent-started children of Run 2 (`sessions_start`),
    // one per issue, on two machines. Fresh `updated_at` like every seeded
    // live row (clients hide a row past contract `staleHours`). Both hosts
    // have a devices row without `stale-end`, so the server sweep neither
    // flips nor deletes them (lib/coding-session-sweep.ts).
    {
      id: ids.threadRunsLaptopChild,
      teamId,
      userId: jonas,
      issueId: conflictRules.id,
      parentSessionId: ids.threadRunsResume,
      startedReason: `agent`,
      deviceId: JONAS_LAPTOP_DEVICE_ID,
      deviceLabel: JONAS_LAPTOP_LABEL,
      agent: `claude`,
      status: `running`,
      branch: `exp/${conflictRules.identifier}`,
      createdAt: minutesAgo(52),
      startedAt: minutesAgo(52),
      updatedAt: new Date(now),
    },
    {
      id: ids.threadRunsServerChild,
      teamId,
      userId: jonas,
      issueId: offlineReport.id,
      parentSessionId: ids.threadRunsResume,
      startedReason: `agent`,
      deviceId: DEMO_SERVER_DEVICE_ID,
      deviceLabel: SERVER_LABEL,
      agent: `claude`,
      status: `running`,
      branch: `exp/${offlineReport.identifier}`,
      createdAt: minutesAgo(51),
      startedAt: minutesAgo(51),
      updatedAt: new Date(now),
    },
  ])
  // Run 3's host. Not shared with the team, so no demo-user view lists it;
  // it exists so the sweep has a devices row to consult (see above).
  await db.insert(devices).values({
    userId: jonas,
    deviceId: JONAS_LAPTOP_DEVICE_ID,
    label: JONAS_LAPTOP_LABEL,
    kind: `desktop`,
    platform: `linux`,
    version: DEMO_DEVICE_VERSION,
    agents: [`claude`],
    lastSeenAt: new Date(now),
    createdAt: daysAgo(40),
  })
  await db.insert(threads).values({
    id: DEMO_THREAD_RUNS_ID,
    teamId,
    title: DEMO_THREAD_RUNS_TITLE,
    createdBy: jonas,
    results: runsGuide,
    createdAt: t0,
    updatedAt: minutesAgo(30),
  })
  const issueItem = (
    issue: typeof issues.$inferSelect,
    extra: {
      addedBy: string
      addedBySessionId?: string
      source: `user` | `agent`
      sortOrder: number
      at: Date
    }
  ) => ({
    kind: `issue` as const,
    issueId: issue.id,
    teamId,
    boardId: issue.boardId,
    addedBy: extra.addedBy,
    addedBySessionId: extra.addedBySessionId,
    source: extra.source,
    sortOrder: extra.sortOrder,
    createdAt: extra.at,
    updatedAt: extra.at,
  })
  const runItem = (sessionId: string, addedBy: string, sortOrder: number, at: Date) => ({
    kind: `run` as const,
    sessionId,
    teamId,
    addedBy,
    source: `user` as const,
    sortOrder,
    createdAt: at,
    updatedAt: at,
  })
  await db.insert(threadItems).values(
    [
      issueItem(offlineReport, { addedBy: jonas, source: `user`, sortOrder: 1, at: t0 }),
      runItem(ids.threadRunsChat, jonas, 2, minutesAgo(299)),
      // Filed by Run 1's agent 40 s after it created the issue.
      issueItem(conflictRules, {
        addedBy: jonas,
        addedBySessionId: ids.threadRunsChat,
        source: `agent`,
        sortOrder: 3,
        at: new Date(conflictRules.createdAt.getTime() + 40_000),
      }),
      runItem(ids.threadRunsResume, jonas, 4, minutesAgo(90)),
      runItem(ids.threadRunsLaptopChild, jonas, 5, minutesAgo(52)),
      runItem(ids.threadRunsServerChild, jonas, 6, minutesAgo(51)),
      // An older issue Run 4's agent LINKED (days old, so `Linked`).
      issueItem(ctx.linkedByServerChild, {
        addedBy: jonas,
        addedBySessionId: ids.threadRunsServerChild,
        source: `agent`,
        sortOrder: 7,
        at: minutesAgo(30),
      }),
    ].map((item) => ({ threadId: DEMO_THREAD_RUNS_ID, ...item }))
  )

  // ── Thread 1 (`thread`): the showcase run on APP-5 ─────────────────────
  // Untitled (derived: Run 1's name, the APP-5 title). Its agent linked the
  // older image-cache issue a few minutes in, then filed FB-3 and linked it.
  const showcaseStartMinutes = DEMO_LIVE_SESSIONS.find(
    (spec) => spec.id === DEMO_STEERED_SESSION_ID
  )!.startedMinutesAgo
  const showcaseStart = minutesAgo(showcaseStartMinutes)
  const editorBundle = await feedbackIssue({
    title: `Syntax highlighting ships every language in the editor bundle`,
    description: `The markdown editor imports the full highlighter, so its chunk carries grammars for every language. Load only the common ones and fetch the rest on demand.`,
    creatorId: demoId,
    label: `Performance`,
    createdAt: minutesAgo(showcaseStartMinutes - 22),
    sortOrder: 20,
  })
  // The Guide so far (the run is still live). Written to the THREAD only:
  // mirroring it onto the showcase row would also put it on the `steering`
  // shot for every client that still reads `coding_sessions.results`.
  await db.insert(threads).values({
    id: DEMO_THREAD_ID,
    teamId,
    title: null,
    createdBy: demoId,
    results: [
      guideText(
        `Summary`,
        `Deferred the shape subscribe until after the first frame and added a board snapshot cache that paints the last board before sync finishes: 740 ms on the CI device, under the 800 ms target. Filed #${editorBundle.identifier} for the editor's highlighting bundle.`,
        minutesAgo(12),
        undefined,
        DEMO_STEERED_SESSION_ID
      ),
      guideText(
        `Board snapshot cache`,
        `The last rendered board is written to disk on every sync and painted on launch, then replaced in place once the shape catches up.`,
        minutesAgo(11),
        [`src/board/snapshot-cache.ts`, `src/sync/bootstrap.ts`],
        DEMO_STEERED_SESSION_ID
      ),
    ],
    createdAt: showcaseStart,
    updatedAt: minutesAgo(showcaseStartMinutes - 22.5),
  })
  await db.insert(threadItems).values(
    [
      runItem(DEMO_STEERED_SESSION_ID, demoId, 1, showcaseStart),
      // The run's subject, linked by the start itself.
      issueItem(ctx.showcase, { addedBy: demoId, source: `user`, sortOrder: 2, at: showcaseStart }),
      issueItem(ctx.linkedByShowcase, {
        addedBy: demoId,
        addedBySessionId: DEMO_STEERED_SESSION_ID,
        source: `agent`,
        sortOrder: 3,
        at: minutesAgo(showcaseStartMinutes - 6),
      }),
      issueItem(editorBundle, {
        addedBy: demoId,
        addedBySessionId: DEMO_STEERED_SESSION_ID,
        source: `agent`,
        sortOrder: 4,
        at: minutesAgo(showcaseStartMinutes - 22.5),
      }),
    ].map((item) => ({ threadId: DEMO_THREAD_ID, ...item }))
  )

  // ── Every other run: one thread per resume chain, as the backfill did ──
  // (0166_threads.sql, scoped to this team and to runs still unplaced): id =
  // the chain's earliest run, its issues from issue_id + batch_issue_ids, the
  // Guide copied from the newest member holding one. One statement; the
  // sibling inserts' FK checks run at its end. One deviation, for the demo's
  // sake: a run's stamps are its START and its last ACTIVITY (ended_at, else
  // the heartbeat), because the seed leaves `created_at`/`updated_at` at the
  // insert instant on most rows, which would sort every backfilled thread
  // above the two pinned ones as "just now".
  await db.execute(sql`
    WITH RECURSIVE chain AS (
      SELECT id, id AS root FROM coding_sessions
        WHERE resumed_from_id IS NULL AND team_id = ${teamId}
      UNION ALL
      SELECT c.id, chain.root FROM coding_sessions c JOIN chain ON c.resumed_from_id = chain.id
    ),
    m AS (
      SELECT cs.id AS session_id, cs.team_id, cs.user_id, cs.issue_id, cs.batch_issue_ids,
        cs.results,
        least(cs.created_at, cs.started_at) AS created_at,
        coalesce(cs.ended_at, cs.updated_at) AS updated_at,
        first_value(cs.id) OVER w AS thread_id,
        row_number() OVER w AS ord
      FROM chain JOIN coding_sessions cs ON cs.id = chain.id
      WHERE cs.team_id = ${teamId}
        AND NOT EXISTS (SELECT 1 FROM thread_items ti WHERE ti.session_id = cs.id)
      WINDOW w AS (PARTITION BY chain.root ORDER BY least(cs.created_at, cs.started_at), cs.id)
    ),
    new_threads AS (
      INSERT INTO threads (id, team_id, title, created_by, results, created_at, updated_at)
      SELECT
        m.thread_id,
        m.team_id,
        NULL,
        (SELECT f.user_id FROM m f WHERE f.thread_id = m.thread_id AND f.ord = 1),
        (SELECT r.results FROM m r
           WHERE r.thread_id = m.thread_id AND jsonb_typeof(r.results) = 'array'
             AND jsonb_array_length(r.results) > 0
           ORDER BY r.created_at DESC, r.session_id DESC LIMIT 1),
        min(m.created_at),
        max(m.updated_at)
      FROM m
      GROUP BY m.thread_id, m.team_id
      RETURNING id
    ),
    run_items AS (
      INSERT INTO thread_items (thread_id, kind, session_id, team_id, added_by, source, sort_order, created_at, updated_at)
      SELECT m.thread_id, 'run', m.session_id, m.team_id, m.user_id, 'user', m.ord, m.created_at, m.created_at
      FROM m
      RETURNING id
    )
    INSERT INTO thread_items (thread_id, kind, issue_id, team_id, board_id, board_deleted_at, board_archived_at, added_by, source, sort_order, created_at, updated_at)
    SELECT DISTINCT ON (covered.thread_id, i.id)
      covered.thread_id, 'issue', i.id, b.team_id, i.board_id, b.deleted_at, b.archived_at,
      covered.user_id, 'user', covered.ord * 100 + covered.pos, covered.created_at, covered.created_at
    FROM (
      SELECT m.thread_id, m.team_id, m.user_id, m.ord, m.created_at, m.issue_id::text AS issue_ref, 0::bigint AS pos
      FROM m WHERE m.issue_id IS NOT NULL
      UNION ALL
      SELECT m.thread_id, m.team_id, m.user_id, m.ord, m.created_at, e.value, e.pos
      FROM m,
        LATERAL jsonb_array_elements_text(CASE WHEN jsonb_typeof(m.batch_issue_ids) = 'array' THEN m.batch_issue_ids ELSE '[]'::jsonb END)
          WITH ORDINALITY AS e(value, pos)
    ) covered
    JOIN issues i ON i.id::text = covered.issue_ref
    JOIN boards b ON b.id = i.board_id AND b.team_id = covered.team_id
    ORDER BY covered.thread_id, i.id, covered.ord, covered.pos
  `)
  const [{ unplaced }] = await db
    .select({ unplaced: sql<number>`count(*)::int` })
    .from(codingSessions)
    .where(
      and(
        eq(codingSessions.teamId, teamId),
        sql`NOT EXISTS (SELECT 1 FROM thread_items ti WHERE ti.session_id = ${codingSessions.id})`
      )
    )
  if (unplaced > 0) {
    throw new Error(`${unplaced} demo coding session(s) sit in no thread`)
  }
}

async function main() {
  assertLocalDatabase(SEED_CONSEQUENCE)
  await teardown()
  const demoId = await ensureDemoUser()
  await ensureNewcomerUser()
  await ensureStarterUser()
  const mates = await ensureTeammates()
  const mira = mates[`demo-mira`]
  const jonas = mates[`demo-jonas`]
  const sofia = mates[`demo-sofia`]

  const [ws] = await db
    .insert(teams)
    .values({ name: `Acme`, slug: TEAM_SLUG })
    .returning()

  // Staggered joins (EXP-668): one `values([...])` gives every row the same
  // default `created_at`, and the members list is ordered by it — so the
  // roster came back in a different order on every sync. Minutes apart, owner
  // first, is both deterministic and how a real team reads.
  await db.insert(teamMembers).values(
    [demoId, mira, jonas, sofia].map((userId, index) => ({
      teamId: ws.id,
      userId,
      role: index === 0 ? (`owner` as const) : (`member` as const),
      createdAt: new Date(now - (4 - index) * 60_000),
    }))
  )

  // An unconsumed invite for the `invite-accept` view. Deterministic token, but
  // never a stale one: teardown drops the team, and this row with it.
  await db.insert(teamInvites).values({
    teamId: ws.id,
    invitedById: demoId,
    role: `member`,
    token: DEMO_INVITE_TOKEN,
    email: NEWCOMER_EMAIL,
    sentAt: new Date(),
    expiresAt: DEMO_PENDING_INVITE_EXPIRY.demo,
  })

  // Two MORE unconsumed invites for the `settings-members` view — its pending
  // list renders nothing at all when empty. One was mailed to an address, one
  // is a bare shareable link (the two shapes the row styles differently).
  // Deliberately NOT DEMO_INVITE_TOKEN: that one stays the invite the
  // `invite-accept` view is captured on, and accepting any of these would
  // consume it.
  await db.insert(teamInvites).values([
    {
      teamId: ws.id,
      invitedById: demoId,
      role: `member`,
      token: `screenshots-demo-invite-priya`,
      email: `priya@northwind.dev`,
      createdAt: daysAgo(2),
      sentAt: daysAgo(2),
      expiresAt: DEMO_PENDING_INVITE_EXPIRY.mailed,
    },
    {
      teamId: ws.id,
      invitedById: mira,
      role: `member`,
      token: `screenshots-demo-invite-link`,
      createdAt: hoursAgo(20),
      sentAt: hoursAgo(20),
      expiresAt: DEMO_PENDING_INVITE_EXPIRY.link,
    },
  ])

  const [repo] = await db
    .insert(repositories)
    .values({ teamId: ws.id, fullName: `acme/mobile-app` })
    .returning()
  // EXP-1204: the repo the run-changes PR lives in (see RUN_PR_URL) — the
  // team must own a row for `codingSessions.prFiles` to read the PR. No board
  // points at it; it only shows as a second row on Settings → Repositories.
  await db.insert(repositories).values({
    teamId: ws.id,
    fullName: RUN_PR_REPO,
    sortOrder: 10,
  })

  const [board] = await db
    .insert(boards)
    .values({
      teamId: ws.id,
      name: `Mobile App`,
      slug: `mobile-app`,
      prefix: `APP`,
      color: `#6366f1`,
      repositoryId: repo.id,
      sortOrder: 0,
    })
    .returning()

  // Two more boards so the board-switcher screenshot shows several
  // glyphs side by side. Issue-less on purpose — only the switcher sheet
  // captures them; sortOrder keeps Mobile App the default board (the loader
  // picks the first board by sortOrder). EXP-1284: Product Feedback holds the
  // demo threads' NEW issues (see the threads block below) — it is the one
  // live board no view photographs, so they cost no board shot.
  const [, feedbackBoard] = await db.insert(boards).values([
    {
      teamId: ws.id,
      name: `Launch Marketing`,
      slug: EMPTY_BOARD_SLUG,
      prefix: `MKT`,
      color: `#f59e0b`,
      icon: `square-kanban`,
      sortOrder: 10,
    },
    {
      teamId: ws.id,
      name: `Product Feedback`,
      slug: `product-feedback`,
      prefix: `FB`,
      color: `#22c55e`,
      icon: `megaphone`,
      sortOrder: 20,
    },
  ]).returning()

  // One archived + one trashed board for the `settings-boards` view: both of
  // its cards ("Archived boards", "Pending deletion") hide when empty, so
  // without these the page renders neither. Both keep their (team_id, slug)
  // reservation and vanish from every normal board list server-side
  // (boardVisible() + the shapes' static IS NULL suffixes) — issue-less on
  // purpose, so the fan-out triggers (propagate_board_archived_at /
  // propagate_board_deleted_at) have no child rows to mirror onto. deletedAt
  // is hours old, well inside the 48h window the purge sweep waits out.
  await db.insert(boards).values([
    {
      teamId: ws.id,
      name: `Design System`,
      slug: `design-system`,
      prefix: `DS`,
      color: `#a855f7`,
      icon: `palette`,
      sortOrder: 30,
      // Pinned, not `daysAgo(9)`: the card prints this date absolutely.
      archivedAt: DEMO_PINNED_PAST_DATES.boardArchived,
    },
    {
      teamId: ws.id,
      name: `Growth Experiments`,
      slug: `growth-experiments`,
      prefix: `GRW`,
      color: `#14b8a6`,
      icon: `flask-conical`,
      sortOrder: 40,
      deletedAt: hoursAgo(6),
    },
  ])

  // A custom STARTED status (EXP-314) so the screenshots show team-defined
  // statuses: three started rows exercise the 1/4..3/4 pie-clock ladder
  // (builtin In Progress + In Review sort at 1 and 2 — In QA slots after).
  const [inQa] = await db
    .insert(issueStatuses)
    .values({
      teamId: ws.id,
      category: `started`,
      name: `In QA`,
      color: `#f472b6`,
      sortOrder: 3,
    })
    .returning()

  const labelRows = await db
    .insert(labels)
    .values([
      { teamId: ws.id, name: `Bug`, color: `#ef4444`, sortOrder: 0 },
      { teamId: ws.id, name: `Feature`, color: `#8b5cf6`, sortOrder: 10 },
      { teamId: ws.id, name: `Design`, color: `#3b82f6`, sortOrder: 20 },
      {
        teamId: ws.id,
        name: `Performance`,
        color: `#f59e0b`,
        sortOrder: 30,
      },
    ])
    .returning()
  const label = Object.fromEntries(labelRows.map((l) => [l.name, l.id]))

  // Inserted one-by-one so the issue-number trigger assigns APP-1..APP-n in
  // this exact order. Order = the story the screenshots tell.
  const seedIssues: Array<{
    title: string
    description?: string
    status: `backlog` | `in_progress` | `in_review` | `done`
    // Optional custom-status row (EXP-314) — dual-written with the anchor
    // `status` enum, exactly like the clients do.
    statusId?: string
    priority: `none` | `urgent` | `high` | `medium` | `low`
    assigneeId?: string
    creatorId: string
    dueDate?: string
    labels?: string[]
    createdDaysAgo: number
    completedDaysAgo?: number
    pr?: `open` | `merged`
    /** Link this issue to REVIEW_PR instead of a fictional acme PR. */
    realPr?: boolean
    /** SLOP-3: this PR is based on APP-<n>'s open PR branch (a stacked PR),
     *  so the Related work badge shows a stack on both. */
    stackedOn?: number
  }> = [
    {
      title: `Ship onboarding flow v2`,
      status: `done`,
      priority: `high`,
      assigneeId: sofia,
      creatorId: demoId,
      labels: [`Feature`],
      createdDaysAgo: 12,
      completedDaysAgo: 2,
    },
    {
      title: `Fix crash when uploading HEIC photos`,
      description: `Repro: attach a photo taken in portrait mode on iOS 18 — the upload worker throws on the color-profile conversion.\n\nStack trace points at the resize step, not the network layer.`,
      status: `done`,
      priority: `urgent`,
      assigneeId: jonas,
      creatorId: mira,
      labels: [`Bug`],
      createdDaysAgo: 6,
      completedDaysAgo: 1,
      pr: `merged`,
    },
    {
      title: `Dark mode contrast pass across settings`,
      status: `in_progress`,
      priority: `high`,
      assigneeId: demoId,
      creatorId: sofia,
      dueDate: DEMO_DUE_DATES.darkMode,
      labels: [`Design`],
      createdDaysAgo: 5,
    },
    {
      title: `Real-time sync indicator in the board header`,
      status: `in_progress`,
      statusId: inQa.id,
      priority: `medium`,
      assigneeId: mira,
      creatorId: demoId,
      labels: [`Feature`],
      createdDaysAgo: 4,
    },
    {
      title: `Reduce cold start below 800 ms`,
      description: `Startup profiling shows most of the time goes into the sync bootstrap, not rendering.\n\n- [x] Profile app launch end-to-end\n- [x] Defer shape subscribe until after first frame\n- [ ] Lazy-load the markdown editor\n- [ ] Cache the last board snapshot for instant paint\n\nTarget is \`<800ms\` cold on a mid-range device.\n\n| Device | Cold start | Target | Status |\n| --- | --- | --- | --- |\n| Pixel 6a | 1.4 s | 800 ms | over |\n| Galaxy A54 | 1.6 s | 800 ms | over |\n| iPhone 13 | 910 ms | 800 ms | close |\n| iPhone 15 Pro | 620 ms | 800 ms | ok |`,
      status: `in_progress`,
      priority: `urgent`,
      assigneeId: jonas,
      creatorId: demoId,
      dueDate: DEMO_DUE_DATES.coldStart,
      labels: [`Performance`],
      createdDaysAgo: 3,
    },
    {
      title: `Push notification deep links open the wrong tab`,
      description: `Tapping a comment push lands on the board instead of the issue. Only happens when the app was fully killed.`,
      status: `backlog`,
      priority: `urgent`,
      assigneeId: demoId,
      creatorId: mira,
      dueDate: DEMO_DUE_DATES.deepLinks,
      labels: [`Bug`],
      createdDaysAgo: 2,
    },
    {
      title: `Add drag-and-drop reordering on the board`,
      status: `backlog`,
      priority: `high`,
      assigneeId: mira,
      creatorId: demoId,
      labels: [`Feature`],
      createdDaysAgo: 8,
    },
    {
      title: `Improve empty states with illustrations`,
      status: `backlog`,
      priority: `medium`,
      assigneeId: sofia,
      creatorId: demoId,
      dueDate: DEMO_DUE_DATES.emptyStates,
      labels: [`Design`],
      createdDaysAgo: 7,
    },
    {
      title: `Offline queue for issue edits`,
      status: `backlog`,
      priority: `high`,
      creatorId: jonas,
      labels: [`Feature`],
      createdDaysAgo: 9,
    },
    // The two backlog issues below carry the embedded attachments (EXP-566).
    // Deliberately backlog rows: an embedded image whose blob was never
    // uploaded renders broken, and no capture recipe opens these.
    {
      title: `Migrate image cache to on-disk LRU`,
      description: `The in-memory cache evicts on every backgrounding, so scrolling the board twice re-downloads every avatar.\n\n![Cache hit rate over a week](/api/attachments/${ATTACHMENT_IDS.cacheHitRate})`,
      status: `backlog`,
      priority: `low`,
      creatorId: jonas,
      labels: [`Performance`],
      createdDaysAgo: 15,
    },
    {
      title: `Localize the app in German and Spanish`,
      status: `backlog`,
      priority: `medium`,
      creatorId: demoId,
      createdDaysAgo: 14,
    },
    {
      title: `Quick-add issue from the home screen widget`,
      status: `backlog`,
      priority: `low`,
      creatorId: mira,
      labels: [`Feature`],
      createdDaysAgo: 11,
    },
    {
      title: `Audit accessibility labels for VoiceOver`,
      description: `Sweep every screen with VoiceOver on and give the icon-only controls real labels.\n\n![Contrast sweep, before and after](/api/attachments/${ATTACHMENT_IDS.contrastSweep})`,
      status: `backlog`,
      priority: `medium`,
      creatorId: sofia,
      labels: [`Design`],
      createdDaysAgo: 10,
    },
    // In review with an OPEN pull request — the issue detail renders a live PR
    // card, and this is the issue the Review screenshot opens (realPr: its diff
    // is fetched from GitHub for real, see REVIEW_PR_URL).
    {
      title: `Group board issues by assignee`,
      description: `Add an assignee grouping mode next to the status grouping. Remember the last choice per board.`,
      status: `in_review`,
      priority: `medium`,
      assigneeId: demoId,
      creatorId: jonas,
      labels: [`Feature`],
      createdDaysAgo: 2,
      pr: `open`,
      realPr: true,
    },
    // Three more open PRs (APP-15..17) so the Reviews tab — the way into the
    // Review screenshot — is a real queue, not a single row. Appended AFTER
    // the original 14: the identifiers APP-1..APP-14 are load-bearing for the
    // UI tests and the hand-written notification titles above.
    {
      title: `Batch-edit labels from the board`,
      status: `in_review`,
      priority: `medium`,
      assigneeId: mira,
      creatorId: demoId,
      labels: [`Feature`],
      createdDaysAgo: 1,
      pr: `open`,
      stackedOn: 14,
    },
    {
      title: `Fix flaky scroll restore on the issue list`,
      status: `in_review`,
      priority: `high`,
      assigneeId: jonas,
      creatorId: sofia,
      labels: [`Bug`],
      createdDaysAgo: 1,
      pr: `open`,
    },
    {
      title: `Thumbnail pipeline for faster image loading`,
      status: `in_review`,
      priority: `medium`,
      assigneeId: sofia,
      creatorId: mira,
      labels: [`Performance`],
      createdDaysAgo: 2,
      pr: `open`,
    },
  ]

  const inserted: Array<typeof issues.$inferSelect> = []
  for (const [i, spec] of seedIssues.entries()) {
    const [row] = await db
      .insert(issues)
      .values({
        boardId: board.id,
        teamId: board.teamId,
        title: spec.title,
        description: spec.description,
        status: spec.status,
        statusId: spec.statusId,
        priority: spec.priority,
        assigneeId: spec.assigneeId,
        creatorId: spec.creatorId,
        dueDate: spec.dueDate,
        sortOrder: i * 10,
        // Staggered by index (EXP-668). Several specs share a
        // `createdDaysAgo`, and `daysAgo` returns the SAME instant for the
        // same number — so APP-15/APP-16 (both 1) and APP-6/APP-14/APP-17
        // (all 2) tied exactly, and every list keyed on `createdAt` ordered
        // them differently from one sync to the next. A minute per index
        // keeps the day the reader sees while making the instant unique.
        createdAt: new Date(daysAgo(spec.createdDaysAgo).getTime() - i * 60_000),
        completedAt:
          spec.completedDaysAgo === undefined
            ? undefined
            : daysAgo(spec.completedDaysAgo),
        ...(spec.pr
          ? {
              prUrl: spec.realPr
                ? REVIEW_PR_URL
                : `https://github.com/acme/mobile-app/pull/${40 + i}`,
              prNumber: spec.realPr ? REVIEW_PR_NUMBER : 40 + i,
              prState: spec.pr,
              branch: `exp/APP-${i + 1}`,
              prBaseBranch: spec.stackedOn
                ? `exp/APP-${spec.stackedOn}`
                : undefined,
              prMergedAt: spec.pr === `merged` ? daysAgo(1) : undefined,
            }
          : {}),
      })
      .returning()
    inserted.push(row)
    if (spec.labels?.length) {
      await db.insert(issueLabels).values(
        spec.labels.map((name) => ({
          issueId: row.id,
          labelId: label[name],
          teamId: ws.id,
          boardId: row.boardId,
        }))
      )
    }
  }

  // A closed-as-duplicate issue so the duplicate banner ("Duplicate of APP-6")
  // has something to render. Inserted after the loop because it points at an
  // issue the loop created; enum + FK move in lockstep, which is the whole
  // contract (`populate_issue_status_id` derives status_id from the anchor).
  const [duplicate] = await db
    .insert(issues)
    .values({
      boardId: board.id,
      teamId: board.teamId,
      title: `App opens the board instead of the issue from a push`,
      description: `Same as the deep-link bug — tapping a comment notification lands on the board when the app was killed.`,
      status: `duplicate`,
      duplicateOfId: inserted[5].id,
      priority: `medium`,
      creatorId: sofia,
      sortOrder: seedIssues.length * 10,
      createdAt: hoursAgo(30),
    })
    .returning()
  inserted.push(duplicate)

  // The one `blocks` edge: it puts a blocks rail/badge on two board rows, on
  // purpose — every board view shows it.
  const byTitle = (title: string) => {
    const row = inserted.find((issue) => issue.title === title)
    if (!row) throw new Error(`seed has no issue titled "${title}"`)
    return row
  }
  const [blocker, blocked] = DEMO_BLOCKS.map(byTitle)
  await db.insert(issueRelations).values({
    issueId: blocker.id,
    relatedIssueId: blocked.id,
    type: `blocks`,
    teamId: ws.id,
    boardId: board.id,
    createdAt: daysAgo(6),
  })

  // Showcase issue APP-5: comments + activity + subscribers for the
  // issue-detail and comments screenshots. The offsets are pinned rather than
  // written inline because a comment's relative label is the only thing in
  // this thread that moves between runs (EXP-669) —
  // `DEMO_SHOWCASE_COMMENT_HOURS_AGO` explains which offsets survive the gap
  // between the seed and the shutter, and on which clients.
  const showcase = inserted[4]
  // EXP-741: Mira's profiling comment opens the thread and Jonas answers IN
  // it (parent_id), so every issue-comments shot shows a reply under its
  // card; the offsets are unchanged.
  const [miraComment] = await db
    .insert(comments)
    .values({
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      authorId: mira,
      body: `Profiled on a mid-range device — the shape subscribe alone is **410 ms**. Deferring it until after first frame gets us to ~750 ms cold.`,
      createdAt: hoursAgo(DEMO_SHOWCASE_COMMENT_HOURS_AGO.mira),
    })
    .returning()
  await db.insert(comments).values([
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      authorId: jonas,
      parentId: miraComment.id,
      body: `Nice find. I'll take the board snapshot cache — we can reuse the reducer state and paint before sync finishes.`,
      createdAt: hoursAgo(DEMO_SHOWCASE_COMMENT_HOURS_AGO.jonas),
    },
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      authorId: demoId,
      body: `Deferral PR is merged. CI numbers:\n\n| Run | Before | After |\n| --- | --- | --- |\n| Cold start | 1.4 s | **860 ms** |\n| Warm start | 320 ms | 320 ms |\n\nSnapshot cache should get us under target.`,
      createdAt: hoursAgo(DEMO_SHOWCASE_COMMENT_HOURS_AGO.demo),
    },
    // @mention + #issue ref so the comments screenshot shows both pill types
    // (interchange forms: plain `@<email>` and `#<IDENTIFIER>` GFM text).
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      authorId: sofia,
      body: `@${TEAMMATES[1].email} once #APP-2 is verified on device, can you re-run the profiling? The HEIC fix might shave a little more off the cold start.`,
      createdAt: hoursAgo(DEMO_SHOWCASE_COMMENT_HOURS_AGO.sofia),
    },
  ])
  await db.insert(issueSubscribers).values([
    {
      issueId: showcase.id,
      userId: demoId,
      teamId: ws.id,
      boardId: showcase.boardId,
      source: `creator`,
    },
    {
      issueId: showcase.id,
      userId: jonas,
      teamId: ws.id,
      boardId: showcase.boardId,
      source: `assignee`,
    },
    {
      issueId: showcase.id,
      userId: mira,
      teamId: ws.id,
      boardId: showcase.boardId,
      source: `commenter`,
    },
  ])
  await db.insert(issueEvents).values([
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      actorUserId: jonas,
      type: `status_changed`,
      payload: { from: `backlog`, to: `in_progress` },
      createdAt: hoursAgo(30),
    },
  ])

  // Attachments for the `settings-storage` view, which otherwise reads
  // "0 attachments · 0 B". Rows only, no blobs: the storage manager renders a
  // type icon, the byte total and a referenced/unreferenced badge — never a
  // thumbnail. "Referenced" means the id appears in some issue description or
  // comment body of the team, OR the row is comment-linked (EXP-554,
  // collectTeamReferencedAttachmentIds); exactly one image below is neither, so
  // "Sweep unreferenced images" has a non-zero count to offer.
  const cacheIssue = inserted[9]
  const auditIssue = inserted[12]

  // The comment the comment-linked attachment hangs off — EXP-554 attachments
  // ride comment_id instead of markdown, and the storage view counts them
  // referenced through that link alone.
  const [auditComment] = await db
    .insert(comments)
    .values({
      issueId: auditIssue.id,
      teamId: ws.id,
      boardId: auditIssue.boardId,
      authorId: sofia,
      body: `First pass with VoiceOver on: every icon-only control in the board header is unlabeled. Audit run attached.`,
      createdAt: daysAgo(4),
    })
    .returning()

  const seedAttachments: Array<{
    id: string
    issueId: string
    commentId?: string
    uploaderId: string
    filename: string
    contentType: string
    sizeBytes: number
    width?: number
    height?: number
    createdAt: Date
  }> = [
    {
      id: ATTACHMENT_IDS.cacheHitRate,
      issueId: cacheIssue.id,
      uploaderId: jonas,
      filename: `cache-hit-rate.png`,
      contentType: `image/png`,
      sizeBytes: 412_907,
      width: 1600,
      height: 900,
      createdAt: DEMO_ATTACHMENT_DATES.cacheHitRate,
    },
    {
      id: ATTACHMENT_IDS.contrastSweep,
      issueId: auditIssue.id,
      uploaderId: sofia,
      filename: `contrast-sweep-before-after.png`,
      contentType: `image/png`,
      sizeBytes: 268_441,
      width: 2560,
      height: 1440,
      createdAt: DEMO_ATTACHMENT_DATES.contrastSweep,
    },
    {
      id: ATTACHMENT_IDS.voiceoverLabels,
      issueId: auditIssue.id,
      commentId: auditComment.id,
      uploaderId: sofia,
      filename: `voiceover-missing-labels.png`,
      contentType: `image/png`,
      sizeBytes: 184_320,
      width: 1290,
      height: 2796,
      createdAt: DEMO_ATTACHMENT_DATES.voiceoverLabels,
    },
    // The deliberate orphan: an image no body embeds and no comment links.
    // Older than the sweep's 24h grace window, so pressing the button really
    // reclaims it instead of reporting everything as "too recent".
    {
      id: ATTACHMENT_IDS.cacheSketch,
      issueId: cacheIssue.id,
      uploaderId: mira,
      filename: `image-cache-sketch-v2.png`,
      contentType: `image/png`,
      sizeBytes: 96_244,
      width: 1170,
      height: 2532,
      createdAt: DEMO_ATTACHMENT_DATES.cacheSketch,
    },
    // Non-image rows are never sweep candidates — they live in the issue's
    // Files list, which is not a markdown reference.
    {
      id: ATTACHMENT_IDS.auditPdf,
      issueId: auditIssue.id,
      uploaderId: sofia,
      filename: `accessibility-audit-q3.pdf`,
      contentType: `application/pdf`,
      sizeBytes: 1_248_576,
      createdAt: DEMO_ATTACHMENT_DATES.auditPdf,
    },
    {
      id: ATTACHMENT_IDS.sysdiagnose,
      issueId: inserted[1].id,
      uploaderId: jonas,
      filename: `sysdiagnose-heic-upload.zip`,
      contentType: `application/zip`,
      sizeBytes: 3_874_112,
      createdAt: DEMO_ATTACHMENT_DATES.sysdiagnose,
    },
  ]
  await db.insert(attachments).values(
    seedAttachments.map((spec) => ({
      id: spec.id,
      teamId: ws.id,
      issueId: spec.issueId,
      boardId: board.id,
      commentId: spec.commentId,
      uploaderId: spec.uploaderId,
      filename: spec.filename,
      contentType: spec.contentType,
      sizeBytes: spec.sizeBytes,
      // Derived exactly like the upload path does, from the id minted above.
      storageKey: buildAttachmentStorageKey(
        spec.issueId,
        spec.id,
        spec.filename
      ),
      url: buildAttachmentUrl(spec.id),
      width: spec.width,
      height: spec.height,
      createdAt: spec.createdAt,
    }))
  )

  // EXP-878: two issue DRAFTS for the demo user — what the create dialog kept
  // when it was closed with something in it. One carries the properties a
  // half-filled form has (a custom status, a priority, a label, a due date),
  // the other is title-less so the list's "Untitled draft" row is
  // photographed too. Drafts are per-user and never synced to teammates.
  await db.insert(issueDrafts).values([
    {
      userId: demoId,
      teamId: ws.id,
      boardId: board.id,
      title: `Offline queue for issue edits`,
      description: `Edits made without a connection should queue and replay in order once sync resumes.\n\nOpen question: what happens when a queued edit lands on an issue somebody else already moved?`,
      statusId: inQa.id,
      priority: `high`,
      assigneeId: demoId,
      labelIds: [label.Feature],
      dueDate: `2026-04-17`,
      createdAt: hoursAgo(5),
      updatedAt: hoursAgo(3),
    },
    {
      userId: demoId,
      teamId: ws.id,
      boardId: board.id,
      title: ``,
      description: `Check whether the board list still re-renders every row on a single status change.`,
      priority: `none`,
      createdAt: daysAgo(2),
      updatedAt: daysAgo(2),
    },
  ])

  // Inbox for the demo user — mixed unread/read, matching the wording the
  // real notifier produces (lib/integrations/notifications.ts).
  await db.insert(notifications).values([
    {
      userId: demoId,
      issueId: inserted[5].id,
      type: `issue_assigned`,
      title: `Mira Chen assigned you APP-6`,
      body: inserted[5].title,
      createdAt: hoursAgo(2),
    },
    {
      userId: demoId,
      issueId: showcase.id,
      type: `issue_comment`,
      title: `Jonas Weber commented on APP-5`,
      body: `Nice find. I'll take the board snapshot cache — we can reuse the reducer state…`,
      createdAt: hoursAgo(6),
    },
    {
      userId: demoId,
      issueId: inserted[2].id,
      type: `issue_mention`,
      title: `Sofia Almeida mentioned you in APP-3`,
      body: inserted[2].title,
      createdAt: hoursAgo(20),
    },
    {
      userId: demoId,
      issueId: inserted[1].id,
      type: `pr_merged`,
      title: `Jonas Weber merged the pull request for APP-2`,
      body: inserted[1].title,
      readAt: hoursAgo(12),
      createdAt: hoursAgo(24),
    },
    {
      userId: demoId,
      issueId: inserted[0].id,
      type: `issue_status_changed`,
      title: `Sofia Almeida changed APP-1 to done`,
      body: inserted[0].title,
      readAt: hoursAgo(30),
      createdAt: daysAgo(2),
    },
  ])

  // Live coding sessions for the agents screenshot. The clients hide rows
  // whose updated_at heartbeat is older than the contract staleHours window,
  // so both get a fresh heartbeat; board_id/team_id denormalize by trigger.
  //
  // EXP-733: the demo user's own rows name the MACHINE (`deviceId`) and the
  // AGENT the way a real launch stamps them — `sessionAgentUsage` (×4) needs
  // both to join the session to the stub desktop's synced usage report, which
  // is what puts the usage strip on the steering shot and the Usage sheet on
  // mobile. Mira's row stays bare: her machine never registers.
  //
  // EXP-740/913: every session carries a PINNED id — a session has its own
  // route, and a reseed must be byte-stable. The three LIVE rows (APP-5,
  // APP-4, APP-14) are one re-assertable spec in lib/demo-live-sessions.ts,
  // because the staleness sweep reaps them mid-wave and screenshots:desktop
  // puts them back.
  // The threads are planted below (`seedThreads`), so no re-link here.
  const live = await assertDemoLiveSessions(now, { relinkThreads: false })
  if (live !== 3) throw new Error(`expected 3 live demo sessions, wrote ${live}`)
  await db.insert(codingSessions).values([
    // EXP-739: one FINISHED chat, so the `chat` view photographs its start
    // card over a "Past chats" list instead of an empty page. Issue-less and
    // action-less with the reserved `Chat` name snapshot IS what makes a row
    // a chat run (lib/session-identity.ts, ×4).
    {
      id: DEMO_SESSION_IDS.pastChat,
      teamId: ws.id,
      userId: demoId,
      actionId: null,
      actionName: `Chat`,
      deviceId: DEMO_DEVICE_ID,
      deviceLabel: DEMO_DEVICE_LABEL,
      agent: `claude`,
      status: `ended`,
      endedBy: `user`,
      createdAt: hoursAgo(27),
      // No `summary`: only an UNATTENDED run reports one through
      // `exponential_sessions_end` (EXP-673), and this is a person's chat.
      startedAt: hoursAgo(27),
      endedAt: hoursAgo(26),
    },
    // EXP-1204: Jonas's finished chat run that opened a pull request of its
    // own (`pr_open{repositoryId, head}`, EXP-626) — the `run-changes` view and
    // the Reviews "Agent runs" band (EXP-734/1194). A TEAMMATE's run on
    // purpose: the demo user reviews it read-only (web `TeammateRunChanges`,
    // no steer ticket), which is deterministic without a relay. The PR stays
    // `open` on the row (the real PR's state is never read — only its files);
    // `ended` keeps it out of the staleness sweep, which reaps live rows only.
    // `agent_title` is the chat's subject (the natives' row title and the
    // RunChanges header); the branch is a chat run's `exp/chat-<id8>`.
    {
      id: DEMO_SESSION_IDS.runChanges,
      teamId: ws.id,
      userId: jonas,
      actionId: null,
      actionName: `Chat`,
      agentTitle: `Fix the error-type comparison in the resolver`,
      deviceId: DEMO_SERVER_DEVICE_ID,
      deviceLabel: `Acme build server`,
      agent: `codex`,
      status: `ended`,
      endedBy: `user`,
      branch: `exp/chat-${DEMO_SESSION_IDS.runChanges.slice(0, 8)}`,
      prUrl: RUN_PR_URL,
      prNumber: RUN_PR_NUMBER,
      prState: `open`,
      prBaseBranch: `main`,
      createdAt: hoursAgo(6),
      startedAt: hoursAgo(6),
      endedAt: hoursAgo(5),
    },
  ])

  // Team actions (EXP-253) so the Actions screenshot lists real saved actions
  // above the two client-side builtins. Bodies are short but plausible — the
  // list shows name + description; the body is only fetched on run/edit.
  const actionRows = await db
    .insert(actions)
    .values([
      {
        teamId: ws.id,
        repositoryId: repo.id,
        name: `Update dependencies`,
        description: `Bump every package to the latest compatible release and open a PR.`,
        icon: `package`,
        body: `Update all dependencies to their latest compatible versions. Run the full test suite, fix any breakage the bumps cause, and open a PR summarizing notable upgrades.`,
        sortOrder: 0,
      },
      {
        teamId: ws.id,
        repositoryId: repo.id,
        id: DEMO_ACTION_ID,
        name: `Nightly test triage`,
        description: `Investigate failing or flaky tests and file issues for real bugs.`,
        icon: `flask-conical`,
        body: `Run the test suite three times. For every failure, decide flaky vs real: quarantine and file an issue for flaky tests, fix trivial breakage directly, and file detailed issues for anything deeper.`,
        sortOrder: 10,
      },
      {
        teamId: ws.id,
        name: `Draft release notes`,
        description: `Summarize merged PRs since the last release into user-facing notes.`,
        icon: `sparkles`,
        body: `Collect the PRs merged since the last release tag and draft concise, user-facing release notes grouped by feature, fix, and performance. Post the draft as a comment for review.`,
        sortOrder: 20,
      },
    ])
    .returning()
  const action = Object.fromEntries(actionRows.map((a) => [a.name, a]))

  // Triggers (SLOP-2) for the `action-*` views — without them the action
  // page photographs "No triggers." An action carries them: a when-part
  // (schedule or issue event, typed in @exp/db-schema domain.ts) plus the
  // runner binding, which is the MACHINE's steer deviceId, not a row uuid.
  // minuteOfDay is device-local wall clock by design (no tz field). One
  // paused trigger so the toggle has an off state to render. Written the way
  // the router writes them: the array on the action; the run history's
  // `trigger_id` names a trigger id.
  const seededTriggers: { action: string; triggers: ActionTrigger[] }[] = [
    {
      action: `Nightly test triage`,
      triggers: [
        {
          id: DEMO_TRIGGER_IDS.nightlyTriage,
          enabled: true,
          deviceId: DEMO_DEVICE_ID,
          agent: `claude`,
          kind: `schedule`,
          interval: `daily`,
          minuteOfDay: 180,
        },
      ],
    },
    {
      action: `Update dependencies`,
      triggers: [
        {
          id: DEMO_TRIGGER_IDS.updateDeps,
          enabled: true,
          deviceId: DEMO_DEVICE_ID,
          agent: `codex`,
          kind: `schedule`,
          interval: `weekly`,
          weekday: 1,
          minuteOfDay: 540,
        },
      ],
    },
    {
      action: `Draft release notes`,
      triggers: [
        {
          id: DEMO_TRIGGER_IDS.releaseNotes,
          enabled: false,
          deviceId: DEMO_DEVICE_ID,
          kind: `event`,
          source: `exponential`,
          event: `pr_merged`,
        },
      ],
    },
  ]
  await db.transaction(async (tx) => {
    for (const { action: name, triggers } of seededTriggers) {
      const row = action[name]
      await tx.update(actions).set({ triggers }).where(eq(actions.id, row.id))
    }
  })

  // Two finished triggered runs so an action's Runs says something other
  // than "No runs yet." Action-scoped sessions carry the action id plus a
  // display-name snapshot, `started_reason` (which trigger kind fired) and
  // `trigger_id` (the trigger's id). Ended, so they never join the live
  // agents list.
  //
  // EXP-663: both rows are a full EXP-637 close-out (`ended_by: 'agent'`).
  // EXP-864: the agent's summary is reported to the parent run, never stored,
  // so the rows carry none.
  await db.insert(codingSessions).values([
    {
      id: DEMO_SESSION_IDS.nightlyTriageRun,
      teamId: ws.id,
      userId: demoId,
      actionId: action[`Nightly test triage`].id,
      actionName: `Nightly test triage`,
      triggerId: DEMO_TRIGGER_IDS.nightlyTriage,
      startedReason: `schedule`,
      deviceId: DEMO_DEVICE_ID,
      deviceLabel: DEMO_DEVICE_LABEL,
      status: `ended`,
      endedBy: `agent`,
      // EXP-913: web's "Last run" reads createdAt, the natives startedAt.
      createdAt: hoursAgo(9),
      startedAt: hoursAgo(9),
      // The re-clock anchor (lib/demo-reclock.ts): its offset IS the constant.
      endedAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo),
    },
    {
      id: DEMO_SESSION_IDS.updateDepsRun,
      teamId: ws.id,
      userId: demoId,
      actionId: action[`Update dependencies`].id,
      actionName: `Update dependencies`,
      triggerId: DEMO_TRIGGER_IDS.updateDeps,
      startedReason: `schedule`,
      deviceId: DEMO_DEVICE_ID,
      deviceLabel: DEMO_DEVICE_LABEL,
      status: `ended`,
      endedBy: `agent`,
      createdAt: hoursAgo(33),
      startedAt: hoursAgo(33),
      endedAt: hoursAgo(32),
    },
  ])

  // A teammate's headless `exponential` server shared with the team (EXP-432),
  // so the Agents page renders its "Team machines" section and a Shared badge
  // at all — every other seeded machine is the demo user's own. OFFLINE on
  // purpose: `last_seen_at` freshness is what "online" means (contract
  // `device.onlineWindowSeconds`, 90s), and a fake heartbeat here would be
  // contradicted by the relay the moment a capture looks. The demo user's own
  // desktop row is seeded below (EXP-1008) and kept fresh by
  // screenshots:desktop, whose heartbeat upsert owns its liveness, version,
  // default-machine flag and agent report.
  await db.insert(devices).values({
    userId: jonas,
    deviceId: DEMO_SERVER_DEVICE_ID,
    label: `Acme build server`,
    kind: `server`,
    platform: `linux`,
    version: DEMO_SERVER_VERSION,
    agents: [`claude`, `codex`],
    caps: [`actions`, `action-inputs`, `fix-conflicts`, `chat`, `triggers`, `automations`],
    sharedTeamIds: [ws.id],
    lastSeenAt: hoursAgo(5),
    createdAt: daysAgo(45),
  })

  // EXP-1008: the demo user's own desktop, seeded rather than left to the
  // relay stub, so it exists (with its label) before the first beat. The
  // stub's heartbeat upserts on the same (user_id, device_id) and only
  // refreshes the liveness + report columns, so nothing here is overwritten
  // that the captures rely on. Offline until the stub beats (the 90s window).
  await db.insert(devices).values({
    userId: demoId,
    deviceId: DEMO_DEVICE_ID,
    label: DEMO_DEVICE_LABEL,
    kind: `desktop`,
    platform: `macos`,
    version: DEMO_DEVICE_VERSION,
    isDefault: true,
    agents: [`claude`, `codex`],
    lastSeenAt: hoursAgo(1),
    createdAt: daysAgo(58),
  })
    // screenshots:desktop upserts the same (user_id, device_id) every ~30s, so
    // a stub heartbeat landing between `ensureDemoUser` and this insert must
    // not abort the seed on the unique index; converge the row instead
    // (mirrors the stub's own `onConflictDoUpdate`).
    .onConflictDoUpdate({
      target: [devices.userId, devices.deviceId],
      set: {
        label: DEMO_DEVICE_LABEL,
        kind: `desktop`,
        platform: `macos`,
        version: DEMO_DEVICE_VERSION,
        isDefault: true,
        agents: [`claude`, `codex`],
        lastSeenAt: hoursAgo(1),
        createdAt: daysAgo(58),
      },
    })

  await seedThreads({
    teamId: ws.id,
    demoId,
    jonas,
    sofia,
    feedbackBoardId: feedbackBoard!.id,
    labelIds: label,
    showcase,
    linkedByShowcase: byTitle(`Migrate image cache to on-disk LRU`),
    linkedByServerChild: byTitle(`Offline queue for issue edits`),
  })

  // MCP servers (EXP-792) for the `settings-mcp-servers` view — one row per
  // state the page draws: Linear (OAuth) CONNECTED for the demo user (a
  // server-held credential, stored the way the OAuth callback stores one, so
  // the row reads "Connected" and "1 of N connected"), Sentry (OAuth) not
  // connected (its row offers Connect, and the launch picker greys it
  // "Connect first"), and a sign-in-free docs server ("No sign-in needed").
  // Sentry stays off by default so no composer shot preselects it.
  const [linearServer] = await db
    .insert(mcpServers)
    .values([
      {
        teamId: ws.id,
        name: `linear`,
        transport: `http`,
        url: `https://mcp.linear.app/mcp`,
        auth: `oauth`,
        enabledByDefault: true,
        createdById: demoId,
        createdAt: daysAgo(24),
        updatedAt: daysAgo(24),
      },
      {
        teamId: ws.id,
        name: `sentry`,
        transport: `http`,
        url: `https://mcp.sentry.dev/mcp`,
        auth: `oauth`,
        enabledByDefault: false,
        createdById: demoId,
        createdAt: daysAgo(18),
        updatedAt: daysAgo(18),
      },
      {
        teamId: ws.id,
        name: `acme-docs`,
        transport: `http`,
        url: `https://docs.acme.dev/mcp`,
        auth: `none`,
        enabledByDefault: true,
        createdById: demoId,
        createdAt: daysAgo(11),
        updatedAt: daysAgo(11),
      },
    ])
    .returning({ id: mcpServers.id })
  // A placeholder token set: never valid anywhere, it only makes the row
  // read as connected (a refresh token + no expiry = "connected").
  await storeOauthTokens(db, {
    serverId: linearServer!.id,
    userId: demoId,
    teamId: ws.id,
    issuer: `https://mcp.linear.app`,
    clientId: `exp-screenshot-seed`,
    tokenEndpoint: `https://mcp.linear.app/token`,
    tokens: {
      accessToken: `seed-access-token`,
      refreshToken: `seed-refresh-token`,
      tokenType: `Bearer`,
      expiresAt: null,
    },
  })

  // Embeddable widget configs for the `settings-widget` view ("No widgets
  // yet." without them). One full config — a domain allowlist, two of the
  // seeded labels for the reporter to tag with, a theme — and a second,
  // email-required one on the same board (SLOP-4: every widget files issues
  // onto a board). Keys are minted the way the router mints them.
  const [siteWidget] = await db
    .insert(widgetConfigs)
    .values([
      {
        teamId: ws.id,
        boardId: board.id,
        name: `Acme website`,
        publicKey: generateWidgetKey(),
        allowedDomains: [`acme.dev`, `*.acme.dev`],
        createdByUserId: demoId,
        createdAt: daysAgo(24),
        formConfig: {
          buttonLabel: `Feedback`,
          accentColor: `#6366f1`,
          // EXP-672 dropped the legacy two-value `position` read shim, so the
          // row carries the explicit per-device `launcher` every stored config
          // now has. These values are what the old `position: bottom-right`
          // resolved to on both devices, so the seeded shots do not move.
          launcher: {
            desktop: { mode: `fab`, position: `bottom-right` },
            mobile: { mode: `fab`, position: `bottom-right` },
          },
          collectEmail: true,
          collectName: true,
          labelIds: [label[`Bug`], label[`Feature`]],
          theme: `auto`,
        },
      },
      {
        teamId: ws.id,
        boardId: board.id,
        name: `Help center`,
        publicKey: generateWidgetKey(),
        allowedDomains: [`help.acme.dev`],
        createdByUserId: demoId,
        createdAt: daysAgo(8),
        formConfig: {
          buttonLabel: `Contact us`,
          accentColor: `#22c55e`,
          collectEmail: true,
          emailRequired: true,
          theme: `dark`,
        },
      },
    ])
    .returning({ id: widgetConfigs.id })

  // SLOP-4: ONE widget-filed issue with a reporter conversation — the
  // `support-reporter` view (the anonymous magic-link page, addressed by a
  // token minted over the PINNED issue id) and the reporter card + "Reply to
  // reporter" toggle on every client's issue detail. The report is the
  // issue itself (source `widget`, no creator, the reporter's words escaped
  // the way the submit path escapes them); the conversation is its
  // reporter-audience comments: Sofia's reply (emailed to the reporter) and
  // the reporter's answer (author NULL, source `reporter`). The timestamps
  // print as "X ago" on the reporter page, so `demo-reclock.ts` shifts them.
  const [widgetIssue] = await db
    .insert(issues)
    .values({
      id: DEMO_WIDGET_ISSUE_ID,
      boardId: board.id,
      teamId: board.teamId,
      title: WIDGET_REPORT_TITLE,
      description: escapeReporterText(
        `After the last update the sign-in button just spins forever on my iPad. Works fine on my phone.`
      ),
      status: `backlog`,
      priority: `none`,
      creatorId: null,
      source: `widget`,
      sortOrder: (seedIssues.length + 2) * 10,
      createdAt: hoursAgo(26),
    })
    .returning()
  await db.insert(widgetSubmissions).values({
    widgetConfigId: siteWidget.id,
    issueId: widgetIssue.id,
    reporterEmail: WIDGET_REPORTER_EMAIL,
    reporterName: WIDGET_REPORTER_NAME,
    pageUrl: `https://acme.dev/app/sign-in`,
    userAgent: `Mozilla/5.0 (iPad; CPU OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1`,
    viewportWidth: 1024,
    viewportHeight: 1366,
    screenWidth: 1024,
    screenHeight: 1366,
    devicePixelRatio: 2,
    // The instant their last reply landed — the one moment we know their
    // page was open (the reporter card's read receipt).
    lastReporterSeenAt: hoursAgo(1),
    createdAt: hoursAgo(26),
  })
  await db.insert(issueSubscribers).values({
    issueId: widgetIssue.id,
    userId: null,
    email: WIDGET_REPORTER_EMAIL,
    teamId: ws.id,
    boardId: board.id,
    source: `widget_reporter`,
  })
  await db.insert(comments).values([
    {
      issueId: widgetIssue.id,
      teamId: ws.id,
      boardId: board.id,
      authorId: sofia,
      audience: `reporter`,
      body: `Thanks for the report! We found a token refresh bug on iPadOS and just shipped a fix — could you update to 2.4.1 and try again?`,
      createdAt: hoursAgo(20),
    },
    {
      issueId: widgetIssue.id,
      teamId: ws.id,
      boardId: board.id,
      authorId: null,
      source: `reporter`,
      audience: `reporter`,
      body: escapeReporterText(
        `That fixed it — thank you for the quick turnaround!`
      ),
      createdAt: hoursAgo(1),
    },
  ])

  // Personal API keys for the `settings-api-keys` view ("No API keys yet."
  // without them). Minted through Better Auth exactly like
  // `users.mintPersonalApiKey` does, so the rows are genuine `expu_`
  // credentials (only their hash is stored) rather than hand-written rows —
  // the page lists name, prefix and last use.
  //
  // Then the two columns the page PRINTS are overwritten with their pinned
  // values: a fresh mint randomises `start` and stamps `created_at` now, which
  // rewrote this view on every refresh (see DEMO_API_KEYS).
  for (const key of DEMO_API_KEYS) {
    const created = await auth.api.createApiKey({
      body: {
        name: key.name,
        userId: demoId,
        expiresIn: null,
        rateLimitEnabled: false,
        metadata: { kind: `personal` },
      },
    })
    await db
      .update(apikeys)
      .set({ start: key.start, createdAt: key.createdAt })
      .where(eq(apikeys.id, created.id))
  }

  console.log(`
Seeded screenshot demo data:
  team        ${ws.name} (/${ws.slug})
  board       ${board.name} (APP), ${inserted.length} issues (1 on custom status "In QA", 1 duplicate)
  boards      3 live + 1 archived + 1 trashed (48h pending deletion)
  login       ${DEMO_EMAIL} / ${DEMO_PASSWORD}
  newcomer    ${NEWCOMER_EMAIL} / ${NEWCOMER_PASSWORD} (no team — onboarding + invite views)
  starter     ${STARTER_EMAIL} / ${STARTER_PASSWORD} (owns ${STARTER_TEAM_NAME}, no board — the wizard's invite + devices steps)
  invite      /invite/${DEMO_INVITE_TOKEN} (+ 2 more pending invites)
  showcase    ${showcase.identifier ?? `APP-5`} (markdown + ${4} comments incl. @mention + #issue ref)
  inbox       5 notifications (3 unread)
  agents      3 coding sessions (2 running + 1 in review)
  threads     ${DEMO_THREAD_ID} (showcase run + a linked and a filed issue), ${DEMO_THREAD_RUNS_ID} ("${DEMO_THREAD_RUNS_TITLE}": Jonas's 4 runs, 2 live) + one per other run
  machines    demo desktop (offline until screenshots:desktop beats) + 1 shared team server (offline)
  reviews     4 open pull requests
  review shot APP-14 → ${REVIEW_PR_URL} (real diff, fetched from GitHub)
  run changes Jonas's chat run → ${RUN_PR_URL} (real diff; ${RUN_PR_REPO} registered as a team repo)
  actions     ${actionRows.length} saved team actions
  triggers    ${seededTriggers.length} (2 scheduled + 1 event, 1 paused) + 2 triggered runs
  storage     ${seedAttachments.length} attachments (1 unreferenced image to sweep)
  mcp         3 team MCP servers: linear (OAuth, connected for the demo user), sentry (OAuth, not connected), acme-docs (no sign-in)
  widgets     2 widget configs
  reporter    1 widget-filed issue ("${WIDGET_REPORT_TITLE}") with a reporter conversation
  api keys    2 personal keys

Next: bun run screenshots:desktop (needs STEER_RELAY_URL + STEER_RELAY_SECRET)
`)
  process.exit(0)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
