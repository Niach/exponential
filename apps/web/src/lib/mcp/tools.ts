import { z } from "zod"
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import {
  and,
  asc,
  desc,
  eq,
  gte,
  ilike,
  inArray,
  isNull,
  lte,
  or,
} from "drizzle-orm"
import { db } from "@/db/connection"
import {
  attachments,
  comments,
  issueLabels,
  issues,
  labels,
  notifications,
  boards,
  runConfigs,
  users,
  teamInvites,
  teamMembers,
  teams,
} from "@/db/schema"
import {
  issuePriorityValues,
  issueStatusValues,
  boardIconValues,
} from "@/lib/domain"
import {
  assertTeamMember,
  getAttachmentTeamContext,
  getIssueTeamContext,
  getBoardTeamId,
  getUserTeamIds,
  resolveTeamAccess,
} from "@/lib/team-membership"
import { deleteObject, getObject, uploadObject } from "@/lib/storage"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  isAcceptedImageContentType,
  maxImageUploadBytes,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import { assertWithinStorageLimit } from "@/lib/billing"
import { appRouter } from "@/routes/api/trpc/$"
import type { Context } from "@/lib/trpc"
import { createPullRequest } from "@/lib/integrations/github-pr"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import { isInstallationLinkedToTeam } from "@/lib/trpc/integrations"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { applyPrLifecycleStatusInTx } from "@/lib/integrations/pr-sync"
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"
import { escapeLikePattern } from "@/lib/like-pattern"
import { err, ok } from "./helpers"
import type { McpUser } from "./server"
import {
  assertFullAccess,
  assertBoardGranted,
  assertTeamFullyGranted,
  assertTeamVisible,
  filterVisibleTeamIds,
  isBoardGranted,
  isTeamVisible,
  type McpAccess,
} from "./scope"

function buildCtx(user: McpUser, request: Request): Context {
  const now = new Date()
  return {
    db,
    request,
    session: {
      user: {
        id: user.id,
        email: user.email,
        name: user.name,
        image: user.image,
        emailVerified: user.emailVerified,
        createdAt: user.createdAt,
        updatedAt: user.updatedAt,
      },
      session: {
        id: `mcp`,
        userId: user.id,
        token: `mcp`,
        expiresAt: new Date(now.getTime() + 60 * 60 * 1000),
        createdAt: now,
        updatedAt: now,
        ipAddress: null,
        userAgent: `mcp`,
      },
    },
  } as unknown as Context
}

function caller(user: McpUser, request: Request) {
  return appRouter.createCaller(buildCtx(user, request))
}

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

// Resolve a UUID or human identifier ("MET-12") to an issue UUID, scoped to
// the user's accessible teams intersected with the connection's grant.
// The team-level access check still runs in the caller — this only maps
// the friendly identifier the coding agent knows to the row id. Identifiers
// are stored uppercase; the lookup is case-insensitive.
async function resolveIssueId(
  idOrIdentifier: string,
  userId: string,
  access: McpAccess
): Promise<string> {
  if (UUID_RE.test(idOrIdentifier)) return idOrIdentifier
  const teamIds = await getUserTeamIds(userId)
  if (teamIds.length > 0) {
    const boardRows = await db
      .select({ id: boards.id, teamId: boards.teamId })
      .from(boards)
      .where(
        and(inArray(boards.teamId, teamIds), isNull(boards.deletedAt))
      )
    const boardIds = boardRows
      .filter((r) => isBoardGranted(access, r.id, r.teamId))
      .map((r) => r.id)
    if (boardIds.length > 0) {
      const [row] = await db
        .select({ id: issues.id })
        .from(issues)
        .where(
          and(
            inArray(issues.boardId, boardIds),
            eq(issues.identifier, idOrIdentifier.toUpperCase())
          )
        )
        .limit(1)
      if (row) return row.id
    }
  }
  throw new Error(`Issue not found: ${idOrIdentifier}`)
}

// Comment id → its issue's team/board context, for grant checks on
// comment edit/delete (authorship itself is enforced in the comments router).
async function getCommentIssueContext(commentId: string) {
  const [row] = await db
    .select({ issueId: comments.issueId })
    .from(comments)
    .where(eq(comments.id, commentId))
    .limit(1)
  if (!row) throw new Error(`Comment not found`)
  return getIssueTeamContext(row.issueId)
}

// Run-config id → its board/team, for grant checks on update/delete.
async function getRunConfigContext(id: string) {
  const [row] = await db
    .select({
      boardId: runConfigs.boardId,
      teamId: runConfigs.teamId,
    })
    .from(runConfigs)
    .where(eq(runConfigs.id, id))
    .limit(1)
  if (!row) throw new Error(`Run config not found`)
  return row
}

const issueStatusEnumSchema = z.enum(issueStatusValues)
const issuePriorityEnumSchema = z.enum(issuePriorityValues)
const boardIconEnumSchema = z.enum(boardIconValues)
const dateOnly = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/, `Expected YYYY-MM-DD`)

export function registerExponentialTools(
  server: McpServer,
  user: McpUser,
  request: Request,
  access: McpAccess
) {
  // -----------------------------------------------------------------------
  // Teams
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_teams_list`,
    {
      title: `List teams`,
      description: `List teams the MCP user is a member of.`,
      inputSchema: {},
    },
    async () => {
      try {
        const memberRows = await db
          .select({
            id: teams.id,
            name: teams.name,
            slug: teams.slug,
            iconUrl: teams.iconUrl,
            role: teamMembers.role,
            createdAt: teams.createdAt,
            updatedAt: teams.updatedAt,
          })
          .from(teams)
          .innerJoin(
            teamMembers,
            eq(teamMembers.teamId, teams.id)
          )
          .where(eq(teamMembers.userId, user.id))
          .orderBy(asc(teams.name))

        // Membership-only, matching the sync semantics: public teams
        // appear once the user has explicitly joined, never implicitly.
        return ok(memberRows.filter((row) => isTeamVisible(access, row.id)))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_teams_get`,
    {
      title: `Get team`,
      description: `Get a single team by id.`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        assertTeamVisible(access, id)
        await resolveTeamAccess(user.id, id)
        const [row] = await db
          .select()
          .from(teams)
          .where(eq(teams.id, id))
          .limit(1)
        return ok(row)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Boards
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_boards_list`,
    {
      title: `List boards`,
      description: `List boards in a team, or across all teams the user belongs to.`,
      inputSchema: {
        teamId: z.string().uuid().optional(),
        includeArchived: z.boolean().default(false),
      },
    },
    async ({ teamId, includeArchived }) => {
      try {
        let allowedTeamIds: Array<string>
        if (teamId) {
          assertTeamVisible(access, teamId)
          await resolveTeamAccess(user.id, teamId)
          allowedTeamIds = [teamId]
        } else {
          allowedTeamIds = filterVisibleTeamIds(
            access,
            await getUserTeamIds(user.id)
          )
          if (allowedTeamIds.length === 0) return ok([])
        }

        const rows = await db
          .select()
          .from(boards)
          .where(inArray(boards.teamId, allowedTeamIds))
          .orderBy(asc(boards.sortOrder), asc(boards.name))

        const filtered = rows.filter(
          (row) =>
            isBoardGranted(access, row.id, row.teamId) &&
            row.deletedAt == null &&
            (includeArchived || row.archivedAt == null)
        )
        return ok(filtered)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_get`,
    {
      title: `Get board`,
      description: `Get a single board by id.`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        const board = await getBoardTeamId(id)
        assertBoardGranted(access, board.id, board.teamId)
        await resolveTeamAccess(user.id, board.teamId)
        const [row] = await db
          .select()
          .from(boards)
          .where(eq(boards.id, id))
          .limit(1)
        return ok(row)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_create`,
    {
      title: `Create board`,
      description: `Create a board in a team. The repository is always optional (coding features gate on repo presence). icon is a curated display icon name. For the repository pass either an existing registry repo (repository.repositoryId) or connect one inline (repository.fullName, "owner/name"). The MCP user must be a member of the team (owner/admin to connect a new repo).`,
      inputSchema: {
        teamId: z.string().uuid(),
        name: z.string().min(1).max(255),
        // Mirrors boards.create's floor (EXP-46): letter-led alphanumeric,
        // so identifiers stay `{PREFIX}-{number}` referenceable.
        prefix: z
          .string()
          .trim()
          .regex(
            /^[A-Za-z][A-Za-z0-9]{0,9}$/,
            `Prefix must be 1-10 letters or digits, starting with a letter`
          ),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
        icon: boardIconEnumSchema.optional(),
        repository: z
          .union([
            z.object({ repositoryId: z.string().uuid() }),
            z.object({
              fullName: z
                .string()
                .min(1)
                .max(255)
                .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`),
              defaultBranch: z.string().min(1).max(255).optional(),
              private: z.boolean().optional(),
              installationId: z.number().int().optional(),
            }),
          ])
          .optional(),
      },
    },
    async (input) => {
      try {
        // Creating a board needs the whole-team grant — a
        // single-board grant must not spawn siblings it can't see.
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).boards.create(input)
        return ok(result.board)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_update`,
    {
      title: `Update board`,
      description: `Update a board's name, color, icon, or archive state (archiving is owner-only).`,
      inputSchema: {
        id: z.string().uuid(),
        icon: boardIconEnumSchema.nullable().optional(),
        name: z.string().min(1).max(255).optional(),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
        archivedAt: z.string().datetime().nullable().optional(),
      },
    },
    async (input) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(input.id)
          assertBoardGranted(access, board.id, board.teamId)
        }
        const result = await caller(user, request).boards.update(input)
        return ok(result.board)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Issues
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issues_list`,
    {
      title: `List issues`,
      description: `List issues in boards the MCP user can access. Supports filtering by board, status, priority, assignee, due-date range, and a free-text title search. Defaults to non-archived issues, newest first.`,
      inputSchema: {
        boardId: z.string().uuid().optional(),
        teamId: z.string().uuid().optional(),
        status: z.array(issueStatusEnumSchema).optional(),
        priority: z.array(issuePriorityEnumSchema).optional(),
        assigneeId: z.string().nullable().optional(),
        dueAfter: dateOnly.optional(),
        dueBefore: dateOnly.optional(),
        search: z.string().min(1).optional(),
        includeArchived: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      },
    },
    async ({
      boardId,
      teamId,
      status,
      priority,
      assigneeId,
      dueAfter,
      dueBefore,
      search,
      includeArchived,
      limit,
      offset,
    }) => {
      try {
        let allowedBoardIds: Array<string>

        if (boardId) {
          const board = await getBoardTeamId(boardId)
          assertBoardGranted(access, board.id, board.teamId)
          await resolveTeamAccess(user.id, board.teamId)
          allowedBoardIds = [boardId]
        } else {
          let teamIds: Array<string>
          if (teamId) {
            assertTeamVisible(access, teamId)
            await resolveTeamAccess(user.id, teamId)
            teamIds = [teamId]
          } else {
            teamIds = filterVisibleTeamIds(
              access,
              await getUserTeamIds(user.id)
            )
          }
          if (teamIds.length === 0) return ok([])
          const boardRows = await db
            .select({ id: boards.id, teamId: boards.teamId })
            .from(boards)
            .where(
              and(
                inArray(boards.teamId, teamIds),
                isNull(boards.deletedAt)
              )
            )
          allowedBoardIds = boardRows
            .filter((r) => isBoardGranted(access, r.id, r.teamId))
            .map((r) => r.id)
        }

        if (allowedBoardIds.length === 0) return ok([])

        const conditions = [inArray(issues.boardId, allowedBoardIds)]
        if (status && status.length > 0) {
          conditions.push(inArray(issues.status, status))
        }
        if (priority && priority.length > 0) {
          conditions.push(inArray(issues.priority, priority))
        }
        if (assigneeId === null) {
          conditions.push(isNull(issues.assigneeId))
        } else if (assigneeId !== undefined) {
          conditions.push(eq(issues.assigneeId, assigneeId))
        }
        if (dueAfter) conditions.push(gte(issues.dueDate, dueAfter))
        if (dueBefore) conditions.push(lte(issues.dueDate, dueBefore))
        if (search) {
          conditions.push(ilike(issues.title, `%${escapeLikePattern(search)}%`))
        }
        // Filter in SQL, not after — a post-limit JS filter under-fills
        // pages and makes offset pagination skip live issues.
        if (!includeArchived) conditions.push(isNull(issues.archivedAt))

        const rows = await db
          .select()
          .from(issues)
          .where(and(...conditions))
          .orderBy(desc(issues.createdAt))
          .limit(limit)
          .offset(offset)

        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_get`,
    {
      title: `Get issue`,
      description: `Get a single issue by UUID or human identifier (e.g. "MET-12"), including its label ids and the latest comments on the thread (newest first). The recentComments array is capped at ${`50`}; pass commentsLimit to override.`,
      inputSchema: {
        id: z.string().min(1),
        commentsLimit: z.number().int().min(0).max(200).optional(),
      },
    },
    async ({ id: idInput, commentsLimit }) => {
      try {
        const id = await resolveIssueId(idInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(id)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        const [issue] = await db
          .select()
          .from(issues)
          .where(eq(issues.id, id))
          .limit(1)
        const labelRows = await db
          .select({ labelId: issueLabels.labelId })
          .from(issueLabels)
          .where(eq(issueLabels.issueId, id))
        const recentComments = await db
          .select({
            id: comments.id,
            authorId: comments.authorId,
            body: comments.body,
            createdAt: comments.createdAt,
            editedAt: comments.editedAt,
          })
          .from(comments)
          .where(eq(comments.issueId, id))
          .orderBy(desc(comments.createdAt))
          .limit(commentsLimit ?? 50)
        return ok({
          ...issue,
          labelIds: labelRows.map((r) => r.labelId),
          recentComments,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_create`,
    {
      title: `Create issue`,
      description: `Create a new issue in a board the MCP user has access to. Description must be plain text (no embedded images on creation).`,
      inputSchema: {
        boardId: z.string().uuid(),
        title: z.string().min(1).max(500),
        status: issueStatusEnumSchema.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        descriptionText: z.string().optional(),
        dueDate: dateOnly.nullable().optional(),
        labelIds: z.array(z.string().uuid()).optional(),
      },
    },
    async ({ descriptionText, ...rest }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(rest.boardId)
          assertBoardGranted(access, board.id, board.teamId)
        }
        const result = await caller(user, request).issues.create({
          ...rest,
          description: descriptionText ? descriptionText : undefined,
        })
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_update`,
    {
      title: `Update issue`,
      description: `Update an issue's fields. Pass only the fields you want to change.`,
      inputSchema: {
        id: z.string().uuid(),
        title: z.string().min(1).max(500).optional(),
        status: issueStatusEnumSchema.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        descriptionText: z.string().nullable().optional(),
        dueDate: dateOnly.nullable().optional(),
      },
    },
    async ({ descriptionText, ...rest }) => {
      try {
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(rest.id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const description =
          descriptionText === undefined
            ? undefined
            : descriptionText === null
              ? null
              : descriptionText
        const result = await caller(user, request).issues.update({
          ...rest,
          description,
        })
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_delete`,
    {
      title: `Delete issue`,
      description: `Permanently delete an issue. Cascades to its labels, attachments, comments, and relations. Attachment storage objects are also removed.`,
      inputSchema: { id: z.string().uuid() },
    },
    async (input) => {
      try {
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(input.id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).issues.delete(input)
        return ok({ ok: true, id: input.id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Attachments
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_attachments_get`,
    {
      title: `Get attachment (image)`,
      description: `Fetch an issue attachment by id and return it as image content so MCP clients can view it. Issue descriptions and comments embed attachments as markdown image links of the form ![alt](/api/attachments/{id}) — pass that {id} here. Only image attachments are returned inline; other content types are rejected.`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        const attachment = await getAttachmentTeamContext(id)
        assertBoardGranted(access, attachment.boardId, attachment.teamId)
        await resolveTeamAccess(user.id, attachment.teamId)

        if (!attachment.contentType.startsWith(`image/`)) {
          throw new Error(
            `Attachment ${id} is ${attachment.contentType}, not an image — only images can be returned inline.`
          )
        }

        const object = await getObject(attachment.storageKey)
        if (!object?.Body) throw new Error(`Attachment object not found`)
        const bytes = await object.Body.transformToByteArray()

        return {
          content: [
            {
              type: `image` as const,
              data: Buffer.from(bytes).toString(`base64`),
              mimeType: attachment.contentType,
            },
          ],
        }
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Labels
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_labels_list`,
    {
      title: `List labels`,
      description: `List labels for a team.`,
      inputSchema: { teamId: z.string().uuid() },
    },
    async ({ teamId }) => {
      try {
        // Labels are team-level but issue workflows in a granted board
        // need them, so a visible (board-granted) team suffices to read.
        assertTeamVisible(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const rows = await db
          .select()
          .from(labels)
          .where(eq(labels.teamId, teamId))
          .orderBy(asc(labels.sortOrder), asc(labels.name))
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_get`,
    {
      title: `Get label`,
      description: `Get a label by id (must be in a team the user belongs to).`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        const [label] = await db
          .select()
          .from(labels)
          .where(eq(labels.id, id))
          .limit(1)
        if (!label) return err(new Error(`Label not found`))
        assertTeamVisible(access, label.teamId)
        await resolveTeamAccess(user.id, label.teamId)
        return ok(label)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_create`,
    {
      title: `Create label`,
      description: `Create a label in a team.`,
      inputSchema: {
        teamId: z.string().uuid(),
        name: z.string().min(1).max(255),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .default(`#6366f1`),
      },
    },
    async (input) => {
      try {
        // Label mutations touch every board in the team — whole-
        // team grant required.
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).labels.create(input)
        return ok(result.label)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_update`,
    {
      title: `Update label`,
      description: `Update a label's name or color.`,
      inputSchema: {
        teamId: z.string().uuid(),
        labelId: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
      },
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        await caller(user, request).labels.update(input)
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_delete`,
    {
      title: `Delete label`,
      description: `Delete a label from a team.`,
      inputSchema: {
        teamId: z.string().uuid(),
        labelId: z.string().uuid(),
      },
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        await caller(user, request).labels.delete(input)
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Issue ↔ Label
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issue_labels_add`,
    {
      title: `Add label to issue`,
      description: `Attach a label to an issue (teams must match).`,
      inputSchema: {
        issueId: z.string().uuid(),
        labelId: z.string().uuid(),
      },
    },
    async (input) => {
      try {
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(input.issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).issueLabels.add(input)
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issue_labels_remove`,
    {
      title: `Remove label from issue`,
      description: `Detach a label from an issue.`,
      inputSchema: {
        issueId: z.string().uuid(),
        labelId: z.string().uuid(),
      },
    },
    async (input) => {
      try {
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(input.issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).issueLabels.remove(input)
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Comments
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_comments_list`,
    {
      title: `List comments on an issue`,
      description: `List comments on an issue (oldest first) by UUID or human identifier (e.g. "MET-12"). The MCP user must have access to the issue's team.`,
      inputSchema: {
        issueId: z.string().min(1),
        limit: z.number().int().min(1).max(200).default(100),
        offset: z.number().int().min(0).default(0),
      },
    },
    async ({ issueId: issueIdInput, limit, offset }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(issueId)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        const rows = await db
          .select()
          .from(comments)
          .where(eq(comments.issueId, issueId))
          .orderBy(asc(comments.createdAt))
          .limit(limit)
          .offset(offset)
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_comments_create`,
    {
      title: `Comment on an issue`,
      description: `Post a regular comment on an issue (by UUID or human identifier, e.g. "MET-12") authored by the MCP user. Body is plain text.`,
      inputSchema: {
        issueId: z.string().min(1),
        bodyText: z.string().min(1).max(10_000),
      },
    },
    async ({ issueId: issueIdInput, bodyText }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).comments.create({
          issueId,
          body: bodyText,
        })
        return ok(result.comment)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Coding flow (status + pull requests)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issues_update_status`,
    {
      title: `Update issue status (coding flow)`,
      description: `Set an issue's status during a coding session. Restricted to 'in_progress' (you started working) and 'done' (work is complete and merged). Do NOT set 'in_review' yourself — calling exponential_pr_open automatically moves the issue to 'in_review', and merging the PR moves it to 'done'. Accepts a UUID or human identifier (e.g. "MET-12").`,
      inputSchema: {
        issueId: z.string().min(1),
        status: z.enum([`in_progress`, `done`]),
      },
    },
    async ({ issueId, status }) => {
      try {
        const id = await resolveIssueId(issueId, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).issues.update({ id, status })
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_open`,
    {
      title: `Open a pull request for one issue or a batch of issues`,
      description: `Open a GitHub pull request via the linked repository and link it to the issue(s). The SERVER opens the PR via the GitHub App — you don't need 'gh' or a token. Pass EXACTLY ONE of 'issueId' (single issue) or 'issueIds' (a batch coding run's issues — ONE combined PR linked to every listed issue; all issues must resolve to the same repository, and 'head' is REQUIRED: the pushed batch branch, e.g. 'exp/batch-<id>'). For a single issue, 'head' defaults to the issue's branch or 'exp/<IDENTIFIER>'. 'base' defaults to the repo's default branch. On success each linked issue records prUrl/prNumber/prState='open'/branch and a pr_opened activity event, and moves to status 'in_review'; merging the PR later completes them all (status 'done'). Fails with a clear message if a board has no linked repository. Accepts UUIDs or human identifiers (e.g. "MET-12").`,
      inputSchema: {
        issueId: z.string().min(1).optional(),
        issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
        title: z.string().min(1).max(255),
        body: z.string().max(60_000).optional(),
        head: z.string().max(255).optional(),
        base: z.string().max(255).optional(),
      },
    },
    async ({ issueId, issueIds, title, body, head, base }) => {
      try {
        if (Boolean(issueId) === Boolean(issueIds?.length)) {
          throw new Error(`Provide exactly one of issueId or issueIds`)
        }
        if (issueIds?.length && !head) {
          throw new Error(
            `'head' is required with issueIds — pass the pushed batch branch`
          )
        }

        // Resolve + authorize every issue; a batch must land in ONE repo.
        const rawIds = issueIds ?? [issueId!]
        const ids: string[] = []
        for (const raw of rawIds) {
          const id = await resolveIssueId(raw, user.id, access)
          if (!ids.includes(id)) ids.push(id)
        }

        const teamIdByIssue = new Map<string, string>()
        let repo: {
          repositoryId: string
          fullName: string
          defaultBranch: string
        } | null = null
        for (const id of ids) {
          const issueCtx = await getIssueTeamContext(id)
          assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
          await resolveTeamAccess(user.id, issueCtx.teamId)
          teamIdByIssue.set(id, issueCtx.teamId)

          const issueRepo = await caller(user, request).repositories.forIssue({
            issueId: id,
          })
          if (!issueRepo) {
            throw new Error(
              `No repository linked to this board — link one in team settings.`
            )
          }
          if (repo && repo.repositoryId !== issueRepo.repositoryId) {
            throw new Error(
              `All issues in a batch PR must share one repository (${repo.fullName} vs ${issueRepo.fullName}).`
            )
          }
          repo = issueRepo
        }
        if (!repo) throw new Error(`Issue not found`)

        let headBranch = head
        if (!headBranch) {
          const [issue] = await db
            .select({ identifier: issues.identifier, branch: issues.branch })
            .from(issues)
            .where(eq(issues.id, ids[0]))
            .limit(1)
          if (!issue) throw new Error(`Issue not found`)
          headBranch = issue.branch ?? `exp/${issue.identifier}`
        }
        const baseBranch = base ?? repo.defaultBranch

        const resolved = await resolveRepoInstallationTokenInfo(repo.fullName)
        if (!resolved) {
          throw new Error(
            `The Exponential GitHub App is not installed on ${repo.fullName}.`
          )
        }
        // Link-gate (mirrors issues.mergePr/closePr): the installation serving
        // this repo must still be claimed by the issue's team — a
        // deliberately severed GitHub connection must not keep authorizing PR
        // writes through the App.
        for (const wsId of new Set(teamIdByIssue.values())) {
          if (
            !(await isInstallationLinkedToTeam(
              wsId,
              resolved.installationId
            ))
          ) {
            throw new Error(
              `${repo.fullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`
            )
          }
        }
        const token = resolved.token

        const created = await createPullRequest({
          repo: repo.fullName,
          head: headBranch,
          base: baseBranch,
          title,
          body: body ?? ``,
          token,
        })

        await db.transaction(async (tx) => {
          for (const id of ids) {
            const [current] = await tx
              .select({ status: issues.status })
              .from(issues)
              .where(eq(issues.id, id))
              .limit(1)
            await tx
              .update(issues)
              .set({
                prUrl: created.url,
                prNumber: created.number,
                prState: `open`,
                branch: headBranch,
              })
              .where(eq(issues.id, id))
            await recordIssueEvent(tx, {
              issueId: id,
              teamId: teamIdByIssue.get(id)!,
              actorUserId: user.id,
              type: `pr_opened`,
              payload: {
                prUrl: created.url,
                prNumber: created.number,
                branch: headBranch,
              },
            })
            // The open PR parks the issue in review (EXP-120).
            if (current) {
              await applyPrLifecycleStatusInTx(tx, {
                issueId: id,
                teamId: teamIdByIssue.get(id)!,
                actorUserId: user.id,
                currentStatus: current.status,
                to: `in_review`,
              })
            }
          }
        })

        // Away/phone flow: "PR opened" reaches assignee + subscribers on
        // in-app + push + email (deliver()'s dedupe window absorbs the
        // near-simultaneous GitHub webhook `opened` fan-out).
        for (const id of ids) {
          fireAndForgetPrNotify({
            issueId: id,
            type: `pr_opened`,
            actorUserId: user.id,
          })
        }

        return ok({ url: created.url, number: created.number })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Comments (edit / delete)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_comments_update`,
    {
      title: `Edit a comment`,
      description: `Edit the body of an existing comment (by its UUID). Only the comment's author can edit it. Body is plain text; the edit stamps editedAt.`,
      inputSchema: {
        id: z.string().uuid(),
        bodyText: z.string().min(1).max(10_000),
      },
    },
    async ({ id, bodyText }) => {
      try {
        if (!access.full) {
          const ctxIssue = await getCommentIssueContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).comments.update({
          id,
          body: bodyText,
        })
        return ok(result.comment)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_comments_delete`,
    {
      title: `Delete a comment`,
      description: `Permanently delete a comment (by its UUID). Only the comment's author or an admin can delete it.`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const ctxIssue = await getCommentIssueContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).comments.delete({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Subscriptions (follow / unfollow an issue)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issues_subscribe`,
    {
      title: `Subscribe to an issue`,
      description: `Subscribe the MCP user to an issue (by UUID or human identifier, e.g. "MET-12") so they receive its notifications. Idempotent.`,
      inputSchema: { issueId: z.string().min(1) },
    },
    async ({ issueId: issueIdInput }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).subscriptions.subscribe({ issueId })
        return ok({ ok: true, issueId, subscribed: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_unsubscribe`,
    {
      title: `Unsubscribe from an issue`,
      description: `Unsubscribe the MCP user from an issue (by UUID or human identifier, e.g. "MET-12"). Suppresses future auto-resubscribe until they act on the issue again.`,
      inputSchema: { issueId: z.string().min(1) },
    },
    async ({ issueId: issueIdInput }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).subscriptions.unsubscribe({ issueId })
        return ok({ ok: true, issueId, subscribed: false })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Notifications (inbox)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_notifications_list`,
    {
      title: `List notifications`,
      description: `List the MCP user's own notifications, newest first. Set unreadOnly to show only those not yet read.`,
      inputSchema: {
        unreadOnly: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      },
    },
    async ({ unreadOnly, limit, offset }) => {
      try {
        const conditions = [eq(notifications.userId, user.id)]
        if (unreadOnly) conditions.push(isNull(notifications.readAt))
        if (access.full) {
          const rows = await db
            .select()
            .from(notifications)
            .where(and(...conditions))
            .orderBy(desc(notifications.createdAt))
            .limit(limit)
            .offset(offset)
          return ok(rows)
        }
        // Scoped connection: the inbox spans every team, so join through
        // the notification's issue and keep only granted boards (rows
        // without an issue stay private). The grant filter runs in SQL,
        // BEFORE limit/offset — a post-limit JS filter under-fills pages and
        // makes offset pagination skip in-scope notifications.
        const grantedBoardIds = [...access.grantedBoardIds]
        const fullTeamIds = [...access.fullTeamIds]
        const grantClauses = [
          ...(grantedBoardIds.length > 0
            ? [inArray(issues.boardId, grantedBoardIds)]
            : []),
          ...(fullTeamIds.length > 0
            ? [inArray(boards.teamId, fullTeamIds)]
            : []),
        ]
        if (grantClauses.length === 0) return ok([])
        conditions.push(or(...grantClauses)!)
        const rows = await db
          .select({ notification: notifications })
          .from(notifications)
          .innerJoin(issues, eq(notifications.issueId, issues.id))
          .innerJoin(boards, eq(issues.boardId, boards.id))
          .where(and(...conditions))
          .orderBy(desc(notifications.createdAt))
          .limit(limit)
          .offset(offset)
        return ok(rows.map((r) => r.notification))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_notifications_mark_read`,
    {
      title: `Mark notifications read`,
      description: `Mark a single notification read by passing its id, or mark every unread notification read by passing all=true. Only the MCP user's own notifications are affected.`,
      inputSchema: {
        id: z.string().uuid().optional(),
        all: z.boolean().default(false),
      },
    },
    async ({ id, all }) => {
      try {
        if (all) {
          // Marking the whole inbox read touches every team.
          assertFullAccess(access)
          await caller(user, request).notifications.markAllRead()
          return ok({ ok: true, marked: `all` })
        }
        if (!id) {
          throw new Error(`Pass a notification id, or all=true.`)
        }
        if (!access.full) {
          const [row] = await db
            .select({ issueId: notifications.issueId })
            .from(notifications)
            .where(
              and(eq(notifications.id, id), eq(notifications.userId, user.id))
            )
            .limit(1)
          if (!row?.issueId) throw new Error(`Notification not found`)
          const ctxIssue = await getIssueTeamContext(row.issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).notifications.markRead({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Members (resolve assignees)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_members_list`,
    {
      title: `List team members`,
      description: `List the members of a team with their id, name, email, and role — use this to resolve an assigneeId for issues. Synthetic bot users (the feedback-widget helpdesk identity) are excluded unless includeAgents is set.`,
      inputSchema: {
        teamId: z.string().uuid(),
        includeAgents: z.boolean().default(false),
      },
    },
    async ({ teamId, includeAgents }) => {
      try {
        assertTeamVisible(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const conditions = [eq(teamMembers.teamId, teamId)]
        if (!includeAgents) conditions.push(eq(users.isAgent, false))
        const rows = await db
          .select({
            id: users.id,
            name: users.name,
            email: users.email,
            image: users.image,
            isAgent: users.isAgent,
            role: teamMembers.role,
          })
          .from(teamMembers)
          .innerJoin(users, eq(users.id, teamMembers.userId))
          .where(and(...conditions))
          .orderBy(asc(users.name))
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Repositories
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_repositories_list`,
    {
      title: `List repositories`,
      description: `List the repositories registered in a team, each with the boards it backs. The MCP user must be a member of the team.`,
      inputSchema: { teamId: z.string().uuid() },
    },
    async ({ teamId }) => {
      try {
        assertTeamVisible(access, teamId)
        const result = await caller(user, request).repositories.list({
          teamId,
        })
        if (access.full) return ok(result)
        // Each repo rides with the boards it backs — a board-scoped
        // grant must not enumerate ungranted sibling boards through them.
        return ok(
          result.map((repo) => ({
            ...repo,
            boards: repo.boards.filter((p) =>
              isBoardGranted(access, p.id, teamId)
            ),
          }))
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_repositories_add`,
    {
      title: `Register a repository`,
      description: `Register a GitHub repository ("owner/name") in a team so boards can be backed by it. The repo must belong to a GitHub account (App installation) connected to the team — connect one in team settings → Repositories. Owner/admin only.`,
      inputSchema: {
        teamId: z.string().uuid(),
        fullName: z
          .string()
          .min(1)
          .max(255)
          .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`),
        defaultBranch: z.string().min(1).max(255).optional(),
        private: z.boolean().optional(),
        installationId: z.number().int().optional(),
      },
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).repositories.add(input)
        return ok(result.repository)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_repositories_branch_diff`,
    {
      title: `Diff an issue's branch`,
      description: `Get the diff of an issue's exp/<IDENTIFIER> branch against its repository's default branch (by issue UUID or human identifier, e.g. "MET-12"). Returns null when the branch was never pushed. The MCP user must be a member of the issue's team.`,
      inputSchema: { issueId: z.string().min(1) },
    },
    async ({ issueId: issueIdInput }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).repositories.branchDiff({
          issueId,
        })
        return ok(result)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Run configs (per-board terminal commands)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_run_configs_list`,
    {
      title: `List run configs`,
      description: `List a board's run configs (named terminal commands: argv, cwd, env). The MCP user must be a member of the board's team.`,
      inputSchema: { boardId: z.string().uuid() },
    },
    async ({ boardId }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(boardId)
          assertBoardGranted(access, board.id, board.teamId)
        }
        const result = await caller(user, request).runConfigs.list({
          boardId,
        })
        return ok(result.configs)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_run_configs_create`,
    {
      title: `Create a run config`,
      description: `Create a named run config for a board. argv is spawned directly (no shell) — argv[0] is the program, the rest are its arguments. cwd (relative to repo root, no "..") and env are optional. Team owner only.`,
      inputSchema: {
        boardId: z.string().uuid(),
        name: z.string().min(1).max(120),
        argv: z.array(z.string()).min(1),
        cwd: z.string().nullable().optional(),
        env: z.record(z.string(), z.string()).optional(),
      },
    },
    async (input) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(input.boardId)
          assertBoardGranted(access, board.id, board.teamId)
        }
        const result = await caller(user, request).runConfigs.create(input)
        return ok(result.config)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_run_configs_update`,
    {
      title: `Update a run config`,
      description: `Update a run config's name, argv, cwd, env, or sortOrder (by its UUID). Pass only the fields you want to change. Team owner only.`,
      inputSchema: {
        id: z.string().uuid(),
        name: z.string().min(1).max(120).optional(),
        argv: z.array(z.string()).min(1).optional(),
        cwd: z.string().nullable().optional(),
        env: z.record(z.string(), z.string()).optional(),
        sortOrder: z.number().finite().optional(),
      },
    },
    async (input) => {
      try {
        if (!access.full) {
          const cfg = await getRunConfigContext(input.id)
          assertBoardGranted(access, cfg.boardId, cfg.teamId)
        }
        const result = await caller(user, request).runConfigs.update(input)
        return ok(result.config)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_run_configs_delete`,
    {
      title: `Delete a run config`,
      description: `Delete a run config by its UUID. Team owner only.`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const cfg = await getRunConfigContext(id)
          assertBoardGranted(access, cfg.boardId, cfg.teamId)
        }
        await caller(user, request).runConfigs.delete({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Pull request changed files
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issues_pr_files`,
    {
      title: `List an issue's PR changed files`,
      description: `List the changed files (with patches and add/delete counts) of the pull request linked to an issue (by UUID or human identifier, e.g. "MET-12"). Returns an empty file list when the issue has no linked PR. The MCP user must be a member of the issue's team.`,
      inputSchema: { issueId: z.string().min(1) },
    },
    async ({ issueId: issueIdInput }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).issues.prFiles({ issueId })
        return ok(result)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Boards (delete / retarget repository)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_boards_delete`,
    {
      title: `Delete a board`,
      description: `Move a board to the trash. It is permanently purged (with all issues) after 48 hours; team owners can restore it from web settings before then. Team owner only. Protected boards cannot be deleted.`,
      inputSchema: { boardId: z.string().uuid() },
    },
    async ({ boardId }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(boardId)
          assertBoardGranted(access, board.id, board.teamId)
        }
        await caller(user, request).boards.delete({ boardId })
        return ok({ ok: true, boardId })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_set_repository`,
    {
      title: `Retarget a board's repository`,
      description: `Point a board at a different registered repository (both must be in the same team). Owner/admin only. Existing worktrees keep working; new coding sessions use the new repo.`,
      inputSchema: {
        boardId: z.string().uuid(),
        repositoryId: z.string().uuid(),
      },
    },
    async (input) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(input.boardId)
          // Retargeting widens the token's GitHub reach to ANY repo in the
          // team registry (pr_open / pr_files / branch_diff then reach
          // the new repo through the granted board's issues) — so this is
          // a team-registry mutation, gated like repositories_add, not
          // a board-scoped one.
          assertTeamFullyGranted(access, board.teamId)
        }
        const result = await caller(user, request).boards.setRepository(input)
        return ok(result.board)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Teams (create / update)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_teams_create`,
    {
      title: `Create a team`,
      description: `Create a new team owned by the MCP user (a unique slug is derived from the name).`,
      inputSchema: {
        name: z.string().min(1).max(255),
        iconUrl: z.string().url().max(2048).optional(),
      },
    },
    async (input) => {
      try {
        // A new team is outside any selectable grant — full access only.
        assertFullAccess(access)
        const result = await caller(user, request).teams.create(input)
        return ok(result.team)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_teams_update`,
    {
      title: `Update a team`,
      description: `Update a team's name or icon (by its UUID). Team owner only. Teams are always private.`,
      inputSchema: {
        id: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        iconUrl: z.string().url().max(2048).nullable().optional(),
      },
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.id)
        const result = await caller(user, request).teams.update(input)
        return ok(result.team)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Team invites (owner-gated)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_invites_create`,
    {
      title: `Create a team invite`,
      description: `Create an invite link for a team, returning the token to share. Owner only.`,
      inputSchema: {
        teamId: z.string().uuid(),
        role: z.enum([`owner`, `member`]).default(`member`),
      },
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).teamInvites.create(input)
        return ok({ invite: result.invite, token: result.token })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_invites_list`,
    {
      title: `List pending invites`,
      description: `List the pending (unaccepted) invites for a team. The MCP user must be a member of the team.`,
      inputSchema: { teamId: z.string().uuid() },
    },
    async ({ teamId }) => {
      try {
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).teamInvites.list({
          teamId,
        })
        return ok(result.invites)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_invites_revoke`,
    {
      title: `Revoke a team invite`,
      description: `Revoke a pending invite by its UUID. Owner only.`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const [invite] = await db
            .select({ teamId: teamInvites.teamId })
            .from(teamInvites)
            .where(eq(teamInvites.id, id))
            .limit(1)
          if (!invite) throw new Error(`Invite not found`)
          assertTeamFullyGranted(access, invite.teamId)
        }
        await caller(user, request).teamInvites.revoke({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Attachments upload (base64 image → S3 → attachments row)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_attachments_upload`,
    {
      title: `Upload an image attachment`,
      description: `Upload a base64-encoded image and attach it to an issue (by UUID or human identifier, e.g. "MET-12"). Returns the canonical markdown form ![](/api/attachments/{id}) — embed that string in the issue's description or a comment to show the image. Images only (png/jpeg/webp/gif/avif), 10 MB max; the team storage plan limit applies.`,
      inputSchema: {
        issueId: z.string().min(1),
        filename: z.string().min(1).max(255),
        contentType: z.string().min(1).max(255),
        dataBase64: z.string().min(1),
        alt: z.string().max(500).optional(),
      },
    },
    async ({
      issueId: issueIdInput,
      filename: filenameInput,
      contentType,
      dataBase64,
      alt,
    }) => {
      try {
        // The zod schema only checks length — strip control chars (CRLF would
        // otherwise poison the read path's Content-Disposition header).
        const filename = sanitizeUploadFilename(filenameInput, `image`)
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const issueCtx = await getIssueTeamContext(issueId)
        assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
        await assertTeamMember(user.id, issueCtx.teamId)

        if (!isAcceptedImageContentType(contentType)) {
          throw new Error(
            `Unsupported image type "${contentType}" — only PNG, JPEG, WebP, GIF, and AVIF images are accepted.`
          )
        }

        const body = new Uint8Array(Buffer.from(dataBase64, `base64`))
        if (body.byteLength === 0) {
          throw new Error(`Decoded image is empty — check the base64 payload.`)
        }
        if (body.byteLength > maxImageUploadBytes) {
          throw new Error(`Images must be 10 MB or smaller.`)
        }

        await assertWithinStorageLimit(issueCtx.teamId, body.byteLength)

        const attachmentId = crypto.randomUUID()
        const storageKey = buildAttachmentStorageKey(
          issueId,
          attachmentId,
          filename
        )
        const url = buildAttachmentUrl(attachmentId)
        const dimensions = getImageDimensions(body)

        await uploadObject({
          body,
          contentLength: body.byteLength,
          contentType,
          key: storageKey,
        })

        try {
          await db.insert(attachments).values({
            id: attachmentId,
            teamId: issueCtx.teamId,
            boardId: issueCtx.boardId,
            issueId,
            uploaderId: user.id,
            filename,
            contentType,
            sizeBytes: body.byteLength,
            storageKey,
            url,
            width: dimensions?.width ?? null,
            height: dimensions?.height ?? null,
          })
        } catch (error) {
          try {
            await deleteObject(storageKey)
          } catch (deleteError) {
            console.error(
              `Failed to rollback uploaded attachment object`,
              deleteError
            )
          }
          throw error
        }

        return ok({
          id: attachmentId,
          url,
          markdown: `![${alt ?? ``}](${url})`,
          filename,
          contentType,
          sizeBytes: body.byteLength,
          width: dimensions?.width ?? null,
          height: dimensions?.height ?? null,
        })
      } catch (e) {
        return err(e)
      }
    }
  )
}
