import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { router, publicProcedure } from "@/lib/trpc"
import {
  codingSessions,
  comments,
  issueLabels,
  issues,
  labels,
  projects,
  workspaces,
} from "@/db/schema"

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
})
