import { randomUUID } from "node:crypto"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { router, publicProcedure, generateTxId } from "@/lib/trpc"
import {
  codingSessions,
  comments,
  issueLabels,
  issues,
  issueSubscribers,
  labels,
  projects,
  users,
  widgetConfigs,
  widgetSubmissions,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import {
  clientIpFromRequest,
  envInt,
  TokenBucketLimiter,
} from "@/lib/widget/rate-limit"
import {
  appBaseUrl,
  buildIssueDeepLinkPath,
} from "@/lib/notification-email-policy"

// Read-only public surface of feedback boards, served over tRPC. This is what
// the web app renders for every NON-member visitor — anonymous or signed-in.
// (A signed-in non-member's Electric shapes are membership-scoped and would
// deliver nothing for this workspace, so the public view cannot come from
// sync; anonymous shape scoping still exists as defense-in-depth and for the
// logged-out live path.) Field discipline mirrors the anonymous shape
// allowlists exactly: no pr_url / pr_number / branch (private-repo identity),
// user fields are bare ids (clients render the deterministic "Member XXXX"
// anonymous handle — the users table never leaves the server here).

const ISSUE_COLUMNS = {
  id: issues.id,
  identifier: issues.identifier,
  // EXP-38: the canonical in-group comparator's final tie-break sorts by issue
  // `number` numerically (never the identifier string).
  number: issues.number,
  title: issues.title,
  description: issues.description,
  status: issues.status,
  priority: issues.priority,
  assigneeId: issues.assigneeId,
  creatorId: issues.creatorId,
  dueDate: issues.dueDate,
  sortOrder: issues.sortOrder,
  completedAt: issues.completedAt,
  prState: issues.prState,
  prMergedAt: issues.prMergedAt,
  duplicateOfId: issues.duplicateOfId,
  createdAt: issues.createdAt,
  updatedAt: issues.updatedAt,
}

async function resolvePublicProject(
  db: {
    select: typeof import("@/db/connection").db.select
  },
  workspaceSlug: string,
  projectSlug: string
) {
  const [row] = await db
    .select({
      workspaceId: workspaces.id,
      workspaceName: workspaces.name,
      workspaceSlug: workspaces.slug,
      projectId: projects.id,
      projectName: projects.name,
      projectSlug: projects.slug,
      prefix: projects.prefix,
      color: projects.color,
      type: projects.type,
      publicShowComments: projects.publicShowComments,
      publicShowActivity: projects.publicShowActivity,
      publicShowCoding: projects.publicShowCoding,
    })
    .from(projects)
    .innerJoin(workspaces, eq(workspaces.id, projects.workspaceId))
    .where(
      and(
        eq(workspaces.slug, workspaceSlug),
        eq(projects.slug, projectSlug),
        eq(projects.type, `feedback`),
        isNull(projects.archivedAt),
        isNull(projects.deletedAt)
      )
    )
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND` })
  }
  return row
}

type Tx = Parameters<
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

// In-process token buckets for the anonymous create path — same per-replica
// stance as the widget submit endpoint (single Coolify instance). Env-tunable.
let createIpLimiter: TokenBucketLimiter | null = null
let createProjectLimiter: TokenBucketLimiter | null = null

function getCreateIssueLimiters() {
  createIpLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`PUBLIC_BOARD_RATE_LIMIT_IP_BURST`, 5),
    refillPerHour: envInt(`PUBLIC_BOARD_RATE_LIMIT_PER_IP_HOURLY`, 30),
  })
  createProjectLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`PUBLIC_BOARD_RATE_LIMIT_PROJECT_BURST`, 10),
    refillPerHour: envInt(`PUBLIC_BOARD_RATE_LIMIT_PER_PROJECT_HOURLY`, 60),
  })
  return { createIpLimiter, createProjectLimiter }
}

// EXP-42a URL contract: absolute public issue URL on the app origin. The
// board passed resolvePublicProject, so it is a live public feedback board.
// Segments go through buildIssueDeepLinkPath (per-segment encoding): legacy
// project prefixes predate the letter-led-alphanumeric floor, so identifiers
// can contain characters like `#` that would otherwise truncate the path.
function publicIssueUrl(
  workspaceSlug: string,
  projectSlug: string,
  identifier: string
): string {
  return `${appBaseUrl()}${buildIssueDeepLinkPath({ workspaceSlug, projectSlug, identifier })}`
}

// Creator identity for public-board submissions: reuse the workspace's widget
// bot when a widget config exists; otherwise get-or-create ONE per-workspace
// feedback bot. The deterministic email keys an idempotent select-then-insert
// (users.email is unique, so a concurrent create conflicts instead of
// duplicating). Like widget users (lib/widget/widget-user.ts), the bot is
// NEVER deletable — issues.creator_id cascades on user delete.
async function resolvePublicReporterUserId(
  tx: Tx,
  workspaceId: string
): Promise<string> {
  const [config] = await tx
    .select({ widgetUserId: widgetConfigs.widgetUserId })
    .from(widgetConfigs)
    .where(eq(widgetConfigs.workspaceId, workspaceId))
    .orderBy(asc(widgetConfigs.createdAt))
    .limit(1)
  if (config) return config.widgetUserId

  const botEmail = `feedback-bot-${workspaceId}@exponential.local`
  const [existing] = await tx
    .select({ id: users.id })
    .from(users)
    .where(eq(users.email, botEmail))
    .limit(1)
  let botId = existing?.id
  if (!botId) {
    botId = randomUUID()
    const now = new Date()
    await tx
      .insert(users)
      .values({
        id: botId,
        name: `Public board`,
        email: botEmail,
        emailVerified: true,
        image: null,
        isAdmin: false,
        // Keeps the bot out of subscriptions/notifications/@-mentions and
        // seat counts, while a plain membership (below) lets clients resolve
        // its display name.
        isAgent: true,
        createdAt: now,
        updatedAt: now,
      })
      .onConflictDoNothing()
    // Lost a race on the unique email — adopt the winner's row.
    const [row] = await tx
      .select({ id: users.id })
      .from(users)
      .where(eq(users.email, botEmail))
      .limit(1)
    botId = row?.id ?? botId
  }

  await tx
    .insert(workspaceMembers)
    .values({ workspaceId, userId: botId, role: `member` })
    .onConflictDoNothing()

  return botId
}

export const publicBoardRouter = router({
  // The public feedback boards a workspace hosts (name/slug only). Lets the
  // bare /w/$slug URL resolve to the board without exposing sibling projects.
  boards: publicProcedure
    .input(z.object({ workspaceSlug: z.string().min(1).max(255) }))
    .query(async ({ ctx, input }) => {
      return await ctx.db
        .select({
          projectSlug: projects.slug,
          projectName: projects.name,
        })
        .from(projects)
        .innerJoin(workspaces, eq(workspaces.id, projects.workspaceId))
        .where(
          and(
            eq(workspaces.slug, input.workspaceSlug),
            eq(projects.type, `feedback`),
            isNull(projects.archivedAt),
            isNull(projects.deletedAt)
          )
        )
        .orderBy(asc(projects.sortOrder))
    }),

  // Board page: the public project + its non-archived issues + the labels in
  // use. One shot, no sync — visitors refetch on navigation.
  board: publicProcedure
    .input(
      z.object({
        workspaceSlug: z.string().min(1).max(255),
        projectSlug: z.string().min(1).max(255),
      })
    )
    .query(async ({ ctx, input }) => {
      const board = await resolvePublicProject(
        ctx.db,
        input.workspaceSlug,
        input.projectSlug
      )

      const issueRows = await ctx.db
        .select(ISSUE_COLUMNS)
        .from(issues)
        .where(
          and(eq(issues.projectId, board.projectId), isNull(issues.archivedAt))
        )
        .orderBy(asc(issues.sortOrder))

      const labelLinks = await ctx.db
        .select({
          issueId: issueLabels.issueId,
          labelId: issueLabels.labelId,
          name: labels.name,
          color: labels.color,
        })
        .from(issueLabels)
        .innerJoin(labels, eq(labels.id, issueLabels.labelId))
        .where(eq(issueLabels.projectId, board.projectId))

      return { board, issues: issueRows, labelLinks }
    }),

  // Issue page: one public issue (+ comments when the board shows them).
  issue: publicProcedure
    .input(
      z.object({
        workspaceSlug: z.string().min(1).max(255),
        projectSlug: z.string().min(1).max(255),
        identifier: z.string().min(1).max(20),
      })
    )
    .query(async ({ ctx, input }) => {
      const board = await resolvePublicProject(
        ctx.db,
        input.workspaceSlug,
        input.projectSlug
      )

      const [issue] = await ctx.db
        .select(ISSUE_COLUMNS)
        .from(issues)
        .where(
          and(
            eq(issues.projectId, board.projectId),
            eq(issues.identifier, input.identifier),
            isNull(issues.archivedAt)
          )
        )
        .limit(1)
      if (!issue) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }

      const labelRows = await ctx.db
        .select({ id: labels.id, name: labels.name, color: labels.color })
        .from(issueLabels)
        .innerJoin(labels, eq(labels.id, issueLabels.labelId))
        .where(eq(issueLabels.issueId, issue.id))

      const commentRows = board.publicShowComments
        ? await ctx.db
            .select({
              id: comments.id,
              authorId: comments.authorId,
              body: comments.body,
              createdAt: comments.createdAt,
              editedAt: comments.editedAt,
            })
            .from(comments)
            .where(eq(comments.issueId, issue.id))
            .orderBy(asc(comments.createdAt))
        : []

      // The public "coding now" surface, per the board's opt-in level: `badge`
      // exposes only that a session runs (+ device label); `live` additionally
      // lets the client mint a public activity ticket for it.
      let codingSession: {
        id: string
        deviceLabel: string | null
        startedAt: Date
        live: boolean
      } | null = null
      if (board.publicShowCoding !== `off`) {
        const [running] = await ctx.db
          .select({
            id: codingSessions.id,
            deviceLabel: codingSessions.deviceLabel,
            startedAt: codingSessions.startedAt,
          })
          .from(codingSessions)
          .where(
            and(
              eq(codingSessions.issueId, issue.id),
              eq(codingSessions.status, `running`)
            )
          )
          .orderBy(asc(codingSessions.startedAt))
          .limit(1)
        if (running) {
          codingSession = {
            ...running,
            live: board.publicShowCoding === `live`,
          }
        }
      }

      return {
        board,
        issue,
        labels: labelRows,
        comments: commentRows,
        codingSession,
      }
    }),

  // Duplicate-target lookup so the public issue view can label "duplicate of
  // #EXP-n" without syncing anything.
  issueRefs: publicProcedure
    .input(
      z.object({
        workspaceSlug: z.string().min(1).max(255),
        projectSlug: z.string().min(1).max(255),
        issueIds: z.array(z.string().uuid()).max(50),
      })
    )
    .query(async ({ ctx, input }) => {
      if (input.issueIds.length === 0) return []
      const board = await resolvePublicProject(
        ctx.db,
        input.workspaceSlug,
        input.projectSlug
      )
      return await ctx.db
        .select({
          id: issues.id,
          identifier: issues.identifier,
          title: issues.title,
        })
        .from(issues)
        .where(
          and(
            eq(issues.projectId, board.projectId),
            inArray(issues.id, input.issueIds)
          )
        )
    }),

  // EXP-42c: the ONLY public write on this router — "Create issue" on a
  // public feedback board, for anonymous and signed-in non-member visitors
  // alike. Deliberately narrow (no status/priority/label inputs — spam
  // surface) and mirrors the widget submit pipeline: honeypot, in-process
  // rate limits, synthetic bot creator, widget_reporter subscriber + a
  // widget_submissions row (widgetConfigId null) so the resolution-email
  // flow and the members-only "Reported via widget" card work identically.
  // No notification fan-out — public-board triage is pull-based.
  createIssue: publicProcedure
    .input(
      z.object({
        workspaceSlug: z.string().min(1).max(255),
        projectSlug: z.string().min(1).max(255),
        title: z.string().trim().min(1).max(500),
        description: z.string().max(10_000).default(``),
        email: z
          .string()
          .trim()
          .email()
          .max(320)
          .optional()
          .or(z.literal(``).transform(() => undefined)),
        // Honeypot — real users never see or fill this field.
        website: z.string().max(1024).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const board = await resolvePublicProject(
        ctx.db,
        input.workspaceSlug,
        input.projectSlug
      )

      // Honeypot: pretend success (shape-identical, plausible identifier) so
      // bots don't adapt; nothing is created.
      if (input.website && input.website.length > 0) {
        const fake = `${board.prefix}-${Math.floor(Math.random() * 900) + 100}`
        return {
          identifier: fake,
          url: publicIssueUrl(board.workspaceSlug, board.projectSlug, fake),
        }
      }

      const { createIpLimiter, createProjectLimiter } = getCreateIssueLimiters()
      const ipLimit = createIpLimiter.tryTake(
        `ip:${clientIpFromRequest(ctx.request)}`
      )
      const projectLimit = createProjectLimiter.tryTake(
        `project:${board.projectId}`
      )
      if (!ipLimit.ok || !projectLimit.ok) {
        throw new TRPCError({
          code: `TOO_MANY_REQUESTS`,
          message: `Too many submissions, try again later`,
        })
      }

      // EXP-42b applies here too: the description is the visitor's text only,
      // no metadata block.
      const description = input.description.trim()

      const issue = await ctx.db.transaction(async (tx) => {
        await generateTxId(tx)
        const creatorId = await resolvePublicReporterUserId(
          tx,
          board.workspaceId
        )

        const [created] = await tx
          .insert(issues)
          .values({
            projectId: board.projectId,
            title: input.title,
            status: `backlog`,
            priority: `none`,
            description: description || null,
            creatorId,
          })
          .returning({ id: issues.id, identifier: issues.identifier })

        // One-way helpdesk: the reporter gets the clean resolution email when
        // the issue closes; member fan-out ignores null-userId rows.
        if (input.email) {
          await tx.insert(issueSubscribers).values({
            issueId: created.id,
            userId: null,
            email: input.email,
            workspaceId: board.workspaceId,
            projectId: board.projectId,
            source: `widget_reporter`,
            unsubscribed: false,
          })
        }

        await tx.insert(widgetSubmissions).values({
          widgetConfigId: null,
          issueId: created.id,
          reporterEmail: input.email ?? null,
        })

        return created
      })

      return {
        identifier: issue.identifier,
        url: publicIssueUrl(
          board.workspaceSlug,
          board.projectSlug,
          issue.identifier
        ),
      }
    }),
})
