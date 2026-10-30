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
 * always look fresh. The USER ids and the session-tree ids are pinned
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
 * not obviously local (see assertLocalDatabase) unless `--allow-remote`, and
 * the SHARED dev DB (localhost:54321/exponential, assertNotSharedDevDatabase)
 * unless `--shared-dev-db` (the default `bun run shots` path passes it).
 *
 * Usage (from apps/web, local dev env with password signup enabled):
 *   bun run seed:screenshots                  # temp DB / throwaway stack
 *   bun run seed:screenshots --shared-dev-db  # the shared dev DB, on purpose
 *   bun run screenshots:desktop   # second shell, stays up during the capture
 */
import { ASSISTANT_COPY, assistantScheduleTriggers } from "@exp/domain-contract/agent-settings"
import { and, eq, inArray, isNull, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { storeOauthTokens } from "@/lib/mcp-oauth/credentials"
import {
  accounts,
  apikeys,
  attachments,
  agents,
  apps,
  appRecords,
  appVersions,
  comments,
  devices,
  events,
  issueDrafts,
  issueEvents,
  issueLabels,
  issueRelations,
  issues,
  issueStatuses,
  issueSubscribers,
  labels,
  mcpServers,
  messages,
  conversations,
  notifications,
  boards,
  pullRequestIssues,
  prompts,
  pullRequests,
  repositories,
  sessionIssues,
  sessionReads,
  sessions,
  teamInvites,
  users,
  userVoiceSettings,
  teamMembers,
  teams,
  widgetConfigs,
  widgetSubmissions,
} from "@/db/schema"
import type { AgentTrigger, GuideEntry, MessageCard } from "@exp/db-schema/domain"
import workingHoursTemplate from "@exp/ui/exponential-ui/templates/working-hours.json"
import { auth } from "@/lib/auth"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
} from "@/lib/storage/issue-attachments"
import { deleteObject } from "@/lib/storage"
import { generateWidgetKey } from "@/lib/widget/key"
import { seedDemoGuidePictures } from "./lib/demo-guide-pictures"
import {
  assertDemoLiveSessions,
  DEMO_LIVE_SESSIONS,
  demoNeedsInput,
} from "./lib/demo-live-sessions"
import { DEMO_CLOCK_ANCHOR } from "./lib/demo-reclock"
import { parseFreezeNow } from "./lib/freeze-now"
import { assertLocalDatabase, assertNotSharedDevDatabase } from "./lib/local-db-guard"
import {
  DEMO_API_KEYS,
  DEMO_ATTACHMENT_DATES,
  DEMO_ACTION_ID,
  DEMO_AGENT_CHANNEL_TIMES,
  DEMO_AGENT_COMMENT_ID,
  DEMO_AGENT_COMMENT_MINUTES_AGO,
  DEMO_AGENT_CONVERSATION_ID,
  DEMO_AGENT_CONVERSATION_IDS,
  DEMO_ASSISTANT_CONVERSATION_ID,
  DEMO_ASSISTANT_QUESTION,
  DEMO_AGENT_EVENT_TRIGGER_ID,
  DEMO_AGENT_ID,
  DEMO_AGENT_IDS,
  DEMO_ASSISTANT_ID,
  DEMO_AGENT_REQUEST,
  DEMO_APP_CHAT_TIMES,
  DEMO_APP_EDITOR_SESSION_ID,
  DEMO_APP_EMPTY_ID,
  DEMO_APP_ID,
  DEMO_APP_DASHBOARD_ID,
  DEMO_APP_DASHBOARD_SLUG,
  DEMO_APP_DASHBOARD_VERSION_IDS,
  DEMO_APP_RECORD_IDS,
  DEMO_APP_SLUG,
  DEMO_APP_VERSION_IDS,
  DEMO_AGENT_TRIGGER_ID,
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
  demoAgentReport,
  DEMO_SHOWCASE_COMMENT_HOURS_AGO,
  DEMO_FEED_QUESTION,
  DEMO_PULL_REQUEST_IDS,
  DEMO_REPORT_SESSION_ID,
  DEMO_GROUPED_SESSION_ID,
  DEMO_SESSION_ID,
  DEMO_THREAD_ID,
  DEMO_THREAD_IDS,
  DEMO_TIMEZONE,
  DEMO_USER_ID,
  DEMO_BLOCKS,
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

// The Review screenshot shows a REAL diff: the PR files are fetched from
// GitHub, so the fictional acme/mobile-app PR URLs render an empty
// file list. Safe to point one issue at a public PR because the Changes screen
// displays neither the repo name nor the PR number — only the branch
// (`exp/APP-14`), the state capsule, the file counts and the patches
// (ChangesView.swift / ChangesScreen.kt) — so the demo fiction survives intact.
//
// EXP-1293: it is the `pull_requests` row of the APP-14 thread (stacked on
// APP-15's), filed under the team's acme/mobile-app repo like its siblings;
// the files fetch goes by its URL.
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
    // The team cascade drops the rows; the Guide pictures' blobs (the only
    // seeded rows with stored bytes, owner-less) go first.
    const pictures = await db
      .select({ storageKey: attachments.storageKey })
      .from(attachments)
      .where(
        and(
          eq(attachments.teamId, ws.id),
          isNull(attachments.issueId),
          isNull(attachments.draftId)
        )
      )
    for (const { storageKey } of pictures) {
      await deleteObject(storageKey).catch((error: unknown) =>
        console.warn(`couldn't delete seeded picture ${storageKey}`, error)
      )
    }
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

/** A Guide text entry (EXP-933/1154), stamped like a `sessions_guide` write. */
function guideText(
  topic: string,
  text: string,
  at: Date,
  files?: string[],
  /** The session that wrote the entry — stamped server-side on every real
   *  write; a session block shows only its own. */
  sessionId?: string
): GuideEntry {
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

type IssueRow = typeof issues.$inferSelect

/**
 * The Product Feedback board's issues, FB-1..FB-9 in insert order (the
 * identifier trigger numbers them). The session tree names three of them:
 * FB-3 (the cold-start thread FILED it), FB-8 (a widget report the Triage
 * agent closed as a duplicate of FB-3) and FB-9 (the widget report it started
 * a thread for). Product Feedback is the live board no view photographs, so
 * its rows cost no board shot. None of their titles says "cold start" (the
 * `search` view types it).
 */
async function seedFeedbackIssues(ctx: {
  teamId: string
  boardId: string
  demoId: string
  mira: string
  jonas: string
  sofia: string
  labelIds: Record<string, string>
}): Promise<Record<string, IssueRow>> {
  const { demoId, mira, jonas, sofia } = ctx
  const specs: Array<{
    title: string
    description: string
    creatorId: string | null
    label?: string
    createdAt: Date
    duplicateOf?: string
  }> = [
    {
      title: `Edits made offline disappear after reconnecting`,
      description: `Changed an issue's status on the train, the app showed it saved, and once I was back online the old status came back.`,
      creatorId: sofia,
      label: `Bug`,
      createdAt: daysAgo(4),
    },
    {
      title: `Conflict rules for replaying queued offline edits`,
      description: `When a queued edit replays onto an issue somebody else changed meanwhile, keep their value per field and tell the user which of their edits lost.`,
      creatorId: jonas,
      label: `Feature`,
      createdAt: daysAgo(3),
    },
    {
      title: `Syntax highlighting ships every language in the editor bundle`,
      description: `The markdown editor imports the full highlighter, so its chunk carries grammars for every language. Load only the common ones and fetch the rest on demand.`,
      creatorId: demoId,
      label: `Performance`,
      createdAt: hoursAgo(1),
    },
    {
      title: `Pin a board to the top of the sidebar`,
      description: `With six boards the one I use daily sits at the bottom. Let me pin it.`,
      creatorId: mira,
      label: `Feature`,
      createdAt: daysAgo(6),
    },
    {
      title: `Comment drafts get lost when switching issues`,
      description: `Typed half a comment, opened another issue to check something, came back and the draft was gone.`,
      creatorId: sofia,
      label: `Bug`,
      createdAt: daysAgo(5),
    },
    {
      title: `Keyboard shortcut to copy an issue link`,
      description: `I paste issue links into chat all day. A shortcut would save the trip to the menu.`,
      creatorId: jonas,
      label: `Feature`,
      createdAt: daysAgo(3),
    },
    {
      title: `Export a board as CSV`,
      description: `For the quarterly review we copy the board into a spreadsheet by hand.`,
      creatorId: mira,
      label: `Feature`,
      createdAt: daysAgo(2),
    },
    {
      title: `Editor takes seconds to open on older phones`,
      description: `Opening the description editor on my Galaxy A12 shows a blank box for a couple of seconds.`,
      creatorId: null,
      createdAt: hoursAgo(14),
      duplicateOf: `Syntax highlighting ships every language in the editor bundle`,
    },
    {
      title: `Screenshots attached to feedback are cut off on Android`,
      description: `Every screenshot I attach from my Pixel loses the bottom third.`,
      creatorId: null,
      label: `Bug`,
      createdAt: hoursAgo(13),
    },
  ]
  const byTitle = new Map<string, IssueRow>()
  const byIdentifier: Record<string, IssueRow> = {}
  for (const [index, spec] of specs.entries()) {
    const duplicateOf = spec.duplicateOf ? byTitle.get(spec.duplicateOf) : undefined
    const [row] = await db
      .insert(issues)
      .values({
        boardId: ctx.boardId,
        teamId: ctx.teamId,
        title: spec.title,
        description: spec.description,
        status: duplicateOf ? `duplicate` : `backlog`,
        duplicateOfId: duplicateOf?.id,
        priority: `medium`,
        creatorId: spec.creatorId,
        // The two creator-less rows are the widget reports the Triage agent
        // sorted (its channel names FB-8 and FB-9).
        source: spec.creatorId ? `user` : `widget`,
        sortOrder: index * 10,
        createdAt: spec.createdAt,
      })
      .returning()
    byTitle.set(row.title, row)
    if (row.identifier) byIdentifier[row.identifier] = row
    if (spec.label) {
      await db.insert(issueLabels).values({
        issueId: row.id,
        labelId: ctx.labelIds[spec.label],
        teamId: ctx.teamId,
        boardId: ctx.boardId,
      })
    }
  }
  return byIdentifier
}

/**
 * EXP-1293 (Threads v2): the session tree — six thread roots (one per
 * Threads-list state, see DEMO_THREAD_IDS), the cold-start thread's child
 * session (DEMO_SESSION_ID), three team agents, the roots' issues, their
 * conversations and their pull requests.
 *
 * No trigger maintains the child tables' mirrors yet, so they are written the
 * way the server writes them at insert: `root_id`/`depth` on sessions,
 * `shared`/`owner_id` (the root's) on session_issues + messages, the issue's
 * `board_id` on session_issues + pull_request_issues.
 *
 * The LIVE rows' state comes from `DEMO_LIVE_SESSIONS` (the stand-in desktop
 * re-asserts it on every beat), so the seed and the stub cannot disagree.
 */
async function seedSessionTree(ctx: {
  teamId: string
  demoId: string
  jonas: string
  repoId: string
  issue: (identifier: string) => IssueRow
}) {
  const { teamId, demoId, repoId, issue } = ctx
  const minutesAgo = (m: number) => new Date(now - m * 60_000)
  const ids = DEMO_THREAD_IDS
  const live = new Map(DEMO_LIVE_SESSIONS.map((spec) => [spec.id, spec]))
  const liveState = (id: string) => {
    const spec = live.get(id)
    if (!spec) throw new Error(`no live spec for session ${id}`)
    return {
      status: spec.status,
      busy: spec.busy,
      needsInput: demoNeedsInput(spec),
      startedAt: minutesAgo(spec.startedMinutesAgo),
      createdAt: minutesAgo(spec.startedMinutesAgo),
      lastTurnAt: new Date(now),
      updatedAt: new Date(now),
    }
  }
  /** A root: a thread on the demo user's desktop unless told otherwise. */
  const root = (
    id: string,
    extra: Partial<typeof sessions.$inferInsert>
  ): typeof sessions.$inferInsert => ({
    id,
    teamId,
    rootId: id,
    depth: 0,
    kind: `thread`,
    userId: demoId,
    deviceId: DEMO_DEVICE_ID,
    cli: `claude`,
    repositoryId: repoId,
    shared: true,
    // Every root dispatches (threads and team agents alike): it runs on the
    // device's thread model and hands real work to child sessions.
    dispatch: true,
    ...extra,
  })

  // ── the team agents (NEW-2: `agents` definition rows) ─────────────────
  // Each runs as the demo user's conversation session with it (a root with
  // `agent_id`, never ended: it idles between messages). Triggers live on
  // the definition and the server fires them.
  const agentSpecs = [
    {
      key: `triage` as const,
      name: `Triage`,
      icon: `bug`,
      color: `#a855f7`,
      instructions: `Look after the boards: merge duplicates, add labels and priorities, start a thread for anything that needs code.`,
      sortOrder: 1,
      triggers: [
        {
          id: DEMO_AGENT_TRIGGER_ID,
          enabled: true,
          kind: `schedule`,
          cron: `0 9 * * *`,
          tz: `Europe/Vienna`,
        },
        { id: DEMO_AGENT_EVENT_TRIGGER_ID, enabled: true, kind: `event`, name: `issue.created` },
      ] satisfies AgentTrigger[],
    },
    {
      key: `designer` as const,
      name: `Designer`,
      icon: `palette`,
      color: `#ec4899`,
      instructions: `Review UI changes against the design system: spacing, colour tokens and icon usage. Comment with a screenshot when something drifts.`,
      sortOrder: 2,
      triggers: [
        { id: `0e7bc9f6-1f25-447c-ace6-9e61b395886f`, enabled: true, kind: `event`, name: `ui-changed` },
      ] satisfies AgentTrigger[],
    },
    {
      key: `release` as const,
      name: `Release`,
      icon: `package`,
      color: `#22c55e`,
      instructions: `Draft release notes from the pull requests merged since the last tag and post them for review.`,
      sortOrder: 3,
      triggers: [] satisfies AgentTrigger[],
    },
  ]
  await db.insert(agents).values(
    agentSpecs.map((spec) => ({
      id: DEMO_AGENT_IDS[spec.key],
      teamId,
      name: spec.name,
      icon: spec.icon,
      color: spec.color,
      instructions: spec.instructions,
      triggers: spec.triggers,
      // Agents run on the team's shared build server, hosted by its owner.
      deviceId: DEMO_SERVER_DEVICE_ID,
      cli: `claude`,
      account: `system`,
      ownerId: demoId,
      sortOrder: spec.sortOrder,
      createdAt: daysAgo(20),
      updatedAt: daysAgo(20),
    }))
  )
  // S-3: the demo user's Assistant (agent-settings.json `assistant`): what
  // `agents.ensureAssistant` would create on first use, seeded so every
  // client photographs it at once: the copy's name, no icon/colour (the
  // brand mark), the default hourly check-in, on the demo desktop.
  await db.insert(agents).values({
    id: DEMO_ASSISTANT_ID,
    teamId,
    name: ASSISTANT_COPY.name,
    icon: null,
    color: null,
    instructions: `Keep my mornings quiet: batch what needs me into one update.`,
    triggers: assistantScheduleTriggers(`1h`).map((trigger) => ({
      ...trigger,
      id: `2a9c4e6f-8b1d-4f3a-9c5e-7d1f3b5a9c2e`,
    })) satisfies AgentTrigger[],
    deviceId: DEMO_DEVICE_ID,
    cli: `claude`,
    ownerId: demoId,
    role: `assistant`,
    sortOrder: 0,
    createdAt: daysAgo(20),
    updatedAt: daysAgo(20),
  })
  await db.insert(sessions).values(
    agentSpecs.map((spec) =>
      root(DEMO_AGENT_CONVERSATION_IDS[spec.key], {
        agentId: DEMO_AGENT_IDS[spec.key],
        // Legacy read shims (≤0.14.74 devices): kind + the agent's facets.
        kind: `agent`,
        name: spec.name,
        icon: spec.icon,
        color: spec.color,
        title: spec.name,
        deviceId: DEMO_SERVER_DEVICE_ID,
        account: `system`,
        repositoryId: null,
        status: `idle`,
        startedAt: daysAgo(20),
        lastTurnAt: DEMO_AGENT_CHANNEL_TIMES.reply,
        createdAt: daysAgo(20),
        updatedAt: DEMO_AGENT_CHANNEL_TIMES.reply,
      })
    )
  )
  // #EXP-1456: the demo user's Assistant conversation, idle on its question.
  await db.insert(sessions).values(
    root(DEMO_ASSISTANT_CONVERSATION_ID, {
      agentId: DEMO_ASSISTANT_ID,
      kind: `agent`,
      name: ASSISTANT_COPY.name,
      title: ASSISTANT_COPY.name,
      repositoryId: null,
      shared: false,
      status: `idle`,
      startedAt: daysAgo(20),
      lastTurnAt: minutesAgo(4),
      createdAt: daysAgo(20),
      updatedAt: minutesAgo(4),
    })
  )

  // ── VAPP-110 + VAPP-122: the team's apps (the `apps` / `app` views) ────
  // The Working hours app (DEMO_APP_ID, the `app` view's subject): three
  // versions, the demo user's EDITOR SESSION with it (a root with `app_id`,
  // idle: the person is in the editor) and its RECORDS (a month of the demo
  // user's hours, a settings record, two of Jonas's days). The Team
  // dashboard app keeps its three versions (the head binds the feedback
  // board's issues). A third app has no version yet (the list's "No version
  // yet" pill).
  const feedbackBoardId = issue(`FB-3`).boardId
  const appPackage = (version: `1.0.0` | `1.1.0` | `1.2.0`) => {
    const components: Record<string, unknown>[] = [
      { id: `root`, component: `Stack`, gap: `md`, children: [`band`, `stats`, ...(version !== `1.0.0` ? [`chart`] : []), ...(version === `1.2.0` ? [`issues`] : [])] },
      { id: `band`, component: `Section`, title: `Product Feedback`, icon: `nav-threads` },
      { id: `stats`, component: `Grid`, columns: 2, gap: `sm`, children: [`open`, `rollout`] },
      { id: `open`, component: `Card`, title: `Open issues`, children: [`openValue`] },
      { id: `openValue`, component: `Text`, text: { path: `/issues/count` }, variant: `title` },
      { id: `rollout`, component: `Card`, title: `Rollout`, children: [`bar`] },
      { id: `bar`, component: `Progress`, value: 72, max: 100, label: `This week`, tone: `primary` },
    ]
    if (version !== `1.0.0`) {
      components.push({
        id: `chart`,
        component: `Chart`,
        kind: `line`,
        title: `Reports per day`,
        height: 140,
        categories: [`Mon`, `Tue`, `Wed`, `Thu`, `Fri`, `Sat`, `Sun`],
        series: [{ name: `Reports`, values: [4, 7, 5, 9, 6, 3, 2] }],
      })
    }
    if (version === `1.2.0`) {
      components.push(
        { id: `issues`, component: `List`, divided: true, children: { componentId: `issueRow`, path: `/issues/rows` } },
        {
          id: `issueRow`,
          component: `Row`,
          identifier: { path: `identifier` },
          title: { path: `title` },
          pressable: true,
          chevron: `always`,
          on: { press: { functionCall: { call: `harness.openIssue`, args: { identifier: { path: `identifier` } } } } },
        }
      )
    }
    return {
      id: `acme.team-dashboard`,
      name: `Team dashboard`,
      version,
      catalogId: `https://ui.exponential.at/catalogs/exponential-app/v1`,
      icon: { glyph: `chart-line` },
      functions: [`harness.openIssue`, `harness.toast`],
      sources: [`exp:issues`],
      templates: {
        dashboard: {
          data: { issues: { rows: [], count: 0 } },
          bindings: [{ path: `/issues`, source: `exp:issues?board=${feedbackBoardId}&limit=6` }],
          components,
        },
      },
    }
  }
  // The Working hours package, by version: v1 = the month, the total and the
  // Log hours form; v2 = the target (a settings record) and what is left;
  // v3 = previous months through `bind` and the per-day trend (the SDK's
  // example package, packages/ui/exponential-ui/templates/working-hours.json).
  // v4 (VAPP-119) = the whole template: a teammate's month through
  // exp:members + bind, the issues you worked on as exp:issues suggestions.
  // v5 (VAPP-120) = the whole template: "Post to the issue" calls the typed
  // action exponential.comments.create, granted on the settings page.
  const hoursPackage = (version: `1.0.0` | `1.1.0` | `1.2.0` | `1.3.0` | `1.4.0`) => {
    const full = workingHoursTemplate as unknown as {
      version: string
      sources: string[]
      functions: string[]
      templates: { month: { components: Record<string, unknown>[]; bindings: { path: string; source: string }[]; data: Record<string, unknown> } }
    }
    if (version === `1.4.0`) return { ...full, version }
    const v4 = version === `1.3.0`
    const dropped = new Set<string>([
      `editorFooter`,
      `postComment`,
      ...(v4 ? [] : [`toolbarActions`, `who`]),
      ...(version === `1.0.0` ? [`prev`, `next`, `statTarget`, `statLeft`, `target`, `targetRow`, `targetField`, `targetSave`] : version === `1.1.0` ? [`prev`, `next`] : []),
    ])
    const components = full.templates.month.components
      .filter((c) => !dropped.has(c.id as string))
      .map((c) => {
        const out: Record<string, unknown> = { ...c }
        if (Array.isArray(out.children)) out.children = (out.children as string[]).filter((id) => !dropped.has(id))
        // Before v5 the dialog's footer is the Delete button alone.
        if (out.id === `editor`) out.slots = { footer: `deleteEntry` }
        if (v4) return out
        // Before v4 the Log hours button sits in the toolbar's trailing slot itself.
        if (out.id === `toolbar`) out.slots = { trailing: `log` }
        if (out.id === `issues`) {
          delete out.suggestions
          out.placeholder = `MET-1437`
        }
        if ((out.id === `prev` || out.id === `next`) && out.on) {
          const on = JSON.parse(JSON.stringify(out.on)) as { press: { functionCall: { args: { params: Record<string, unknown> } } } }
          delete on.press.functionCall.args.params.user
          out.on = on
        }
        if (version !== `1.2.0` && out.id === `statTotal`) delete out.trend
        if (version === `1.0.0` && out.id === `stats`) out.columns = 1
        return out
      })
    const functions = full.functions.filter((f) => !f.startsWith(`exponential.`))
    if (v4) return { ...full, version, functions, templates: { month: { ...full.templates.month, components } } }
    const { members: _m, issues: _i, who: _w, ...data } = full.templates.month.data
    return {
      ...full,
      version,
      functions,
      sources: [`exp:records`],
      templates: { month: { ...full.templates.month, data, bindings: full.templates.month.bindings.filter((b) => b.source.startsWith(`exp:records`)), components } },
    }
  }
  await db.insert(apps).values([
    {
      id: DEMO_APP_ID,
      teamId,
      slug: DEMO_APP_SLUG,
      name: `Working hours`,
      icon: `clock`,
      color: `#10b981`,
      description: `Your hours per day, a month at a time: log them, see the total against the target, keep the issues you worked on.`,
      package: hoursPackage(`1.4.0`),
      headVersion: 5,
      // VAPP-120: nothing granted yet: "Post to the issue" asks with the
      // operation's words on every platform (the consent card shots); the
      // settings page shows the declared action with its switch off.
      grantedTools: [],
      createdBy: demoId,
      createdAt: daysAgo(6),
      updatedAt: DEMO_APP_CHAT_TIMES.fifth,
    },
    {
      id: DEMO_APP_DASHBOARD_ID,
      teamId,
      slug: DEMO_APP_DASHBOARD_SLUG,
      name: `Team dashboard`,
      icon: `chart-line`,
      color: `#0ea5e9`,
      description: `Open feedback, this week's rollout and the newest reports at a glance.`,
      package: appPackage(`1.2.0`),
      headVersion: 3,
      createdBy: demoId,
      createdAt: daysAgo(9),
      updatedAt: daysAgo(8),
    },
    {
      id: DEMO_APP_EMPTY_ID,
      teamId,
      slug: `release-checklist`,
      name: `Release checklist`,
      icon: `rocket`,
      color: `#f59e0b`,
      description: `The steps before a store release, ticked off per platform.`,
      package: null,
      headVersion: 0,
      createdBy: ctx.jonas,
      createdAt: daysAgo(1),
      updatedAt: daysAgo(1),
    },
  ])
  await db.insert(sessions).values([
    root(DEMO_APP_EDITOR_SESSION_ID, {
      appId: DEMO_APP_ID,
      title: `Working hours`,
      repositoryId: null,
      status: `idle`,
      startedAt: DEMO_APP_CHAT_TIMES.request,
      lastTurnAt: DEMO_APP_CHAT_TIMES.fifth,
      createdAt: DEMO_APP_CHAT_TIMES.request,
      updatedAt: DEMO_APP_CHAT_TIMES.fifth,
    }),
  ])
  await db.insert(appVersions).values([
    ...(
      [
        [DEMO_APP_VERSION_IDS[0], 1, `1.0.0`, `First cut: the month, the total and a Log hours form`, DEMO_APP_CHAT_TIMES.first],
        [DEMO_APP_VERSION_IDS[1], 2, `1.1.0`, `A monthly target and what is left`, DEMO_APP_CHAT_TIMES.second],
        [DEMO_APP_VERSION_IDS[2], 3, `1.2.0`, `Previous months with the switcher, the per-day trend`, DEMO_APP_CHAT_TIMES.third],
        [DEMO_APP_VERSION_IDS[3], 4, `1.3.0`, `A teammate's month, the issues you worked on`, DEMO_APP_CHAT_TIMES.fourth],
        [DEMO_APP_VERSION_IDS[4], 5, `1.4.0`, `Post a day to its issue as a comment`, DEMO_APP_CHAT_TIMES.fifth],
      ] as const
    ).map(([id, number, version, note, at]) => ({
      id,
      appId: DEMO_APP_ID,
      teamId,
      number,
      package: hoursPackage(version),
      note,
      sessionId: DEMO_APP_EDITOR_SESSION_ID,
      createdBy: null,
      createdAt: at,
    })),
    ...(
      [
        [DEMO_APP_DASHBOARD_VERSION_IDS[0], 1, `1.0.0`, `First cut: open issues and the rollout`, daysAgo(9)],
        [DEMO_APP_DASHBOARD_VERSION_IDS[1], 2, `1.1.0`, `Added the reports chart`, daysAgo(8.5)],
        [DEMO_APP_DASHBOARD_VERSION_IDS[2], 3, `1.2.0`, `Newest reports as rows, open on tap`, daysAgo(8)],
      ] as const
    ).map(([id, number, version, note, at]) => ({
      id,
      appId: DEMO_APP_DASHBOARD_ID,
      teamId,
      number,
      package: appPackage(version),
      note,
      sessionId: null,
      createdBy: demoId,
      createdAt: at,
    })),
  ])
  // VAPP-122: the Working hours records: the demo user's October (26.5 h, the
  // spreadsheet it replaces), two September days (the month switcher), the
  // target, and two of Jonas's days (`mine` keeps them out of the view).
  const entry = (id: string, by: string, date: string, hours: number, note: string, issues: string[] = []) => ({
    id,
    appId: DEMO_APP_ID,
    teamId,
    kind: `entry`,
    data: { date, hours, note, issues },
    createdBy: by,
    createdAt: new Date(`${date}T18:00:00Z`),
    updatedAt: new Date(`${date}T18:00:00Z`),
  })
  await db.insert(appRecords).values([
    entry(DEMO_APP_RECORD_IDS[0], demoId, `2026-10-05`, 3, `leaderboard, meet`, [`APP-3`]),
    entry(DEMO_APP_RECORD_IDS[1], demoId, `2026-10-09`, 3, `academy hiding, mcp`, [`APP-6`]),
    entry(DEMO_APP_RECORD_IDS[2], demoId, `2026-10-12`, 3, `support`),
    entry(DEMO_APP_RECORD_IDS[3], demoId, `2026-10-15`, 2, `leaderboard, stripe, templates`, [`APP-10`]),
    entry(DEMO_APP_RECORD_IDS[4], demoId, `2026-10-17`, 0.5, `statistics lifetime`),
    entry(DEMO_APP_RECORD_IDS[5], demoId, `2026-10-19`, 1, `meet, stats, support`),
    entry(DEMO_APP_RECORD_IDS[6], demoId, `2026-10-21`, 8, `docs, admin`, [`APP-15`]),
    entry(DEMO_APP_RECORD_IDS[7], demoId, `2026-10-22`, 3, `admin`),
    entry(DEMO_APP_RECORD_IDS[8], demoId, `2026-10-23`, 3, `support, admin, orgs`),
    entry(DEMO_APP_RECORD_IDS[9], demoId, `2026-09-29`, 4, `release train`),
    entry(DEMO_APP_RECORD_IDS[10], demoId, `2026-09-30`, 2, `docs`),
    entry(DEMO_APP_RECORD_IDS[11], ctx.jonas, `2026-10-02`, 2, `Memleak`, [`APP-5`]),
    entry(DEMO_APP_RECORD_IDS[12], ctx.jonas, `2026-10-05`, 4, `MET sync`),
    {
      id: DEMO_APP_RECORD_IDS[13],
      appId: DEMO_APP_ID,
      teamId,
      kind: `settings`,
      data: { hours_per_month: 40 },
      createdBy: demoId,
      createdAt: DEMO_APP_CHAT_TIMES.second,
      updatedAt: DEMO_APP_CHAT_TIMES.second,
    },
  ])

  // ── the thread roots ───────────────────────────────────────────────────
  const batchGuide = [
    guideText(
      `Summary`,
      `Multi-select on the board now batch-edits labels: pick rows, open Labels, toggle. One request per change, so a 40-issue edit lands as one undo step.`,
      minutesAgo(100),
      undefined,
      ids.batchLabels
    ),
    guideText(
      `Label picker on the selection bar`,
      `The selection bar gained a Labels button that opens the usual picker with a mixed state for labels only some of the rows carry.`,
      minutesAgo(99),
      [`src/board/selection-bar.tsx`, `src/board/label-batch.ts`, `src/board/label-batch.test.ts`],
      ids.batchLabels
    ),
    // EXP-1283: two real pictures under the topic, so the Guide picture
    // viewer has something to open (`1 / 2`).
    ...(await seedDemoGuidePictures({
      teamId,
      sessionId: ids.batchLabels,
      uploaderId: demoId,
      at: minutesAgo(99),
    })),
  ]
  const coldStartGuide = [
    guideText(
      `Summary`,
      `Deferred the shape subscribe until after the first frame and added a board snapshot cache that paints the last board before sync finishes: 740 ms on the CI device, under the 800 ms target. Filed #${issue(`FB-3`).identifier} for the editor's highlighting bundle.`,
      minutesAgo(12),
      undefined,
      DEMO_SESSION_ID
    ),
    guideText(
      `Board snapshot cache`,
      `The last rendered board is written to disk on every sync and painted on launch, then replaced in place once the shape catches up.`,
      minutesAgo(11),
      [`src/board/snapshot-cache.ts`, `src/sync/bootstrap.ts`],
      DEMO_SESSION_ID
    ),
  ]
  await db.insert(sessions).values([
    root(ids.coldStart, {
      ...liveState(ids.coldStart),
      branch: `exp/APP-5`,
      guide: coldStartGuide,
      // S-3: the Triage agent's conversation started it (the Conversation
      // opens with `Started by Triage ›`).
      startedByAgentId: DEMO_AGENT_IDS.triage,
    }),
    root(ids.groupByAssignee, {
      ...liveState(ids.groupByAssignee),
      cli: `codex`,
      branch: `exp/APP-14`,
      // EXP-1323: the live caption under its name.
      caption: `Editing src/board/group-by.tsx`,
    }),
    root(ids.sortReports, {
      ...liveState(ids.sortReports),
      title: `Sort today's widget reports`,
      deviceId: DEMO_SERVER_DEVICE_ID,
      branch: `exp/FB-9`,
      spawnedBySessionId: DEMO_AGENT_CONVERSATION_ID,
    }),
    root(ids.offlineSpike, {
      ...liveState(ids.offlineSpike),
      title: `Spike: offline queue`,
      // Not shared: only its starter sees it (the list's lock).
      shared: false,
      repositoryId: null,
    }),
    root(ids.batchLabels, {
      status: `idle`,
      branch: `exp/APP-15`,
      guide: batchGuide,
      startedAt: minutesAgo(140),
      lastTurnAt: minutesAgo(98),
      createdAt: minutesAgo(140),
      updatedAt: minutesAgo(98),
    }),
    root(ids.scrollRestore, {
      status: `ended`,
      branch: `exp/APP-16`,
      // The re-clock anchor (lib/demo-reclock.ts): its offset IS the constant.
      startedAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo + 2),
      lastTurnAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo),
      endedAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo),
      createdAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo + 2),
      updatedAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo),
    }),
  ])
  // The cold-start thread's child: the session the agent started to profile
  // the launch path, waiting on the question at the end of the conversation.
  await db.insert(sessions).values({
    ...root(DEMO_SESSION_ID, liveState(DEMO_SESSION_ID)),
    parentId: ids.coldStart,
    rootId: ids.coldStart,
    depth: 1,
    dispatch: false,
    title: `Profile the launch path`,
    branch: `exp/APP-5`,
  })
  // EXP-1323: a second child, started from the suggested tasks card and
  // ended: its end landed on the thread as a `Report from …` line.
  await db.insert(sessions).values({
    ...root(DEMO_REPORT_SESSION_ID, {
      status: `ended`,
      startedAt: minutesAgo(47),
      lastTurnAt: minutesAgo(20),
      endedAt: minutesAgo(20),
      createdAt: minutesAgo(47),
      updatedAt: minutesAgo(20),
    }),
    parentId: ids.coldStart,
    rootId: ids.coldStart,
    depth: 1,
    title: `Measure on the Galaxy A54`,
    branch: `exp/APP-5-a54`,
  })

  // EXP-1341: an UNNAMED child the dispatcher handed two issues: its name is
  // its assigned share (`#APP-10 … + 1`), never `Session N`.
  await db.insert(sessions).values({
    ...root(DEMO_GROUPED_SESSION_ID, {
      status: `running`,
      busy: true,
      startedAt: minutesAgo(12),
      lastTurnAt: minutesAgo(1),
      createdAt: minutesAgo(12),
      updatedAt: minutesAgo(1),
    }),
    parentId: ids.coldStart,
    rootId: ids.coldStart,
    depth: 1,
    dispatch: false,
    title: null,
    branch: `exp/APP-10`,
  })

  // ── the roots' issues ──────────────────────────────────────────────────
  const link = (
    sessionId: string,
    identifier: string,
    sortOrder: number,
    extra: {
      shared?: boolean
      source?: `user` | `agent`
      by?: string
      to?: string
      at?: Date
    } = {}
  ): typeof sessionIssues.$inferInsert => {
    const row = issue(identifier)
    // NEW-1: a link is a row on the session that HOLDS the issue (`to` = the
    // child it was forwarded to); the root's strip is the union over its
    // tree. `assigned_session_id` is a never-written read shim.
    return {
      sessionId: extra.to ?? sessionId,
      issueId: row.id,
      teamId,
      boardId: row.boardId,
      shared: extra.shared ?? true,
      ownerId: demoId,
      source: extra.source ?? `user`,
      addedBy: demoId,
      addedBySessionId: extra.by,
      sortOrder,
      createdAt: extra.at,
      updatedAt: extra.at,
    }
  }
  await db.insert(sessionIssues).values([
    link(ids.coldStart, `APP-5`, 1, { at: minutesAgo(60) }),
    // Linked by the child's agent a few minutes in, then the one it FILED.
    link(ids.coldStart, `APP-10`, 2, {
      source: `agent`,
      by: DEMO_SESSION_ID,
      to: DEMO_GROUPED_SESSION_ID,
      at: minutesAgo(46),
    }),
    link(ids.coldStart, `FB-3`, 3, {
      source: `agent`,
      by: DEMO_SESSION_ID,
      to: DEMO_GROUPED_SESSION_ID,
      at: minutesAgo(30),
    }),
    link(ids.groupByAssignee, `APP-14`, 1),
    link(ids.sortReports, `FB-9`, 1, { source: `agent`, by: DEMO_AGENT_CONVERSATION_ID }),
    link(ids.batchLabels, `APP-15`, 1),
    link(ids.scrollRestore, `APP-16`, 1),
  ])

  // ── conversations ──────────────────────────────────────────────────────
  // NEW-3: every seeded session got its conversation from the
  // `create_session_conversation` trigger; a message names it.
  const conversationBySession = new Map(
    (
      await db
        .select({ id: conversations.id, sessionId: conversations.sessionId })
        .from(conversations)
        .where(eq(conversations.teamId, teamId))
    ).map((row) => [row.sessionId ?? ``, row.id])
  )
  const conversationOf = (sessionId: string): string => {
    const id = conversationBySession.get(sessionId)
    if (!id) throw new Error(`seed: session ${sessionId} has no conversation`)
    return id
  }
  type MessageSpec = {
    sessionId: string
    author: `user` | `agent`
    body?: string
    card?: MessageCard
    at: Date
    shared?: boolean
    /** A child session's message: its thread's root. */
    rootId?: string
    /** S-3: the session that posted it (its own card = a story). */
    authorSessionId?: string
  }
  const message = (spec: MessageSpec): typeof messages.$inferInsert => ({
    conversationId: conversationOf(spec.sessionId),
    sessionId: spec.sessionId,
    rootId: spec.rootId ?? spec.sessionId,
    teamId,
    shared: spec.shared ?? true,
    ownerId: demoId,
    authorKind: spec.author,
    authorId: spec.author === `user` ? demoId : null,
    authorSessionId: spec.authorSessionId ?? null,
    body: spec.body ?? ``,
    card: spec.card ?? null,
    // A person's message the device already delivered (none sits queued).
    consumedAt: spec.author === `user` ? new Date(spec.at.getTime() + 2_000) : null,
    createdAt: spec.at,
    updatedAt: spec.at,
  })
  const fb3 = issue(`FB-3`).identifier
  const DEMO_THREAD_PLAN = [
    `## Plan: lazy-load the markdown editor`,
    ``,
    `1. Split the TipTap bundle out of the board route's entry chunk.`,
    `2. Load it on the first focus of a description or comment field.`,
    `3. Keep a plain-text fallback painting until the editor is ready.`,
    `4. Re-measure cold start on the Pixel 6a and the CI device.`,
    `5. Open the PR with the before/after numbers.`,
  ].join(`\n`)
  // STORIES TAB (stories.json, the `thread-stories` view): APP-14's thread
  // keeps a settled history beside its waiting draft: a result with two
  // real pictures (the board's committed web + iOS shots) and an approved
  // plan. Both are older than the ring's keep window / settled, so the chat
  // list's rings never change; the tab keeps them for good.
  const assigneePictures = (
    await seedDemoGuidePictures({ teamId, sessionId: ids.groupByAssignee, uploaderId: demoId, at: hoursAgo(27) })
  ).map((entry, index) => ({
    label: `${entry.label} assignee groups`,
    attachmentId: entry.attachmentId!,
    width: entry.width ?? null,
    height: entry.height ?? null,
    caption: index === 0 ? `Grouped by assignee` : `iPhone`,
  }))
  await db.insert(messages).values(
    ([
      {
        sessionId: ids.groupByAssignee,
        author: `agent`,
        authorSessionId: ids.groupByAssignee,
        card: { kind: `plan`, plan: `## Group by assignee\n\nA third grouping mode beside Status, remembered per board.`, decision: `approved` },
        at: hoursAgo(30),
      },
      {
        sessionId: ids.groupByAssignee,
        author: `agent`,
        authorSessionId: ids.groupByAssignee,
        card: {
          kind: `surface`,
          story: true,
          title: `Assignee groups`,
          surface: { root: { id: `groups`, component: `Text`, props: { text: `Grouped by assignee` } }, data: {} },
          pictures: assigneePictures,
        },
        at: hoursAgo(26),
      },
    ] as MessageSpec[]).map(message)
  )
  await db.insert(messages).values(
    [
      // The cold-start thread (the `thread` view).
      {
        sessionId: ids.coldStart,
        author: `user`,
        body: `Cold start is at 1.4s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of the critical path.`,
        at: minutesAgo(60),
      },
      {
        sessionId: ids.coldStart,
        author: `agent`,
        card: { kind: `session`, sessionId: DEMO_SESSION_ID },
        at: minutesAgo(52),
      },
      {
        sessionId: ids.coldStart,
        author: `agent`,
        body: `### Board snapshot cache\n\nThe last rendered board is written to disk on every sync and painted on launch, then replaced in place once the shape catches up.\n\nDeferred the shape subscribe until after the first frame and added a board snapshot cache that paints the last board before sync finishes: 740 ms on the CI device, under the 800 ms target. Filed #${fb3} for the editor's highlighting bundle.`,
        at: minutesAgo(11),
      },
      // S-3 stories.json: the session's own card = a waiting story.
      {
        sessionId: ids.coldStart,
        author: `agent`,
        authorSessionId: ids.coldStart,
        card: {
          kind: `question`,
          question: DEMO_FEED_QUESTION,
          // EXP-1322 / #1080: labels in `options`, descriptions in the
          // parallel sidecar (object options are no longer read by any
          // client from 0.14.61/0.14.63/0.14.71 on).
          options: [`Also lazy-load the markdown editor`, `Open the PR with what we have`],
          optionDescriptions: [`~90ms more, touches the editor entry point`, `Target is met; keep the diff small`],
          answer: null,
        },
        at: minutesAgo(2),
      },
      // The ask dock (ask-dock.json): the child's plan waits too, so the
      // thread stacks two cards in the composer's slot.
      {
        sessionId: DEMO_SESSION_ID,
        rootId: ids.coldStart,
        author: `agent`,
        authorSessionId: DEMO_SESSION_ID,
        card: { kind: `plan`, plan: DEMO_THREAD_PLAN, decision: null },
        at: minutesAgo(1),
      },
      // The other threads open on their request.
      {
        sessionId: ids.groupByAssignee,
        author: `user`,
        body: `Add an assignee grouping mode next to the status grouping. Remember the last choice per board.`,
        at: minutesAgo(25),
      },
      // S-3 stories.json: a DRAFT story waiting on a choice (the chat
      // list's second ring; the story-viewer's draft).
      {
        sessionId: ids.groupByAssignee,
        author: `agent`,
        authorSessionId: ids.groupByAssignee,
        card: {
          kind: `question`,
          question: `Use this grouping menu?`,
          options: [`Ship it`, `Try a segmented control`],
          custom: true,
          surface: {
            root: {
              id: `menu`,
              component: `Card`,
              children: [
                { id: `menu-title`, component: `Heading`, props: { text: `Group by` } },
                { id: `menu-status`, component: `Row`, props: { title: `Status`, value: `` } },
                { id: `menu-assignee`, component: `Row`, props: { title: `Assignee`, value: `✓` } },
                { id: `menu-label`, component: `Row`, props: { title: `Label`, value: `` } },
              ],
            },
            data: {},
          },
          answer: null,
        },
        at: minutesAgo(6),
      },
      {
        sessionId: ids.sortReports,
        author: `agent`,
        body: `#${issue(`FB-9`).identifier}: screenshots attached from Android lose their bottom third. Reproduce with a Pixel-sized capture, fix the crop and open a PR.`,
        at: minutesAgo(8),
      },
      {
        sessionId: ids.offlineSpike,
        author: `user`,
        body: `Spike an offline queue for issue edits: persist each edit with the value it was based on and replay in order once sync is back. No PR, just tell me what breaks.`,
        at: minutesAgo(40),
        shared: false,
      },
      {
        sessionId: ids.batchLabels,
        author: `user`,
        body: `Let me select several issues on the board and change their labels in one go.`,
        at: minutesAgo(140),
      },
      {
        sessionId: ids.batchLabels,
        author: `agent`,
        body: `Done: the selection bar has a Labels button now, and a 40-issue edit lands as one undo step. The PR is open and green.`,
        at: minutesAgo(98),
      },
      // EXP-1211: its quick replies above the composer (the `thread-replies`
      // view); never a Conversation row.
      {
        sessionId: ids.batchLabels,
        author: `agent`,
        card: {
          kind: `replies`,
          items: [
            { id: `merge`, text: `Merge it` },
            { id: `test`, text: `Add a test for the undo step` },
            { id: `diff`, text: `Show me the diff` },
          ],
        },
        at: minutesAgo(97),
      },
      {
        sessionId: ids.scrollRestore,
        author: `user`,
        body: `The issue list sometimes jumps back to the top after closing an issue. Find out why and fix it.`,
        at: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo + 2),
      },
      // The demo user's conversation with the Triage agent (the
      // `agent-channel` view).
      {
        sessionId: DEMO_AGENT_CONVERSATION_ID,
        author: `agent`,
        body: `Two new widget reports on Product Feedback. ${issue(`FB-8`).identifier} is a duplicate of ${fb3}, merged it. ${issue(`FB-9`).identifier} needs code, so I started a thread:`,
        at: DEMO_AGENT_CHANNEL_TIMES.report,
      },
      {
        sessionId: DEMO_AGENT_CONVERSATION_ID,
        author: `agent`,
        card: { kind: `thread_started`, sessionId: ids.sortReports },
        at: DEMO_AGENT_CHANNEL_TIMES.threadStarted,
      },
      {
        sessionId: DEMO_AGENT_CONVERSATION_ID,
        author: `user`,
        body: DEMO_AGENT_REQUEST,
        at: DEMO_AGENT_CHANNEL_TIMES.request,
      },
      {
        sessionId: DEMO_AGENT_CONVERSATION_ID,
        author: `agent`,
        body: `Done, 3 issues labelled android.`,
        at: DEMO_AGENT_CHANNEL_TIMES.reply,
      },
      // S-3: the Triage agent's result story (its ring on the chat list).
      {
        sessionId: DEMO_AGENT_CONVERSATION_ID,
        author: `agent`,
        authorSessionId: DEMO_AGENT_CONVERSATION_ID,
        card: {
          kind: `surface`,
          story: true,
          title: `Today's triage`,
          surface: {
            root: {
              id: `triage`,
              component: `Box`,
              style: { display: `flex`, flexDirection: `column`, gap: `$spacing.sm` },
              children: [
                { id: `triage-new`, component: `Row`, props: { title: `New reports`, value: `2` } },
                { id: `triage-dup`, component: `Row`, props: { title: `Merged as duplicate`, value: `1` } },
                { id: `triage-label`, component: `Row`, props: { title: `Labelled android`, value: `3` } },
              ],
            },
            data: {},
          },
        },
        // Fresh: a result story drops after stories.json `unseenHours`.
        at: minutesAgo(40),
      },
      // #EXP-1456: the Assistant's decision (the inbox row's target).
      {
        sessionId: DEMO_ASSISTANT_CONVERSATION_ID,
        author: `agent`,
        authorSessionId: DEMO_ASSISTANT_CONVERSATION_ID,
        card: {
          kind: `question`,
          question: DEMO_ASSISTANT_QUESTION,
          options: [`Ship it today`, `Wait for the fix`],
          answer: null,
        },
        at: minutesAgo(4),
      },
      // VAPP-110 + VAPP-122: the demo user's editor session with the Working
      // hours app (the `app` view's chat), one exchange per version.
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `user`,
        body: `Replace my hours spreadsheet: every day of the month, how many hours I worked and what I did. Mine only, with the month's total.`,
        at: DEMO_APP_CHAT_TIMES.request,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `agent`,
        body: `Published v1: a month view bound to your own records (kind entry), a Log hours form that writes them, the total at the top.`,
        at: DEMO_APP_CHAT_TIMES.first,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `user`,
        body: `Add a monthly target and show what is left. I want to change the target in the app.`,
        at: DEMO_APP_CHAT_TIMES.change,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `agent`,
        body: `v2: a settings record keeps the target; the tiles show the target and what is left, Save target writes it.`,
        at: DEMO_APP_CHAT_TIMES.second,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `user`,
        body: `Let me switch to previous months, and show the days as a small trend.`,
        at: DEMO_APP_CHAT_TIMES.polish,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `agent`,
        body: `v3: previous and next re-bind the month through bind, the total tile carries the per-day sparkline, an entry opens for editing.`,
        at: DEMO_APP_CHAT_TIMES.third,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `user`,
        body: `Let me look at a teammate's month too, and suggest my open issues when I log a day.`,
        at: DEMO_APP_CHAT_TIMES.team,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `agent`,
        body: `v4: a member picker from exp:members re-binds the month for that person; the issues field suggests your assigned issues from exp:issues.`,
        at: DEMO_APP_CHAT_TIMES.fourth,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `user`,
        body: `When I log a day on an issue, let me post it there as a comment.`,
        at: DEMO_APP_CHAT_TIMES.post,
      },
      {
        sessionId: DEMO_APP_EDITOR_SESSION_ID,
        author: `agent`,
        body: `v5: Post to the issue calls exponential.comments.create with the hours and the note; allow it once on the app's settings page and it posts without asking.`,
        at: DEMO_APP_CHAT_TIMES.fifth,
      },
    ].map((spec) => message(spec as MessageSpec))
  )

  // NEW-2: the team's event stream (Settings → Agents lists it): two server
  // builtins and a custom one a thread emitted over MCP `events_emit`.
  await db.insert(events).values([
    {
      teamId,
      name: `ui-changed`,
      payload: { files: [`src/board/selection-bar.tsx`] },
      emittedBy: `session`,
      sessionId: ids.batchLabels,
      createdAt: minutesAgo(12),
    },
    {
      teamId,
      name: `issue.created`,
      payload: { issueId: issue(`FB-9`).id, identifier: issue(`FB-9`).identifier },
      emittedBy: `server`,
      createdAt: minutesAgo(48),
    },
    {
      teamId,
      name: `pr.merged`,
      payload: {},
      emittedBy: `server`,
      createdAt: hoursAgo(3),
    },
    {
      teamId,
      name: `deploy:staging`,
      payload: {},
      emittedBy: `user`,
      userId: demoId,
      createdAt: hoursAgo(5),
    },
  ])

  // EXP-1323: the cold-start thread's suggested tasks card (one started, two
  // open) and the started child's report, posted by that child.
  const thread = (spec: Partial<typeof messages.$inferInsert> & { at: Date }) => ({
    conversationId: conversationOf(spec.sessionId ?? ids.coldStart),
    sessionId: ids.coldStart,
    rootId: ids.coldStart,
    teamId,
    shared: true,
    ownerId: demoId,
    authorKind: `agent` as const,
    authorId: null,
    body: ``,
    consumedAt: spec.at,
    createdAt: spec.at,
    updatedAt: spec.at,
    ...spec,
  })
  await db.insert(messages).values([
    thread({
      at: minutesAgo(48),
      card: {
        kind: `suggestions`,
        items: [
          {
            id: `measure-a54`,
            title: `Measure on the Galaxy A54`,
            text: `Check the 800 ms target on a mid-range Android before the PR.`,
            sessionId: DEMO_REPORT_SESSION_ID,
          },
          {
            id: `lazy-editor`,
            title: `Lazy-load the markdown editor`,
            text: `About 90 ms more off the launch path; touches the editor entry point.`,
          },
          {
            id: `icon-cache`,
            title: `Cache the board icons`,
            text: `The icons decode on the main thread during the first frame.`,
          },
        ],
      },
    }),
    // S-3: the profiling child's relayed turn (`Message from Profile the
    // launch path ›` over its bubble).
    thread({
      at: minutesAgo(30),
      authorSessionId: DEMO_SESSION_ID,
      body: `The sync bootstrap blocks the first frame for 410 ms; deferring the shape subscribe now.`,
    }),
    thread({
      at: minutesAgo(20),
      authorSessionId: DEMO_REPORT_SESSION_ID,
      body: `740 ms cold start on the Galaxy A54, under the 800 ms target. The snapshot cache paints in 210 ms.`,
      card: {
        kind: `report`,
        sessionId: DEMO_REPORT_SESSION_ID,
        summary: `740 ms cold start on the Galaxy A54, under the 800 ms target. The snapshot cache paints in 210 ms.`,
      },
    }),
    // S-3 / Chats `surfaceCard`: the agent's inline show card.
    thread({
      at: minutesAgo(9),
      // S-3: a result story (stories.json) in the thread's ring.
      authorSessionId: ids.coldStart,
      card: {
        kind: `surface`,
        story: true,
        title: `Launch timings`,
        surface: {
          root: {
            id: `timings`,
            component: `Box`,
            style: { display: `flex`, flexDirection: `column`, gap: `$spacing.sm` },
            children: [
              { id: `timings-pixel`, component: `Row`, props: { title: `Pixel 6a`, value: `760 ms` } },
              { id: `timings-a54`, component: `Row`, props: { title: `Galaxy A54`, value: `740 ms` } },
              { id: `timings-target`, component: `Row`, props: { title: `Target`, value: `800 ms` } },
            ],
          },
          data: {},
        },
      },
    }),
  ])
  // EXP-1323: the demo user's read marks: every thread read now but FB-9
  // (read before the agent's request landed: a bold row). FB-9 because no
  // capture opens it; opening a thread marks it read.
  await db.insert(sessionReads).values(
    [...Object.values(ids), DEMO_SESSION_ID, DEMO_REPORT_SESSION_ID].map((sessionId) => ({
      userId: demoId,
      sessionId,
      readAt: sessionId === ids.sortReports ? minutesAgo(10) : new Date(now),
    }))
  )

  // ── pull requests (all PR state lives here, never on `issues`) ─────────
  const prs = DEMO_PULL_REQUEST_IDS
  const acmePr = (number: number) => `https://github.com/acme/mobile-app/pull/${number}`
  await db.insert(pullRequests).values([
    {
      id: prs.batchLabels,
      teamId,
      rootSessionId: ids.batchLabels,
      sessionId: ids.batchLabels,
      repositoryId: repoId,
      number: 54,
      url: acmePr(54),
      title: `Batch-edit labels from the board`,
      head: `exp/APP-15`,
      base: `main`,
      state: `open`,
      createdAt: minutesAgo(99),
      updatedAt: minutesAgo(99),
    },
  ])
  await db.insert(pullRequests).values([
    // Stacked on APP-15's open PR (a native GitHub stack).
    {
      id: prs.groupByAssignee,
      teamId,
      rootSessionId: ids.groupByAssignee,
      sessionId: ids.groupByAssignee,
      repositoryId: repoId,
      number: REVIEW_PR_NUMBER,
      url: REVIEW_PR_URL,
      title: `Group board issues by assignee`,
      head: `exp/APP-14`,
      base: `exp/APP-15`,
      basePullRequestId: prs.batchLabels,
      state: `open`,
      createdAt: minutesAgo(20),
      updatedAt: minutesAgo(20),
    },
    {
      id: prs.scrollRestore,
      teamId,
      rootSessionId: ids.scrollRestore,
      sessionId: ids.scrollRestore,
      repositoryId: repoId,
      number: 55,
      url: acmePr(55),
      title: `Fix flaky scroll restore on the issue list`,
      head: `exp/APP-16`,
      base: `main`,
      state: `merged`,
      mergedAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo + 0.25),
      createdAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo + 1),
      updatedAt: hoursAgo(DEMO_CLOCK_ANCHOR.endedHoursAgo + 0.25),
    },
    // Two thread-less PRs: the merged HEIC fix and the open thumbnail one.
    {
      id: prs.heicFix,
      teamId,
      repositoryId: repoId,
      number: 41,
      url: acmePr(41),
      title: `Fix crash when uploading HEIC photos`,
      head: `exp/APP-2`,
      base: `main`,
      state: `merged`,
      mergedAt: daysAgo(1),
      createdAt: daysAgo(2),
      updatedAt: daysAgo(1),
    },
    {
      id: prs.thumbnails,
      teamId,
      repositoryId: repoId,
      number: 56,
      url: acmePr(56),
      title: `Thumbnail pipeline for faster image loading`,
      head: `exp/APP-17`,
      base: `main`,
      state: `open`,
      createdAt: daysAgo(1),
      updatedAt: daysAgo(1),
    },
  ])
  const closes = (pullRequestId: string, identifier: string) => {
    const row = issue(identifier)
    return { pullRequestId, issueId: row.id, teamId, boardId: row.boardId }
  }
  await db.insert(pullRequestIssues).values([
    closes(prs.batchLabels, `APP-15`),
    closes(prs.groupByAssignee, `APP-14`),
    closes(prs.scrollRestore, `APP-16`),
    closes(prs.heicFix, `APP-2`),
    closes(prs.thumbnails, `APP-17`),
  ])
}

async function main() {
  assertLocalDatabase(SEED_CONSEQUENCE)
  assertNotSharedDevDatabase(SEED_CONSEQUENCE)
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

  // FEED-108: a member added WITHOUT an invite (what "Add without inviting"
  // and the Linear import create): a placeholder users row, a membership and
  // the unsent invite binding it — so the `settings-members` view shows the
  // "Not invited" pill the way a real roster does. Teardown drops the team;
  // the users row goes with the orphan purge of the next seed (the ON
  // CONFLICT keeps a re-seed idempotent).
  await db
    .insert(users)
    .values({
      id: `demo-hannes`,
      name: `Hannes Gruber`,
      email: `hannes@acme.dev`,
      emailVerified: true,
      placeholderAt: daysAgo(3),
      createdAt: daysAgo(3),
      updatedAt: daysAgo(3),
    })
    .onConflictDoNothing({ target: users.email })
  const [hannes] = await db
    .select({ id: users.id })
    .from(users)
    .where(eq(users.email, `hannes@acme.dev`))
  await db.insert(teamMembers).values({
    teamId: ws.id,
    userId: hannes!.id,
    role: `member`,
    createdAt: daysAgo(3),
  })
  await db.insert(teamInvites).values({
    teamId: ws.id,
    invitedById: demoId,
    role: `member`,
    token: `screenshots-demo-invite-hannes-unsent`,
    email: `hannes@acme.dev`,
    placeholderUserId: hannes!.id,
    createdAt: daysAgo(3),
    sentAt: null,
    expiresAt: daysAgo(3),
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
    /** This PR is based on APP-<n>'s open PR branch (a stacked PR). Legacy
     *  issue mirror only: EXP-1293's `pull_requests` rows are the truth. */
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
      stackedOn: 15,
    },
    // Three more PRs (APP-15..17; EXP-1293: APP-16's merged) for the demo
    // threads' pull requests (seedSessionTree). Appended AFTER
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
    },
    // EXP-1293: its thread ended with the PR merged (the list's Recent row).
    {
      title: `Fix flaky scroll restore on the issue list`,
      status: `done`,
      priority: `high`,
      assigneeId: jonas,
      creatorId: sofia,
      labels: [`Bug`],
      createdDaysAgo: 1,
      completedDaysAgo: 0.3,
      pr: `merged`,
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

  // EXP-1259: the comment the steered session's scripted
  // `exponential_comments_create` call posted (screenshot-desktop.ts names it
  // in the settle's preview), so `comment-call-card` renders the real row.
  // On the image-cache issue, never APP-5 (its thread is a photographed view).
  await db.insert(comments).values({
    id: DEMO_AGENT_COMMENT_ID,
    issueId: cacheIssue.id,
    teamId: ws.id,
    boardId: cacheIssue.boardId,
    authorId: demoId,
    source: `mcp`,
    body: `From APP-5: the board snapshot cache now persists on \`ON_STOP\` (\`BoardSnapshotCache.persistOnStop\`), so the LRU can reuse its directory and eviction hook instead of opening a second one.\n\nCold start after the change: **740 ms** p50 on the Pixel 6a.`,
    createdAt: new Date(now - DEMO_AGENT_COMMENT_MINUTES_AGO * 60_000),
  })

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

  // The team's saved prompts (NEW-2: prompts replaced actions — a name, an
  // icon and a body, no inputs or triggers) so Settings → Prompts and the
  // composers' `/` menus list real rows. The `actions` table stays EMPTY (a
  // read shim until NEW-8). Bodies are short but plausible: the list shows
  // the name.
  const promptRows = await db
    .insert(prompts)
    .values([
      {
        teamId: ws.id,
        name: `Release train`,
        icon: `rocket`,
        body: `Collect the PRs merged since the last release tag and draft concise, user-facing release notes grouped by feature, fix, and performance. Post the draft as a comment for review.`,
        sortOrder: 1,
      },
      {
        teamId: ws.id,
        id: DEMO_ACTION_ID,
        name: `Fix flaky tests`,
        icon: `bug`,
        body: `Run the test suite three times. For every failure, decide flaky vs real: quarantine and file an issue for flaky tests, fix trivial breakage directly, and file detailed issues for anything deeper.`,
        sortOrder: 2,
      },
      {
        teamId: ws.id,
        name: `Triage new issues`,
        icon: `flag`,
        body: `Read every issue filed since yesterday, add the right labels, set a priority, link duplicates and ask the reporter for what is missing.`,
        sortOrder: 3,
      },
      {
        teamId: ws.id,
        name: `Update dependencies`,
        icon: `package`,
        body: `Update all dependencies to their latest compatible versions. Run the full test suite, fix any breakage the bumps cause, and open a PR summarizing notable upgrades.`,
        sortOrder: 4,
      },
    ])
    .returning()
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
    caps: [`agent-login`, `acp`, `agent-import`, `agent-usage-refresh`, `update-now`],
    // EXP-1306: the logins it last reported, so the agents' Account rows read
    // the login (brand mark + email) like every phone specimen does.
    agentAccounts: demoAgentReport(hoursAgo(5)).agentAccounts,
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

  // EXP-1293: the session tree (threads, a child session, agents, their
  // conversations and pull requests), then the TEMP steer-ticket row.
  const feedback = await seedFeedbackIssues({
    teamId: ws.id,
    boardId: feedbackBoard!.id,
    demoId,
    mira,
    jonas,
    sofia,
    labelIds: label,
  })
  const issue = (identifier: string) => {
    const row =
      feedback[identifier] ?? inserted.find((candidate) => candidate.identifier === identifier)
    if (!row) throw new Error(`seed has no issue ${identifier}`)
    return row
  }
  await seedSessionTree({ teamId: ws.id, demoId, jonas, repoId: repo.id, issue })
  // #EXP-1456: the Assistant's decision push (issue-less, session-routed:
  // notifyAssistantDecision), the inbox's newest row; after the tree, whose
  // Assistant conversation it names.
  await db.insert(notifications).values({
    userId: demoId,
    teamId: ws.id,
    sessionId: DEMO_ASSISTANT_CONVERSATION_ID,
    type: `agent_message`,
    title: ASSISTANT_COPY.decisionPush,
    body: DEMO_ASSISTANT_QUESTION,
    createdAt: hoursAgo(0.07),
  })

  // NEW-67 / S-3: the voice runs on the demo user's Assistant (seeded by
  // the session tree above) on the demo desktop (the server-kept mirror of
  // it), so the Assistant page's Voice section shows the tray group too.
  const voice = {
    voiceDeviceId: DEMO_DEVICE_ID,
    agentId: DEMO_ASSISTANT_ID,
    // S-3: push to talk = a bound key (the row's keycaps in the capture).
    pttShortcut: `ctrl+space`,
    wakeWord: false,
    wakePhrase: ``,
    // #NEW-73: German shows the Language row's GPU note in the capture.
    language: `de`,
    voiceFile: ``,
    voiceTranscript: ``,
  }
  await db
    .insert(userVoiceSettings)
    .values({ userId: demoId, ...voice })
    .onConflictDoUpdate({ target: userVoiceSettings.userId, set: voice })
  const live = await assertDemoLiveSessions(now)
  if (live !== DEMO_LIVE_SESSIONS.length) {
    throw new Error(`expected ${DEMO_LIVE_SESSIONS.length} live demo sessions, wrote ${live}`)
  }

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
  inbox       6 notifications (4 unread)
  threads     6 (${DEMO_THREAD_ID} = cold start, needs input via its child ${DEMO_SESSION_ID}; 3 more running, 1 idle with an open PR + Guide, 1 ended with a merged PR)
  agents      Triage (${DEMO_AGENT_ID}), Designer, Release
  machines    demo desktop (offline until screenshots:desktop beats) + 1 shared team server (offline)
  pull reqs   5 (APP-14 stacked on APP-15; APP-14 → ${REVIEW_PR_URL}, a real diff)
  prompts     ${promptRows.length} saved team prompts
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
