import { z } from "zod"
import {
  actionInputsSchema,
  actionTriggerSchema,
  MAX_ISSUE_DESCRIPTION,
} from "@exp/db-schema/domain"
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
  actions,
  attachments,
  codingSessions,
  comments,
  issueLabels,
  issues,
  issueStatuses,
  labels,
  notifications,
  boards,
  users,
  teamInvites,
  teamMembers,
  teams,
} from "@/db/schema"
import {
  issuePriorityValues,
  issueStatusValues,
  issueStatusCategoryDisplayOrder,
  boardIconValues,
} from "@/lib/domain"
import { teamColumns } from "@/lib/team-columns"
import {
  assertTeamMember,
  getAttachmentTeamContext,
  getIssueTeamContext,
  getBoardTeamId,
  getUserTeamIds,
  resolveTeamAccess,
} from "@/lib/team-membership"
import { boardVisible } from "@/lib/board-visibility"
import { deleteObject, getObject, uploadObject } from "@/lib/storage"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  canonicalizeContentType,
  getMaxUploadBytesForContentType,
  isAcceptedImageContentType,
  maxFileUploadBytes,
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
import {
  claimPrOpen,
  releasePrOpenClaim,
} from "@/lib/integrations/pr-actor-claims"
import { escapeLikePattern } from "@/lib/like-pattern"
import { buildRuntimeConfig } from "@/lib/runtime-config"
import { createAgentBugReport } from "@/lib/widget/agent-report"
import { TokenBucketLimiter } from "@/lib/widget/rate-limit"
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

// EXP-496: per-user bound on agent bug reports. In-process like the widget
// buckets (rate-limit.ts documents why that is fine for this deploy).
const agentBugReportLimiter = new TokenBucketLimiter({
  capacity: 3,
  refillPerHour: 10,
})

function buildCtx(user: McpUser, request: Request): Context {
  const now = new Date()
  return {
    db,
    request,
    viaMcp: true,
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
      .where(and(inArray(boards.teamId, teamIds), boardVisible()))
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

// Action id → its team, for grant checks on update/delete.
async function getActionContext(id: string) {
  const [row] = await db
    .select({ teamId: actions.teamId })
    .from(actions)
    .where(eq(actions.id, id))
    .limit(1)
  if (!row) throw new Error(`Action not found`)
  return row
}

const issueStatusEnumSchema = z.enum(issueStatusValues)
const issuePriorityEnumSchema = z.enum(issuePriorityValues)
// EXP-353: keep the serialized tool context small — every schema below is part
// of the MCP client's system prompt. z.uuid()'s 155-char pattern and the
// 60-name icon enum each repeated across tools were ~10k chars of context, so
// both validate via refine (runtime-only, invisible to the JSON schema).
const uuidString = z.string().refine((v) => UUID_RE.test(v), `Expected a UUID`)
const boardIconEnumSchema = z
  .string()
  .refine(
    (v) => (boardIconValues as ReadonlyArray<string>).includes(v),
    `Unknown icon. Valid names: ${boardIconValues.join(`, `)}`
  )
  .transform((v) => v as (typeof boardIconValues)[number])
const dateOnly = z.string().regex(/^\d{4}-\d{2}-\d{2}$/, `Expected YYYY-MM-DD`)

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
          .innerJoin(teamMembers, eq(teamMembers.teamId, teams.id))
          .where(eq(teamMembers.userId, user.id))
          .orderBy(asc(teams.name))

        // Membership-only, matching the sync semantics: a team appears
        // only once the user is a member.
        return ok(memberRows.filter((row) => isTeamVisible(access, row.id)))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_teams_get`,
    {
      description: `Get a single team by id.`,
      inputSchema: { id: uuidString },
    },
    async ({ id }) => {
      try {
        assertTeamVisible(access, id)
        await resolveTeamAccess(user.id, id)
        // Projected, never `select()` — server-only columns (comp_tier) stay
        // behind the same allowlist the teams shape pins (REV2-67).
        const [row] = await db
          .select(teamColumns)
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
      description: `List boards in a team, or across all teams the user belongs to.`,
      inputSchema: {
        teamId: uuidString.optional(),
      },
    },
    async ({ teamId }) => {
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
          .where(and(inArray(boards.teamId, allowedTeamIds), boardVisible()))
          .orderBy(asc(boards.sortOrder), asc(boards.name))

        const filtered = rows.filter((row) =>
          isBoardGranted(access, row.id, row.teamId)
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
      description: `Get a single board by id.`,
      inputSchema: { id: uuidString },
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
      description: `Create a board in a team (member; owner/admin to connect a new repo). The repository is optional. Coding features gate on repo presence. Pass repository.repositoryId (registry repo) or repository.fullName ("owner/name") to connect one inline. icon is a curated icon name.`,
      inputSchema: {
        teamId: uuidString,
        name: z.string().min(1).max(255),
        // Mirrors boards.create's floor (EXP-46): letter-led alphanumeric,
        // max 4, unique per team (REV-4) — identifiers stay `{PREFIX}-{number}`
        // referenceable and team-unique.
        prefix: z
          .string()
          .trim()
          .regex(
            /^[A-Za-z][A-Za-z0-9]{0,3}$/,
            `Prefix must be 1-4 letters or digits, starting with a letter`
          ),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
        icon: boardIconEnumSchema.optional(),
        repository: z
          .union([
            z.object({ repositoryId: uuidString }),
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
      description: `Update a board's name, color, or icon.`,
      inputSchema: {
        id: uuidString,
        icon: boardIconEnumSchema.nullable().optional(),
        name: z.string().min(1).max(255).optional(),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
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
      description: `List issues in boards the MCP user can access. Supports filtering by board, status, priority, assignee, due-date range, and a free-text title search. Newest first.`,
      inputSchema: {
        boardId: uuidString.optional(),
        teamId: uuidString.optional(),
        status: z.array(issueStatusEnumSchema).optional(),
        priority: z.array(issuePriorityEnumSchema).optional(),
        assigneeId: z.string().nullable().optional(),
        dueAfter: dateOnly.optional(),
        dueBefore: dateOnly.optional(),
        search: z.string().min(1).max(256).optional(),
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
              and(inArray(boards.teamId, teamIds), boardVisible())
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
      description: `Get a single issue by UUID or identifier (e.g. "MET-12"), including its label ids and latest comments (newest first, capped at 50; commentsLimit overrides).`,
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
      description: `Create a new issue in a board the MCP user has access to. Description must be plain text (no embedded images on creation).`,
      inputSchema: {
        boardId: uuidString,
        title: z.string().min(1).max(500),
        status: issueStatusEnumSchema.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        descriptionText: z.string().max(MAX_ISSUE_DESCRIPTION).optional(),
        dueDate: dateOnly.nullable().optional(),
        labelIds: z.array(uuidString).optional(),
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
      description: `Update an issue's fields. Pass only the fields you want to change. For a custom status pass statusId (not status); see exponential_statuses_list.`,
      inputSchema: {
        id: uuidString,
        title: z.string().min(1).max(500).optional(),
        status: issueStatusEnumSchema.optional(),
        statusId: uuidString.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        descriptionText: z
          .string()
          .max(MAX_ISSUE_DESCRIPTION)
          .nullable()
          .optional(),
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
      description: `Permanently delete an issue. Cascades to its labels, attachments, comments, and relations. Attachment storage objects are also removed.`,
      inputSchema: { id: uuidString },
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
      description: `Fetch an issue attachment by id and return it as inline image content. Markdown embeds look like ![alt](/api/attachments/{id}). Pass that {id}. Non-image content types are rejected.`,
      inputSchema: { id: uuidString },
    },
    async ({ id }) => {
      try {
        const attachment = await getAttachmentTeamContext(id)
        assertBoardGranted(access, attachment.boardId, attachment.teamId)
        await resolveTeamAccess(user.id, attachment.teamId)

        if (!attachment.contentType.startsWith(`image/`)) {
          throw new Error(
            `Attachment ${id} is ${attachment.contentType}. Only images can be returned inline.`
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
      description: `List labels for a team.`,
      inputSchema: { teamId: uuidString },
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
      description: `Get a label by id (must be in a team the user belongs to).`,
      inputSchema: { id: uuidString },
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
      description: `Create a label in a team.`,
      inputSchema: {
        teamId: uuidString,
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
      description: `Update a label's name or color.`,
      inputSchema: {
        teamId: uuidString,
        labelId: uuidString,
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
      description: `Delete a label from a team.`,
      inputSchema: {
        teamId: uuidString,
        labelId: uuidString,
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
      description: `Attach a label to an issue (teams must match).`,
      inputSchema: {
        issueId: uuidString,
        labelId: uuidString,
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
      description: `Detach a label from an issue.`,
      inputSchema: {
        issueId: uuidString,
        labelId: uuidString,
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
      description: `List comments on an issue (oldest first) by UUID or human identifier (e.g. "MET-12"). Rows include their linked attachments. The MCP user must have access to the issue's team.`,
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
        const linked =
          rows.length > 0
            ? await db
                .select({
                  id: attachments.id,
                  commentId: attachments.commentId,
                  filename: attachments.filename,
                  contentType: attachments.contentType,
                  sizeBytes: attachments.sizeBytes,
                  url: attachments.url,
                })
                .from(attachments)
                .where(
                  inArray(
                    attachments.commentId,
                    rows.map((row) => row.id)
                  )
                )
            : []
        return ok(
          rows.map((row) => ({
            ...row,
            attachments: linked
              .filter((a) => a.commentId === row.id)
              .map(({ commentId: _commentId, ...rest }) => rest),
          }))
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_comments_create`,
    {
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
      description: `Set an issue's status during a coding session (UUID or identifier). Only 'in_progress' (started working) and 'done' (work merged) are allowed. Never set 'in_review' yourself. Both exponential_pr_open and PR merges move issues to the team's configured statuses automatically.`,
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
    `exponential_statuses_list`,
    {
      description: `List a team's issue statuses (id, name, category, position). Use id as statusId in exponential_issues_update.`,
      inputSchema: { teamId: uuidString },
    },
    async ({ teamId }) => {
      try {
        // Team-level read like labels: a visible (board-granted) team
        // suffices, membership is still checked.
        assertTeamVisible(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const rows = await db
          .select({
            id: issueStatuses.id,
            name: issueStatuses.name,
            category: issueStatuses.category,
            builtinKey: issueStatuses.builtinKey,
            sortOrder: issueStatuses.sortOrder,
            createdAt: issueStatuses.createdAt,
          })
          .from(issueStatuses)
          .where(eq(issueStatuses.teamId, teamId))
        // Rule 1 of the cross-platform resolution contract
        // (lib/team-statuses.ts): category display order, then sortOrder,
        // createdAt, id.
        const rank = new Map<string, number>(
          issueStatusCategoryDisplayOrder.map((category, index) => [
            category,
            index,
          ])
        )
        rows.sort(
          (a, b) =>
            (rank.get(a.category) ?? 99) - (rank.get(b.category) ?? 99) ||
            a.sortOrder - b.sortOrder ||
            a.createdAt.getTime() - b.createdAt.getTime() ||
            (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
        )
        const positions = new Map<string, number>()
        return ok(
          rows.map((row) => {
            const position = (positions.get(row.category) ?? 0) + 1
            positions.set(row.category, position)
            return {
              id: row.id,
              name: row.name,
              category: row.category,
              position,
              builtinKey: row.builtinKey,
            }
          })
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_open`,
    {
      description: `Open a GitHub PR on the linked repository via the GitHub App (no 'gh' or token) and link it to the issue(s). Pass EXACTLY ONE of 'issueId' or 'issueIds' (batch: ONE combined PR for all listed issues, same repo; 'head' then REQUIRED, e.g. 'exp/batch-<id>'). Single issue: 'head' defaults to the issue's branch or 'exp/<IDENTIFIER>'; 'base' to the repo default branch. Linked issues record prUrl/prNumber/prState/branch and move to the team's PR-open status (default 'in_review'); merging later moves them to the PR-merge status (default 'done'). Accepts UUIDs or identifiers ("MET-12").`,
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
            `'head' is required with issueIds. Pass the pushed batch branch.`
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
              `No repository linked to this board. Link one in team settings.`
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
            !(await isInstallationLinkedToTeam(wsId, resolved.installationId))
          ) {
            throw new Error(
              `${repo.fullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`
            )
          }
        }
        const token = resolved.token

        // EXP-494: record the initiator BEFORE creating the PR — GitHub's
        // `opened` webhook reliably beats this handler's own DB write, and
        // without the claim it fans out anonymously (self-notifying the very
        // user whose agent opened the PR) whenever no coding_sessions row
        // survived to attribute to.
        claimPrOpen(repo.fullName, headBranch, {
          userId: user.id,
          viaAgent: true,
        })
        let created: Awaited<ReturnType<typeof createPullRequest>>
        try {
          created = await createPullRequest({
            repo: repo.fullName,
            head: headBranch,
            base: baseBranch,
            title,
            body: body ?? ``,
            token,
          })
        } catch (e) {
          // A failed create must not leave a claim that could misattribute a
          // later out-of-band PR on the same branch.
          releasePrOpenClaim(repo.fullName, headBranch)
          throw e
        }

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
            // The open PR moves the issue to the team's PR-open target
            // (EXP-120; default In Review, per-team configurable — EXP-319).
            if (current) {
              await applyPrLifecycleStatusInTx(tx, {
                issueId: id,
                teamId: teamIdByIssue.get(id)!,
                actorUserId: user.id,
                currentStatus: current.status,
                event: `opened`,
              })
            }
          }

          // Batch sessions carry no issue linkage (issue_id NULL), so the
          // per-issue session flip inside applyPrLifecycleStatusInTx misses
          // them — flip the CALLER's running batch session(s) in the
          // affected team(s) instead (EXP-194). The flip itself stays loose,
          // like batch runs themselves: two concurrent batch runs by the
          // same user in one team both flip on either's PR — running rows
          // are indistinguishable. But the flip STAMPS the PR's head branch
          // onto the row (EXP-545), creating the batch↔PR linkage clients
          // use to tie the row's Merge shortcut to ITS OWN PR — without it,
          // "the team's sole open batch PR" could target a teammate's PR
          // once this session's own PR closed unmerged. The caller matches
          // as owner OR host (EXP-432): on a shared server device the agent
          // authenticates with the daemon owner's key while the batch row is
          // requester-owned.
          if (issueIds?.length) {
            await tx
              .update(codingSessions)
              // needsInput resets with the flip, like the per-issue path
              // (EXP-531).
              .set({
                status: `in_review`,
                branch: headBranch,
                needsInput: false,
                updatedAt: new Date(),
              })
              .where(
                and(
                  or(
                    eq(codingSessions.userId, user.id),
                    eq(codingSessions.hostUserId, user.id)
                  ),
                  inArray(codingSessions.teamId, [
                    ...new Set(teamIdByIssue.values()),
                  ]),
                  isNull(codingSessions.issueId),
                  // Action runs are issue-less too, and one may well be
                  // running alongside the batch — never flip or stamp those
                  // (schema: `branch` is NULL on action rows).
                  isNull(codingSessions.actionId),
                  eq(codingSessions.status, `running`)
                )
              )
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
            actorViaAgent: true,
          })
        }

        return ok({ url: created.url, number: created.number })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_merge`,
    {
      description: `Squash-merge linked open PRs via the GitHub App (no 'gh' or token). Pass EXACTLY ONE of 'issueId' or 'issueIds' (one merge per distinct prUrl, so issues sharing a batch PR merge once). Linked issues flip to prState='merged' and move to the team's PR-merge status (default 'done'); live coding sessions on them end. Merges run sequentially with per-PR results; one unmergeable PR never blocks the rest. A merge rejected for a stale base: fix with exponential_pr_retarget first. Idempotent for already-merged PRs.`,
      inputSchema: {
        issueId: z.string().min(1).optional(),
        issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
      },
    },
    async ({ issueId, issueIds }) => {
      try {
        if (Boolean(issueId) === Boolean(issueIds?.length)) {
          throw new Error(`Provide exactly one of issueId or issueIds`)
        }

        // Resolve + authorize every issue up front — a scope/membership
        // violation fails the WHOLE call (never a per-item "result").
        const rawIds = issueIds ?? [issueId!]
        const ids: string[] = []
        for (const raw of rawIds) {
          const id = await resolveIssueId(raw, user.id, access)
          if (!ids.includes(id)) ids.push(id)
        }
        for (const id of ids) {
          const issueCtx = await getIssueTeamContext(id)
          assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
          await resolveTeamAccess(user.id, issueCtx.teamId)
        }

        // One merge per distinct PR: issues sharing a batch prUrl collapse
        // onto the first listed issue (merging it completes the siblings).
        const rows = await db
          .select({
            id: issues.id,
            identifier: issues.identifier,
            prUrl: issues.prUrl,
          })
          .from(issues)
          .where(inArray(issues.id, ids))
        const rowById = new Map(rows.map((row) => [row.id, row]))
        const seenPrUrls = new Set<string>()
        const targets: { id: string; identifier: string }[] = []
        for (const id of ids) {
          const row = rowById.get(id)
          // Unknown row / no linked PR: keep it as a target so the tRPC
          // mutation's own guard produces the precise per-item message.
          if (row?.prUrl) {
            if (seenPrUrls.has(row.prUrl)) continue
            seenPrUrls.add(row.prUrl)
          }
          targets.push({ id, identifier: row?.identifier ?? id })
        }

        // The tRPC mutation owns the guards (open-state, repo-from-prUrl,
        // installation link-gate) and the shared applyPrMergeState writer.
        const trpcCaller = caller(user, request)
        const results: {
          issueId: string
          identifier: string
          merged: boolean
          error?: string
        }[] = []
        for (const target of targets) {
          try {
            await trpcCaller.issues.mergePr({ issueId: target.id })
            results.push({
              issueId: target.id,
              identifier: target.identifier,
              merged: true,
            })
          } catch (e) {
            results.push({
              issueId: target.id,
              identifier: target.identifier,
              merged: false,
              error: e instanceof Error ? e.message : String(e),
            })
          }
        }
        return ok({ results })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_retarget`,
    {
      description: `Change the base branch of an issue's open PR via the GitHub App. Use it when a merge is rejected because the base is stale (e.g. stacked on an already-merged parent PR). Omit 'base' for the repo's default branch. Then rebase onto the new base, push with --force-with-lease, and call exponential_pr_merge.`,
      inputSchema: {
        issueId: z.string().min(1),
        base: z.string().min(1).max(255).optional(),
      },
    },
    async ({ issueId, base }) => {
      try {
        const id = await resolveIssueId(issueId, user.id, access)
        const issueCtx = await getIssueTeamContext(id)
        assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
        await resolveTeamAccess(user.id, issueCtx.teamId)

        // The tRPC mutation owns the guards (open-state, repo-from-prUrl,
        // installation link-gate) and the default-branch fill-in.
        const result = await caller(user, request).issues.retargetPr({
          issueId: id,
          base,
        })
        return ok({ retargeted: true, base: result.base })
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
      description: `Edit the body of an existing comment (by its UUID). Only the comment's author can edit it. Body is plain text; the edit stamps editedAt.`,
      inputSchema: {
        id: uuidString,
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
      description: `Permanently delete a comment (by its UUID). Only the comment's author can delete it.`,
      inputSchema: { id: uuidString },
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
      description: `Unsubscribe the MCP user from an issue (UUID or identifier). Suppresses auto-resubscribe until they act on the issue again.`,
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
      description: `List the MCP user's own notifications, newest first. Set unreadOnly to show only those not yet read.`,
      inputSchema: {
        unreadOnly: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      },
    },
    async ({ unreadOnly, limit, offset }) => {
      try {
        // Mirror the synced notifications shape: rows from trashed or
        // archived boards are hidden via the trigger-maintained mirrors
        // (issue-less rows keep NULL mirrors and always pass).
        const conditions = [
          eq(notifications.userId, user.id),
          isNull(notifications.boardDeletedAt),
          isNull(notifications.boardArchivedAt),
        ]
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
      description: `Mark one notification read by id, or all unread ones with all=true. Only the MCP user's own notifications are affected.`,
      inputSchema: {
        id: uuidString.optional(),
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
      description: `List the members of a team with their id, name, email, and role. Use this to resolve an assigneeId for issues.`,
      inputSchema: {
        teamId: uuidString,
      },
    },
    async ({ teamId }) => {
      try {
        assertTeamVisible(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const rows = await db
          .select({
            id: users.id,
            name: users.name,
            email: users.email,
            image: users.image,
            role: teamMembers.role,
          })
          .from(teamMembers)
          .innerJoin(users, eq(users.id, teamMembers.userId))
          .where(eq(teamMembers.teamId, teamId))
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
      description: `List the repositories registered in a team, each with the boards it backs. The MCP user must be a member of the team.`,
      inputSchema: { teamId: uuidString },
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
      description: `Register a GitHub repository ("owner/name") in a team so boards can be backed by it. Any member; the repo must be one YOUR GitHub connection grants (team settings → Repositories) — connecting shares it with the team.`,
      inputSchema: {
        teamId: uuidString,
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
      description: `Get the diff of an issue's exp/<IDENTIFIER> branch against the repo's default branch (UUID or identifier). Returns null when the branch was never pushed. Team members only.`,
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
  // Actions (per-team reusable prompts, EXP-253)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_actions_list`,
    {
      description: `List a team's actions: reusable markdown prompts run as interactive agent sessions on a member's own desktop. Team members only.`,
      inputSchema: { teamId: uuidString },
    },
    async ({ teamId }) => {
      try {
        // FULL team grant even for the read: action bodies are locally
        // executed operational prompts, not board-workflow aux data — a
        // board-confined OAuth token has no business reading them (the
        // run_configs precedent confined list the same way).
        if (!access.full) assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).actions.list({ teamId })
        return ok(result.actions)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_create`,
    {
      description: `Create a team action (owner only). body = the markdown prompt an agent runs locally; repositoryId targets that repo's trunk clone; icon = a curated icon name; inputs = run-dialog fields injected into the prompt. trigger binds an automation on deviceId (the hosting device's steer id; pass a provided trigger JSON verbatim): {kind:schedule,deviceId,enabled,interval:daily|weekly|monthly,minuteOfDay:0-1439,weekday?:1-7,dayOfMonth?:1-28} or {kind:event,deviceId,enabled,event:created|status_changed|assignee_changed|label_added|priority_changed|pr_opened|pr_merged,filters?:{boardIds?,labelIds?,priorities?,toStatusIds?}}. An enabled trigger needs every input optional.`,
      inputSchema: {
        teamId: uuidString,
        name: z.string().min(1).max(255),
        description: z.string().nullable().optional(),
        icon: boardIconEnumSchema.nullable().optional(),
        repositoryId: uuidString.nullable().optional(),
        body: z.string().min(1),
        inputs: actionInputsSchema.optional(),
        trigger: z.record(z.string(), z.unknown()).nullish(),
      },
    },
    async (input) => {
      try {
        if (!access.full) assertTeamFullyGranted(access, input.teamId)
        // Declared loose to stay inside the MCP context budget; the strict
        // union validates here (and again in the router — single source).
        const trigger = actionTriggerSchema.nullish().parse(input.trigger)
        const result = await caller(user, request).actions.create({
          ...input,
          trigger,
        })
        return ok(result.action)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_update`,
    {
      description: `Update an action by UUID (owner only); pass only fields to change. icon/trigger: null clears; inputs: whole-array replace; trigger shape as in exponential_actions_create.`,
      inputSchema: {
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        description: z.string().nullable().optional(),
        icon: boardIconEnumSchema.nullable().optional(),
        repositoryId: uuidString.nullable().optional(),
        body: z.string().min(1).optional(),
        inputs: actionInputsSchema.optional(),
        trigger: z.record(z.string(), z.unknown()).nullish(),
        sortOrder: z.number().finite().optional(),
      },
    },
    async (input) => {
      try {
        if (!access.full) {
          const action = await getActionContext(input.id)
          assertTeamFullyGranted(access, action.teamId)
        }
        // Loose in the declared schema (context budget); strict-parsed here.
        const trigger = actionTriggerSchema.nullish().parse(input.trigger)
        const result = await caller(user, request).actions.update({
          ...input,
          trigger,
        })
        return ok(result.action)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_delete`,
    {
      description: `Delete an action by its UUID. Live runs keep their action_name label and degrade to batch-shaped rows. Team owner only.`,
      inputSchema: { id: uuidString },
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const action = await getActionContext(id)
          assertTeamFullyGranted(access, action.teamId)
        }
        await caller(user, request).actions.delete({ id })
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
      description: `List the changed files (with patches and add/delete counts) of the issue's linked pull request (UUID or identifier). Empty list when no PR is linked. Team members only.`,
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
      description: `Move a board to the trash (owner only; protected boards refuse). Purged with all issues after 48 hours; owners can restore from web settings before then.`,
      inputSchema: { boardId: uuidString },
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
      description: `Point a board at a different registered repository (both must be in the same team). Owner/admin only. Existing worktrees keep working; new coding sessions use the new repo.`,
      inputSchema: {
        boardId: uuidString,
        repositoryId: uuidString,
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
      description: `Update a team's name or icon (by its UUID). Team owner only. Teams are always private.`,
      inputSchema: {
        id: uuidString,
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
      description: `Create an invite link for a team, returning the token to share. Owner only.`,
      inputSchema: {
        teamId: uuidString,
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
      description: `List the pending (unaccepted) invites for a team. The MCP user must be a member of the team.`,
      inputSchema: { teamId: uuidString },
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
      description: `Revoke a pending invite by its UUID. Owner only.`,
      inputSchema: { id: uuidString },
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
  // Attachments upload (base64 payload → S3 → attachments row)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_attachments_upload`,
    {
      description: `Upload a base64-encoded file and attach it to an issue (UUID or identifier). Images (png/jpeg/webp/gif/avif, max 10 MB) also return a "markdown" field. Embed that string to show the image. Other types (max 50 MB) land in the issue's Files list, return no markdown, and must not be embedded. Storage limits apply; base64 inflates ~33%.`,
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
      contentType: contentTypeInput,
      dataBase64,
      alt,
    }) => {
      try {
        // Canonicalized (lowercase essence) so the exact-match inline-image
        // classification behaves identically for every stored row.
        const contentType = canonicalizeContentType(contentTypeInput)
        const isImage = isAcceptedImageContentType(contentType)
        // The zod schema only checks length — strip control chars (CRLF would
        // otherwise poison the read path's Content-Disposition header).
        const filename = sanitizeUploadFilename(
          filenameInput,
          isImage ? `image` : `file`
        )
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const issueCtx = await getIssueTeamContext(issueId)
        assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
        await assertTeamMember(user.id, issueCtx.teamId)

        const body = new Uint8Array(Buffer.from(dataBase64, `base64`))
        if (body.byteLength === 0) {
          throw new Error(`Decoded file is empty. Check the base64 payload.`)
        }
        if (body.byteLength > getMaxUploadBytesForContentType(contentType)) {
          throw new Error(
            isImage
              ? `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller.`
              : `Files must be ${maxFileUploadBytes / (1024 * 1024)} MB or smaller.`
          )
        }

        await assertWithinStorageLimit(issueCtx.teamId, body.byteLength)

        const attachmentId = crypto.randomUUID()
        const storageKey = buildAttachmentStorageKey(
          issueId,
          attachmentId,
          filename
        )
        const url = buildAttachmentUrl(attachmentId)
        // Only inline images are probed — a pdf/zip/video has no pixel size.
        const dimensions = isImage ? getImageDimensions(body) : null

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
          // Non-images are NOT markdown-embeddable — they live in the issue's
          // Files list, so no markdown field is offered for them.
          ...(isImage ? { markdown: `![${alt ?? ``}](${url})` } : {}),
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

  server.registerTool(
    `exponential_attachments_delete`,
    {
      description: `Permanently delete an issue attachment by id (the {id} in /api/attachments/{id}) and reclaim its bytes. Descriptions/comments embedding it are rewritten to *(deleted image: …)* in the same transaction.`,
      inputSchema: { id: uuidString },
    },
    async ({ id }) => {
      try {
        const attachment = await getAttachmentTeamContext(id)
        assertBoardGranted(access, attachment.boardId, attachment.teamId)
        // Membership, rewrite and blob reclamation all live in the router —
        // the MCP surface must never fork that logic.
        await caller(user, request).attachments.delete({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-496: vendor bug intake. Registered only where the instance has an
  // in-app feedback widget (cloud — the same gate as the sidebar Feedback
  // button), so self-hosted agents never see the tool. Deliberately NO
  // grant/scope assertion: it writes to the vendor's own feedback board, not
  // to any of the caller's team data.
  const feedbackWidgetKey = buildRuntimeConfig().feedbackWidget?.widgetKey
  if (feedbackWidgetKey) {
    server.registerTool(
      `exponential_report_bug`,
      {
        description: `File a bug report about Exponential itself (the issue tracker) to the Exponential team. Not for issues in the user's own project.`,
        inputSchema: {
          title: z.string().min(1).max(500),
          description: z.string().min(1).max(10_000),
        },
      },
      async ({ title, description }) => {
        try {
          const limit = agentBugReportLimiter.tryTake(user.id)
          if (!limit.ok) {
            return err(
              new Error(
                `Too many bug reports — retry in ${limit.retryAfterSeconds}s`
              )
            )
          }
          const result = await createAgentBugReport({
            widgetKey: feedbackWidgetKey,
            reporter: { email: user.email, name: user.name ?? null },
            title,
            description,
            userAgent: request.headers.get(`user-agent`),
          })
          return ok(result)
        } catch (e) {
          return err(e)
        }
      }
    )
  }
}
