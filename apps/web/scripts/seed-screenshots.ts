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
 * always look fresh. Recreating the users (not just the team) matters:
 * it rotates the user id and with it the identity of the user-scoped
 * Electric shapes (notifications). The vite dev bridge strips the
 * electric-handle/electric-offset response headers from the shape proxies,
 * so clients in local dev can never follow a shape log past its snapshot —
 * a reused shape would serve the previous seed generation forever. Fresh
 * ids on every entity ⇒ fresh shapes ⇒ fresh snapshots.
 *
 * Seeding is only half of it: three of the eight shots (Start-coding dialog,
 * live steering, the "Coding now" row) need a steer relay with a desktop online
 * on it, which `screenshot-desktop.ts` provides — run that next and leave it
 * running for the capture.
 *
 * Usage (from apps/web, local dev env with password signup enabled):
 *   bun run seed:screenshots
 *   bun run screenshots:desktop   # second shell, stays up during the capture
 */
import { eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  actions,
  attachments,
  automations,
  codingSessions,
  comments,
  devices,
  issueEvents,
  issueLabels,
  issues,
  issueStatuses,
  issueSubscribers,
  labels,
  notifications,
  boards,
  repositories,
  supportMessages,
  supportThreads,
  teamInvites,
  users,
  teamMembers,
  teams,
  widgetConfigs,
} from "@/db/schema"
import { auth } from "@/lib/auth"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
} from "@/lib/storage/issue-attachments"
import { generateWidgetKey } from "@/lib/widget/key"
import { parseFreezeNow } from "./lib/freeze-now"
import {
  DEMO_DEVICE_ID,
  DEMO_SERVER_DEVICE_ID,
  DEMO_DEVICE_LABEL,
  DEMO_EMAIL,
  DEMO_INVITE_TOKEN,
  DEMO_PENDING_INVITE_EXPIRY,
  DEMO_NAME,
  DEMO_PASSWORD,
  DEMO_SERVER_VERSION,
  EMPTY_BOARD_SLUG,
  NEWCOMER_EMAIL,
  NEWCOMER_NAME,
  NEWCOMER_PASSWORD,
  SUPPORT_REPORTER_THREAD_TITLE,
  TEAM_SLUG,
} from "./screenshot-demo"

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

// Opt-in frozen clock (SCREENSHOT_FREEZE_NOW) — see lib/freeze-now.ts for why
// the capture pipeline deliberately does NOT set it.
const frozenNow = parseFreezeNow(process.env.SCREENSHOT_FREEZE_NOW)
if (frozenNow !== undefined) {
  console.log(`clock frozen at ${new Date(frozenNow).toISOString()} (SCREENSHOT_FREEZE_NOW)`)
}
const now = frozenNow ?? Date.now()
const daysAgo = (d: number) => new Date(now - d * 86_400_000)
const hoursAgo = (h: number) => new Date(now - h * 3_600_000)
const inDays = (d: number) =>
  new Date(now + d * 86_400_000).toISOString().slice(0, 10)

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

async function ensureDemoUser(): Promise<string> {
  await auth.api.signUpEmail({
    body: { name: DEMO_NAME, email: DEMO_EMAIL, password: DEMO_PASSWORD },
  })
  const [row] = await db
    .select()
    .from(users)
    .where(eq(users.email, DEMO_EMAIL))
    .limit(1)
  if (!row) throw new Error(`demo user missing after signUpEmail`)
  // Verified + onboarded so the apps go straight to the main UI.
  await db
    .update(users)
    .set({ emailVerified: true, onboardingCompletedAt: daysAgo(30) })
    .where(eq(users.id, row.id))
  return row.id
}

/**
 * The team-less second identity (EXP-566). Verified so the app lets it in, but
 * `onboardingCompletedAt` stays NULL and it joins nothing — that is precisely
 * what makes `/onboarding` and `/invite/$token` render instead of redirecting.
 *
 * `signUpEmail` gives every new user a personal team on some instances; this
 * strips whatever it created so the account really does own nothing.
 */
async function ensureNewcomerUser(): Promise<string> {
  await auth.api.signUpEmail({
    body: {
      name: NEWCOMER_NAME,
      email: NEWCOMER_EMAIL,
      password: NEWCOMER_PASSWORD,
    },
  })
  const [row] = await db
    .select()
    .from(users)
    .where(eq(users.email, NEWCOMER_EMAIL))
    .limit(1)
  if (!row) throw new Error(`newcomer user missing after signUpEmail`)
  await db
    .update(users)
    .set({ emailVerified: true, onboardingCompletedAt: null })
    .where(eq(users.id, row.id))
  const owned = await db
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(eq(teamMembers.userId, row.id))
  for (const { teamId } of owned) {
    await db.delete(boards).where(eq(boards.teamId, teamId))
    await db.delete(teams).where(eq(teams.id, teamId))
  }
  return row.id
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

  // Recreate the demo users each run (fresh ids ⇒ fresh user-scoped shapes —
  // see the header). Drop teams where a demo user is the sole member
  // first (their auto-created personal teams would otherwise pile up).
  const emails = [DEMO_EMAIL, NEWCOMER_EMAIL, ...TEAMMATES.map((m) => m.email)]
  const demoUsers = await db
    .select({ id: users.id })
    .from(users)
    .where(inArray(users.email, emails))
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

async function main() {
  await teardown()
  const demoId = await ensureDemoUser()
  await ensureNewcomerUser()
  const mates = await ensureTeammates()
  const mira = mates[`demo-mira`]
  const jonas = mates[`demo-jonas`]
  const sofia = mates[`demo-sofia`]

  // helpdeskEnabled unlocks the Support tab on every client — the support
  // inbox screenshot needs it.
  const [ws] = await db
    .insert(teams)
    .values({ name: `Acme`, slug: TEAM_SLUG, helpdeskEnabled: true })
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
      expiresAt: DEMO_PENDING_INVITE_EXPIRY.mailed,
    },
    {
      teamId: ws.id,
      invitedById: mira,
      role: `member`,
      token: `screenshots-demo-invite-link`,
      createdAt: hoursAgo(20),
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
  // picks the first board by sortOrder).
  await db.insert(boards).values([
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
  ])

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
      archivedAt: daysAgo(9),
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
    status: `backlog` | `todo` | `in_progress` | `in_review` | `done`
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
      dueDate: inDays(2),
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
      description: `Startup profiling shows most of the time goes into the sync bootstrap, not rendering.\n\n- [x] Profile app launch end-to-end\n- [x] Defer shape subscribe until after first frame\n- [ ] Lazy-load the markdown editor\n- [ ] Cache the last board snapshot for instant paint\n\nTarget is \`<800ms\` cold on a mid-range device.`,
      status: `in_progress`,
      priority: `urgent`,
      assigneeId: jonas,
      creatorId: demoId,
      dueDate: inDays(4),
      labels: [`Performance`],
      createdDaysAgo: 3,
    },
    {
      title: `Push notification deep links open the wrong tab`,
      description: `Tapping a comment push lands on the board instead of the issue. Only happens when the app was fully killed.`,
      status: `todo`,
      priority: `urgent`,
      assigneeId: demoId,
      creatorId: mira,
      dueDate: inDays(1),
      labels: [`Bug`],
      createdDaysAgo: 2,
    },
    {
      title: `Add drag-and-drop reordering on the board`,
      status: `todo`,
      priority: `high`,
      assigneeId: mira,
      creatorId: demoId,
      labels: [`Feature`],
      createdDaysAgo: 8,
    },
    {
      title: `Improve empty states with illustrations`,
      status: `todo`,
      priority: `medium`,
      assigneeId: sofia,
      creatorId: demoId,
      dueDate: inDays(7),
      labels: [`Design`],
      createdDaysAgo: 7,
    },
    {
      title: `Offline queue for issue edits`,
      status: `todo`,
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

  // Showcase issue APP-5: comments + activity + subscribers for the
  // issue-detail and comments screenshots.
  const showcase = inserted[4]
  await db.insert(comments).values([
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      authorId: mira,
      body: `Profiled on a mid-range device — the shape subscribe alone is **410 ms**. Deferring it until after first frame gets us to ~750 ms cold.`,
      createdAt: hoursAgo(26),
    },
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      authorId: jonas,
      body: `Nice find. I'll take the board snapshot cache — we can reuse the reducer state and paint before sync finishes.`,
      createdAt: hoursAgo(22),
    },
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      authorId: demoId,
      body: `Deferral PR is merged. CI numbers:\n\n- cold start: ~1.4s → **860 ms**\n- warm start: unchanged\n\nSnapshot cache should get us under target.`,
      createdAt: hoursAgo(5),
    },
    // @mention + #issue ref so the comments screenshot shows both pill types
    // (interchange forms: plain `@<email>` and `#<IDENTIFIER>` GFM text).
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      authorId: sofia,
      body: `@${TEAMMATES[1].email} once #APP-2 is verified on device, can you re-run the profiling? The HEIC fix might shave a little more off the cold start.`,
      createdAt: hoursAgo(2),
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
      actorUserId: demoId,
      type: `status_changed`,
      payload: { from: `backlog`, to: `todo` },
      createdAt: daysAgo(3),
    },
    {
      issueId: showcase.id,
      teamId: ws.id,
      boardId: showcase.boardId,
      actorUserId: jonas,
      type: `status_changed`,
      payload: { from: `todo`, to: `in_progress` },
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
    createdDaysAgo: number
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
      createdDaysAgo: 14,
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
      createdDaysAgo: 9,
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
      createdDaysAgo: 4,
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
      createdDaysAgo: 3,
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
      createdDaysAgo: 10,
    },
    {
      id: ATTACHMENT_IDS.sysdiagnose,
      issueId: inserted[1].id,
      uploaderId: jonas,
      filename: `sysdiagnose-heic-upload.zip`,
      contentType: `application/zip`,
      sizeBytes: 3_874_112,
      createdDaysAgo: 6,
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
      createdAt: daysAgo(spec.createdDaysAgo),
    }))
  )

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
  const reviewIssue = inserted[13]
  await db.insert(codingSessions).values([
    {
      issueId: showcase.id,
      teamId: ws.id,
      userId: demoId,
      deviceLabel: DEMO_DEVICE_LABEL,
      status: `running`,
      startedAt: hoursAgo(1),
    },
    {
      issueId: inserted[3].id,
      teamId: ws.id,
      userId: mira,
      deviceLabel: `Mira's Mac mini`,
      status: `running`,
      startedAt: new Date(now - 20 * 60_000),
    },
    {
      issueId: reviewIssue.id,
      teamId: ws.id,
      userId: demoId,
      deviceLabel: DEMO_DEVICE_LABEL,
      status: `in_review`,
      startedAt: hoursAgo(3),
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

  // Automations (EXP-583) for the `automations` view — without rows it
  // photographs a blank "New automation" dialog over "No automations yet."
  // They are their own entity, never a field on the action: a when-part
  // (`trigger` jsonb — schedule or issue event, typed in
  // @exp/db-schema domain.ts) plus the runner binding, which is the MACHINE's
  // steer deviceId, not a row uuid. minuteOfDay is device-local wall clock by
  // design (no tz field). One disabled row so the toggle has an off state to
  // render.
  const automationRows = await db
    .insert(automations)
    .values([
      {
        teamId: ws.id,
        actionId: action[`Nightly test triage`].id,
        deviceId: DEMO_DEVICE_ID,
        trigger: { kind: `schedule`, interval: `daily`, minuteOfDay: 180 },
        agent: `claude`,
        sortOrder: 0,
        createdAt: daysAgo(21),
      },
      {
        teamId: ws.id,
        actionId: action[`Update dependencies`].id,
        deviceId: DEMO_DEVICE_ID,
        trigger: {
          kind: `schedule`,
          interval: `weekly`,
          weekday: 1,
          minuteOfDay: 540,
        },
        agent: `codex`,
        sortOrder: 10,
        createdAt: daysAgo(18),
      },
      {
        teamId: ws.id,
        actionId: action[`Draft release notes`].id,
        deviceId: DEMO_DEVICE_ID,
        enabled: false,
        trigger: { kind: `event`, event: `pr_merged` },
        sortOrder: 20,
        createdAt: daysAgo(6),
      },
    ])
    .returning()

  // Two finished automated runs so the tab's "Recent automated runs" section —
  // and each row's "last run" line — say something other than "Nothing has
  // fired yet." Action-scoped sessions carry the action id plus a display-name
  // snapshot (actions are server-only, clients can't join), `started_reason`
  // for the run-history filter and `automation_id` for the per-row last-run
  // lookup. Ended, so they never join the live agents list.
  //
  // EXP-663: both rows are a full EXP-637 close-out — `ended_by: 'agent'` plus
  // an outcome and the summary the agent wrote. That trio is what every
  // client's "Recent runs" section gates on, and the two outcomes differ on
  // purpose so a capture photographs both the done tint and the blocked glyph.
  await db.insert(codingSessions).values([
    {
      teamId: ws.id,
      userId: demoId,
      actionId: action[`Nightly test triage`].id,
      actionName: `Nightly test triage`,
      automationId: automationRows[0].id,
      startedReason: `schedule`,
      deviceId: DEMO_DEVICE_ID,
      deviceLabel: DEMO_DEVICE_LABEL,
      status: `ended`,
      endedBy: `agent`,
      outcome: `done`,
      summary: `Triaged 4 failing specs: 3 flaky (retry added), 1 real regression filed as APP-31.`,
      startedAt: hoursAgo(9),
      endedAt: hoursAgo(8),
    },
    {
      teamId: ws.id,
      userId: demoId,
      actionId: action[`Update dependencies`].id,
      actionName: `Update dependencies`,
      automationId: automationRows[1].id,
      startedReason: `schedule`,
      deviceId: DEMO_DEVICE_ID,
      deviceLabel: DEMO_DEVICE_LABEL,
      status: `ended`,
      endedBy: `agent`,
      outcome: `blocked`,
      summary: `Bumped 12 packages; react-day-picker 9.x needs a manual API migration in due-date-picker.tsx before this can merge.`,
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
  // desktop row is registered by screenshots:desktop, which owns its version
  // and default-machine flags.
  await db.insert(devices).values({
    userId: jonas,
    deviceId: DEMO_SERVER_DEVICE_ID,
    label: `Acme build server`,
    kind: `server`,
    platform: `linux`,
    version: DEMO_SERVER_VERSION,
    agents: [`claude`, `codex`],
    caps: [`actions`, `action-inputs`, `fix-conflicts`, `chat`, `automations`],
    sharedTeamId: ws.id,
    lastSeenAt: hoursAgo(5),
    createdAt: daysAgo(45),
  })

  // Helpdesk tickets for the support-inbox screenshot (server-only tRPC —
  // no Electric shape involved). A trailing inbound message marks the
  // thread unread; explicit updatedAt controls the list order.
  const seedThreads: Array<{
    title: string
    reporterName: string
    reporterEmail: string
    messages: Array<{
      direction: `inbound` | `outbound`
      authorUserId?: string
      body: string
      hoursAgo: number
    }>
  }> = [
    {
      title: SUPPORT_REPORTER_THREAD_TITLE,
      reporterName: `Emma Fischer`,
      reporterEmail: `emma@lumenlabs.io`,
      messages: [
        {
          direction: `inbound`,
          body: `After the last update the sign-in button just spins forever on my iPad. Works fine on my phone.`,
          hoursAgo: 26,
        },
        {
          direction: `outbound`,
          authorUserId: sofia,
          body: `Thanks for the report! We found a token refresh bug on iPadOS and just shipped a fix — could you update to 2.4.1 and try again?`,
          hoursAgo: 20,
        },
        {
          direction: `inbound`,
          body: `That fixed it — thank you for the quick turnaround!`,
          hoursAgo: 1,
        },
      ],
    },
    {
      title: `How do I export my issues?`,
      reporterName: `Liam O'Connor`,
      reporterEmail: `liam@brightpath.app`,
      messages: [
        {
          direction: `inbound`,
          body: `We're migrating our workflow docs and I'd love to pull everything out as CSV. Is that possible?`,
          hoursAgo: 8,
        },
      ],
    },
    {
      title: `Weekly summary email for the whole team`,
      reporterName: `Priya Nair`,
      reporterEmail: `priya@northwind.dev`,
      messages: [
        {
          direction: `inbound`,
          body: `A Monday-morning digest of what shipped last week would be amazing for our standups.`,
          hoursAgo: 49,
        },
        {
          direction: `outbound`,
          authorUserId: demoId,
          body: `Love this idea — I've filed it on our roadmap and linked this ticket so you'll hear back when it ships.`,
          hoursAgo: 44,
        },
      ],
    },
    {
      title: `Screenshot upload stuck at 99%`,
      reporterName: `Tom Berger`,
      reporterEmail: `tom@fieldworks.co`,
      messages: [
        {
          direction: `inbound`,
          body: `Attaching a screenshot from the feedback widget hangs at 99% on our office network. Smaller images go through fine.`,
          hoursAgo: 4,
        },
      ],
    },
    {
      title: `Does the widget support German?`,
      reporterName: `Anna Keller`,
      reporterEmail: `anna@studiokeller.de`,
      messages: [
        {
          direction: `inbound`,
          body: `Our customers write in German — can the feedback form labels be localized?`,
          hoursAgo: 30,
        },
        {
          direction: `outbound`,
          authorUserId: sofia,
          body: `Yes! You can override every label in the embed config — I've sent over a snippet with German defaults.`,
          hoursAgo: 27,
        },
      ],
    },
  ]
  for (const spec of seedThreads) {
    const last = spec.messages[spec.messages.length - 1]
    const [thread] = await db
      .insert(supportThreads)
      .values({
        teamId: ws.id,
        title: spec.title,
        status: `open`,
        reporterName: spec.reporterName,
        reporterEmail: spec.reporterEmail,
        createdAt: hoursAgo(spec.messages[0].hoursAgo),
        updatedAt: hoursAgo(last.hoursAgo),
      })
      .returning()
    await db.insert(supportMessages).values(
      spec.messages.map((m) => ({
        threadId: thread.id,
        direction: m.direction,
        visibility: `public` as const,
        authorUserId: m.authorUserId,
        body: m.body,
        createdAt: hoursAgo(m.hoursAgo),
        updatedAt: hoursAgo(m.hoursAgo),
      }))
    )
  }

  // Embeddable widget configs for the `settings-widget` view ("No widgets
  // yet." without them). One full config — both modes, a domain allowlist, two
  // of the seeded labels for the reporter to tag with, a theme — and one
  // support-only config, which is the case that legitimately has no board
  // (support files a standalone ticket, feedback needs somewhere to file an
  // issue). Keys are minted the way the router mints them.
  await db.insert(widgetConfigs).values([
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
        position: `bottom-right`,
        collectEmail: true,
        collectName: true,
        modes: [`feedback`, `support`],
        labelIds: [label[`Bug`], label[`Feature`]],
        theme: `auto`,
      },
    },
    {
      teamId: ws.id,
      name: `Help center`,
      publicKey: generateWidgetKey(),
      allowedDomains: [`help.acme.dev`],
      createdByUserId: demoId,
      createdAt: daysAgo(8),
      formConfig: {
        buttonLabel: `Contact support`,
        accentColor: `#22c55e`,
        collectEmail: true,
        emailRequired: true,
        modes: [`support`],
        theme: `dark`,
      },
    },
  ])

  // Personal API keys for the `settings-api-keys` view ("No API keys yet."
  // without them). Minted through Better Auth exactly like
  // `users.mintPersonalApiKey` does, so the rows are genuine `expu_`
  // credentials (only their hash is stored) rather than hand-written rows —
  // the page lists name, prefix and last use.
  for (const name of [DEMO_DEVICE_LABEL, `Claude Code (MCP)`]) {
    await auth.api.createApiKey({
      body: {
        name,
        userId: demoId,
        expiresIn: null,
        rateLimitEnabled: false,
        metadata: { kind: `personal` },
      },
    })
  }

  console.log(`
Seeded screenshot demo data:
  team        ${ws.name} (/${ws.slug})
  board       ${board.name} (APP), ${inserted.length} issues (1 on custom status "In QA", 1 duplicate)
  boards      3 live + 1 archived + 1 trashed (48h pending deletion)
  login       ${DEMO_EMAIL} / ${DEMO_PASSWORD}
  newcomer    ${NEWCOMER_EMAIL} / ${NEWCOMER_PASSWORD} (no team — onboarding + invite views)
  invite      /invite/${DEMO_INVITE_TOKEN} (+ 2 more pending invites)
  showcase    ${showcase.identifier ?? `APP-5`} (markdown + ${4} comments incl. @mention + #issue ref)
  inbox       5 notifications (3 unread)
  agents      3 coding sessions (2 running + 1 in review)
  machines    1 shared team server (offline; the desktop registers itself)
  reviews     4 open pull requests
  review shot APP-14 → ${REVIEW_PR_URL} (real diff, fetched from GitHub)
  actions     ${actionRows.length} saved team actions
  automations ${automationRows.length} (2 scheduled + 1 event, 1 disabled) + 2 automated runs
  storage     ${seedAttachments.length} attachments (1 unreferenced image to sweep)
  widgets     2 widget configs (feedback+support, support-only)
  api keys    2 personal keys
  support     ${seedThreads.length} helpdesk threads

Next: bun run screenshots:desktop (needs STEER_RELAY_URL + STEER_RELAY_SECRET)
`)
  process.exit(0)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
