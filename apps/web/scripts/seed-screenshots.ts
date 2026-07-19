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
 * Usage (from apps/web, local dev env with password signup enabled):
 *   bun run seed:screenshots
 */
import { eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  comments,
  issueEvents,
  issueLabels,
  issues,
  issueSubscribers,
  labels,
  notifications,
  boards,
  repositories,
  users,
  teamMembers,
  teams,
} from "@/db/schema"
import { auth } from "@/lib/auth"

export const DEMO_EMAIL = `demo@exponential.at`
export const DEMO_PASSWORD = `screenshots-demo`
const DEMO_NAME = `Alex Carter`
const TEAM_SLUG = `acme`

const TEAMMATES = [
  { id: `demo-mira`, name: `Mira Chen`, email: `mira@acme.dev` },
  { id: `demo-jonas`, name: `Jonas Weber`, email: `jonas@acme.dev` },
  { id: `demo-sofia`, name: `Sofia Almeida`, email: `sofia@acme.dev` },
] as const

const now = Date.now()
const daysAgo = (d: number) => new Date(now - d * 86_400_000)
const hoursAgo = (h: number) => new Date(now - h * 3_600_000)
const inDays = (d: number) =>
  new Date(now + d * 86_400_000).toISOString().slice(0, 10)

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
  const emails = [DEMO_EMAIL, ...TEAMMATES.map((m) => m.email)]
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
  const mates = await ensureTeammates()
  const mira = mates[`demo-mira`]
  const jonas = mates[`demo-jonas`]
  const sofia = mates[`demo-sofia`]

  const [ws] = await db
    .insert(teams)
    .values({ name: `Acme`, slug: TEAM_SLUG })
    .returning()

  await db.insert(teamMembers).values([
    { teamId: ws.id, userId: demoId, role: `owner` },
    { teamId: ws.id, userId: mira, role: `member` },
    { teamId: ws.id, userId: jonas, role: `member` },
    { teamId: ws.id, userId: sofia, role: `member` },
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
      slug: `launch-marketing`,
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
    status: `backlog` | `todo` | `in_progress` | `done`
    priority: `none` | `urgent` | `high` | `medium` | `low`
    assigneeId?: string
    creatorId: string
    dueDate?: string
    labels?: string[]
    createdDaysAgo: number
    completedDaysAgo?: number
    pr?: boolean
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
      pr: true,
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
    {
      title: `Migrate image cache to on-disk LRU`,
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
      status: `backlog`,
      priority: `medium`,
      creatorId: sofia,
      labels: [`Design`],
      createdDaysAgo: 10,
    },
  ]

  const inserted: Array<typeof issues.$inferSelect> = []
  for (const [i, spec] of seedIssues.entries()) {
    const [row] = await db
      .insert(issues)
      .values({
        boardId: board.id,
        title: spec.title,
        description: spec.description,
        status: spec.status,
        priority: spec.priority,
        assigneeId: spec.assigneeId,
        creatorId: spec.creatorId,
        dueDate: spec.dueDate,
        sortOrder: i * 10,
        createdAt: daysAgo(spec.createdDaysAgo),
        completedAt:
          spec.completedDaysAgo === undefined
            ? undefined
            : daysAgo(spec.completedDaysAgo),
        ...(spec.pr
          ? {
              prUrl: `https://github.com/acme/mobile-app/pull/42`,
              prNumber: 42,
              prState: `merged` as const,
              branch: `exp/APP-2`,
              prMergedAt: daysAgo(1),
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

  console.log(`
Seeded screenshot demo data:
  team   ${ws.name} (/${ws.slug})
  board     ${board.name} (APP), ${inserted.length} issues
  login       ${DEMO_EMAIL} / ${DEMO_PASSWORD}
  showcase    ${showcase.identifier ?? `APP-5`} (markdown + ${3} comments)
  inbox       5 notifications (3 unread)
`)
  process.exit(0)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
