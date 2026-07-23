import { z } from "zod"
import { useWebMCP } from "@mcp-b/react-webmcp"
import {
  boardCollection,
  commentCollection,
  issueCollection,
  issueLabelCollection,
  labelCollection,
  notificationCollection,
  teamMemberCollection,
  userCollection,
} from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import {
  dateOnlySchema,
  issuePrioritySchema,
  issueStatusSchema,
} from "@/lib/domain"
import { getWebMcpAppContext } from "@/lib/webmcp/app-context"
import {
  resolveAssignee,
  resolveBoard,
  resolveIssue,
  resolveLabels,
  serializeBoard,
  serializeComment,
  serializeIssue,
} from "@/lib/webmcp/resolve"

// WebMCP tool surface (EXP-245): one useWebMCP call per tool, registered for
// the lifetime of the mounted component. Handlers read the app-context store
// and collection snapshots at CALL time (never React closures), so a tool
// invoked long after registration still acts on what's currently on screen.
// Reads come straight from the synced Electric collections; writes go through
// the same tRPC mutations the UI uses — failures surface both to the agent
// (thrown message → isError result) and to the human (mutation error toast).

// Boards shape only syncs live boards (`deleted_at IS NULL` where clause);
// the extra filter just guards optimistic leftovers.
async function getTeamBoards(teamId: string) {
  await boardCollection.preload()
  return boardCollection.toArray.filter(
    (board) => board.teamId === teamId && !board.deletedAt
  )
}

// The issues shape excludes `team_id` (REV2-5 scoping column), so team
// scoping goes through the board ids like every web view does.
async function getTeamIssues(teamId: string) {
  const boards = await getTeamBoards(teamId)
  const boardById = new Map(boards.map((board) => [board.id, board]))
  await issueCollection.preload()
  const issues = issueCollection.toArray.filter((issue) =>
    boardById.has(issue.boardId)
  )
  return { boards, boardById, issues }
}

async function getTeamMembers(teamId: string) {
  await Promise.all([teamMemberCollection.preload(), userCollection.preload()])
  const memberIds = new Set(
    teamMemberCollection.toArray
      .filter((member) => member.teamId === teamId)
      .map((member) => member.userId)
  )
  return userCollection.toArray.filter((user) => memberIds.has(user.id))
}

async function getTeamLabels(teamId: string) {
  await labelCollection.preload()
  return labelCollection.toArray.filter((label) => label.teamId === teamId)
}

async function getUserNameById() {
  await userCollection.preload()
  return new Map(userCollection.toArray.map((user) => [user.id, user.name]))
}

async function getLabelNamesByIssue() {
  await Promise.all([issueLabelCollection.preload(), labelCollection.preload()])
  const labelById = new Map(
    labelCollection.toArray.map((label) => [label.id, label.name])
  )
  const byIssue = new Map<string, string[]>()
  for (const issueLabel of issueLabelCollection.toArray) {
    const name = labelById.get(issueLabel.labelId)
    if (!name) continue
    const names = byIssue.get(issueLabel.issueId) ?? []
    names.push(name)
    byIssue.set(issueLabel.issueId, names)
  }
  return byIssue
}

function issueUrl(teamSlug: string, boardSlug: string, identifier: string) {
  return `/t/${teamSlug}/boards/${boardSlug}/issues/${identifier}`
}

const issueRefSchema = z
  .string()
  .optional()
  .describe(`Issue identifier like EXP-42; defaults to the issue open on screen`)
const boardRefSchema = z
  .string()
  .optional()
  .describe(`Board slug (or exact name); defaults to the board open on screen`)
const assigneeRefSchema = z
  .string()
  .nullable()
  .describe(`Team member email or display name; null to unassign`)

const readAnnotations = { readOnlyHint: true, idempotentHint: true }

export function WebMcpReadTools() {
  useWebMCP({
    name: `get_context`,
    description: `Get what is currently open in the Exponential issue tracker: team, board, issue on screen, and the signed-in user. Call this first to orient.`,
    annotations: readAnnotations,
    handler: async () => {
      const ctx = getWebMcpAppContext()
      const boards = await getTeamBoards(ctx.teamId)
      const board = ctx.boardSlug
        ? (boards.find((b) => b.slug === ctx.boardSlug) ?? null)
        : null
      return {
        team: { slug: ctx.teamSlug, name: ctx.teamName },
        board: board ? { slug: board.slug, name: board.name } : null,
        issue: ctx.issueIdentifier,
        user: { name: ctx.userName, email: ctx.userEmail },
        isMember: ctx.isMember,
        path: window.location.pathname,
      }
    },
  })

  useWebMCP({
    name: `list_boards`,
    description: `List the boards in the current team with their slugs and issue counts.`,
    annotations: readAnnotations,
    handler: async () => {
      const ctx = getWebMcpAppContext()
      const { boards, issues } = await getTeamIssues(ctx.teamId)
      const countByBoard = new Map<string, number>()
      for (const issue of issues) {
        if (issue.archivedAt) continue
        countByBoard.set(
          issue.boardId,
          (countByBoard.get(issue.boardId) ?? 0) + 1
        )
      }
      return {
        boards: boards.map((board) =>
          serializeBoard(board, countByBoard.get(board.id) ?? 0)
        ),
      }
    },
  })

  useWebMCP({
    name: `list_issues`,
    description: `List issues on a board of the current team (newest-updated first). Ignores any filters active in the UI. Archived issues are excluded.`,
    inputSchema: {
      board: boardRefSchema,
      statuses: z
        .array(issueStatusSchema)
        .optional()
        .describe(`Only these statuses`),
      assignee: z
        .string()
        .optional()
        .describe(`Only issues assigned to this member (email or name)`),
      limit: z.number().int().min(1).max(100).optional(),
    },
    annotations: readAnnotations,
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      const { boards, issues } = await getTeamIssues(ctx.teamId)
      const board = resolveBoard(input.board, ctx.boardSlug, boards)
      let rows = issues.filter(
        (issue) => issue.boardId === board.id && !issue.archivedAt
      )
      if (input.statuses && input.statuses.length > 0) {
        const wanted = new Set(input.statuses)
        rows = rows.filter((issue) => wanted.has(issue.status))
      }
      if (input.assignee !== undefined) {
        const assigneeId = resolveAssignee(
          input.assignee,
          await getTeamMembers(ctx.teamId)
        )
        rows = rows.filter((issue) => issue.assigneeId === assigneeId)
      }
      rows = [...rows].sort(
        (a, b) => b.updatedAt.getTime() - a.updatedAt.getTime()
      )
      const limit = input.limit ?? 50
      const [labelNames, userNames] = await Promise.all([
        getLabelNamesByIssue(),
        getUserNameById(),
      ])
      return {
        board: board.slug,
        total: rows.length,
        issues: rows
          .slice(0, limit)
          .map((issue) =>
            serializeIssue(issue, labelNames.get(issue.id) ?? [], userNames)
          ),
      }
    },
  })

  useWebMCP({
    name: `search_issues`,
    description: `Full-text search across all issues of the current team (titles, descriptions, comments).`,
    inputSchema: {
      query: z.string().min(1),
      limit: z.number().int().min(1).max(50).optional(),
    },
    annotations: readAnnotations,
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      const [results, boards] = await Promise.all([
        trpc.issues.search.query({
          teamId: ctx.teamId,
          query: input.query,
          limit: input.limit ?? 20,
        }),
        getTeamBoards(ctx.teamId),
      ])
      const boardSlugById = new Map(boards.map((b) => [b.id, b.slug]))
      return {
        results: results.map((row) => ({
          identifier: row.identifier,
          title: row.title,
          status: row.status,
          priority: row.priority,
          board: boardSlugById.get(row.boardId) ?? null,
        })),
      }
    },
  })

  useWebMCP({
    name: `get_issue`,
    description: `Get the full detail of one issue: fields, description (GFM markdown), labels, and comment thread.`,
    inputSchema: { issue: issueRefSchema },
    annotations: readAnnotations,
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      const { boardById, issues } = await getTeamIssues(ctx.teamId)
      const issue = resolveIssue(input.issue, ctx.issueIdentifier, issues)
      await commentCollection.preload()
      const [labelNames, userNames] = await Promise.all([
        getLabelNamesByIssue(),
        getUserNameById(),
      ])
      const comments = commentCollection.toArray
        .filter((comment) => comment.issueId === issue.id)
        .sort((a, b) => a.createdAt.getTime() - b.createdAt.getTime())
      const boardSlug = boardById.get(issue.boardId)?.slug ?? null
      return {
        ...serializeIssue(issue, labelNames.get(issue.id) ?? [], userNames),
        description: issue.description,
        board: boardSlug,
        url: boardSlug
          ? issueUrl(ctx.teamSlug, boardSlug, issue.identifier)
          : null,
        comments: comments.map((comment) =>
          serializeComment(comment, userNames)
        ),
      }
    },
  })

  useWebMCP({
    name: `list_notifications`,
    description: `List the signed-in user's inbox notifications (newest first, max 50).`,
    inputSchema: {
      unreadOnly: z
        .boolean()
        .optional()
        .describe(`Default true — only unread notifications`),
    },
    annotations: readAnnotations,
    handler: async (input) => {
      await Promise.all([
        notificationCollection.preload(),
        issueCollection.preload(),
      ])
      const unreadOnly = input.unreadOnly ?? true
      const identifierByIssue = new Map(
        issueCollection.toArray.map((issue) => [issue.id, issue.identifier])
      )
      const rows = notificationCollection.toArray
        .filter((n) => (unreadOnly ? !n.readAt : true))
        .sort((a, b) => b.createdAt.getTime() - a.createdAt.getTime())
        .slice(0, 50)
      return {
        notifications: rows.map((n) => ({
          id: n.id,
          type: n.type,
          title: n.title,
          body: n.body,
          issue: n.issueId ? (identifierByIssue.get(n.issueId) ?? null) : null,
          read: Boolean(n.readAt),
          createdAt: n.createdAt.toISOString(),
        })),
      }
    },
  })

  useWebMCP({
    name: `navigate`,
    description: `Navigate the app to a board, an issue, the notification inbox, or the PR reviews view of the current team. Changes what the user sees; modifies no data.`,
    inputSchema: {
      to: z.enum([`board`, `issue`, `inbox`, `reviews`]),
      board: boardRefSchema,
      issue: issueRefSchema,
    },
    annotations: { idempotentHint: true },
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      if (input.to === `inbox` || input.to === `reviews`) {
        ctx.navigate({ kind: input.to })
        return { navigated: input.to }
      }
      if (input.to === `board`) {
        const board = resolveBoard(
          input.board,
          ctx.boardSlug,
          await getTeamBoards(ctx.teamId)
        )
        ctx.navigate({ kind: `board`, boardSlug: board.slug })
        return { navigated: `board`, board: board.slug }
      }
      const { boardById, issues } = await getTeamIssues(ctx.teamId)
      const issue = resolveIssue(input.issue, ctx.issueIdentifier, issues)
      const boardSlug = boardById.get(issue.boardId)?.slug
      if (!boardSlug) {
        throw new Error(`Board for issue ${issue.identifier} is not available`)
      }
      ctx.navigate({
        kind: `issue`,
        boardSlug,
        issueIdentifier: issue.identifier,
      })
      return { navigated: `issue`, issue: issue.identifier }
    },
  })

  return null
}

export function WebMcpWriteTools() {
  useWebMCP({
    name: `create_issue`,
    description: `Create a new issue on a board of the current team.`,
    inputSchema: {
      board: boardRefSchema,
      title: z.string().min(1).max(500),
      description: z
        .string()
        .optional()
        .describe(`GFM markdown (no embedded images)`),
      status: issueStatusSchema.optional().describe(`Default backlog`),
      priority: issuePrioritySchema.optional().describe(`Default none`),
      assignee: z
        .string()
        .optional()
        .describe(`Team member email or display name`),
      dueDate: dateOnlySchema.optional().describe(`YYYY-MM-DD`),
      labels: z.array(z.string()).optional().describe(`Existing label names`),
    },
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      const board = resolveBoard(
        input.board,
        ctx.boardSlug,
        await getTeamBoards(ctx.teamId)
      )
      const assigneeId =
        input.assignee === undefined
          ? undefined
          : resolveAssignee(input.assignee, await getTeamMembers(ctx.teamId))
      const labelIds =
        input.labels && input.labels.length > 0
          ? resolveLabels(input.labels, await getTeamLabels(ctx.teamId)).map(
              (label) => label.id
            )
          : undefined
      const result = await trpc.issues.create.mutate({
        boardId: board.id,
        title: input.title,
        description: input.description,
        status: input.status,
        priority: input.priority,
        assigneeId,
        dueDate: input.dueDate,
        labelIds,
      })
      return {
        created: result.issue.identifier,
        url: issueUrl(ctx.teamSlug, board.slug, result.issue.identifier),
      }
    },
  })

  useWebMCP({
    name: `update_issue`,
    description: `Update fields of an issue in the current team. Only the fields you pass change.`,
    inputSchema: {
      issue: issueRefSchema,
      title: z.string().min(1).max(500).optional(),
      status: issueStatusSchema.optional(),
      priority: issuePrioritySchema.optional(),
      assignee: assigneeRefSchema.optional(),
      description: z
        .string()
        .nullable()
        .optional()
        .describe(`GFM markdown; null clears it`),
      dueDate: dateOnlySchema.nullable().optional().describe(`YYYY-MM-DD; null clears it`),
    },
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      const { issues } = await getTeamIssues(ctx.teamId)
      const issue = resolveIssue(input.issue, ctx.issueIdentifier, issues)
      const assigneeId =
        input.assignee === undefined
          ? undefined
          : resolveAssignee(input.assignee, await getTeamMembers(ctx.teamId))
      await trpc.issues.update.mutate({
        id: issue.id,
        ...(input.title !== undefined && { title: input.title }),
        ...(input.status !== undefined && { status: input.status }),
        ...(input.priority !== undefined && { priority: input.priority }),
        ...(assigneeId !== undefined && { assigneeId }),
        ...(input.description !== undefined && {
          description: input.description,
        }),
        ...(input.dueDate !== undefined && { dueDate: input.dueDate }),
      })
      return { updated: issue.identifier }
    },
  })

  useWebMCP({
    name: `comment_on_issue`,
    description: `Add a comment to an issue in the current team. The comment is posted as the signed-in user.`,
    inputSchema: {
      issue: issueRefSchema,
      body: z.string().min(1).describe(`GFM markdown`),
    },
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      const { issues } = await getTeamIssues(ctx.teamId)
      const issue = resolveIssue(input.issue, ctx.issueIdentifier, issues)
      await trpc.comments.create.mutate({
        issueId: issue.id,
        body: input.body,
      })
      return { commented: issue.identifier }
    },
  })

  useWebMCP({
    name: `change_issue_labels`,
    description: `Add and/or remove labels on an issue in the current team. Labels must already exist (see list_issues output or team settings).`,
    inputSchema: {
      issue: issueRefSchema,
      add: z.array(z.string()).optional().describe(`Label names to add`),
      remove: z.array(z.string()).optional().describe(`Label names to remove`),
    },
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      const { issues } = await getTeamIssues(ctx.teamId)
      const issue = resolveIssue(input.issue, ctx.issueIdentifier, issues)
      const teamLabels = await getTeamLabels(ctx.teamId)
      const toAdd = resolveLabels(input.add ?? [], teamLabels)
      const toRemove = resolveLabels(input.remove ?? [], teamLabels)
      for (const label of toAdd) {
        await trpc.issueLabels.add.mutate({
          issueId: issue.id,
          labelId: label.id,
        })
      }
      for (const label of toRemove) {
        await trpc.issueLabels.remove.mutate({
          issueId: issue.id,
          labelId: label.id,
        })
      }
      return {
        issue: issue.identifier,
        added: toAdd.map((label) => label.name),
        removed: toRemove.map((label) => label.name),
      }
    },
  })

  useWebMCP({
    name: `set_issue_subscription`,
    description: `Subscribe or unsubscribe the signed-in user to an issue's notifications.`,
    inputSchema: {
      issue: issueRefSchema,
      subscribed: z.boolean(),
    },
    annotations: { idempotentHint: true },
    handler: async (input) => {
      const ctx = getWebMcpAppContext()
      const { issues } = await getTeamIssues(ctx.teamId)
      const issue = resolveIssue(input.issue, ctx.issueIdentifier, issues)
      if (input.subscribed) {
        await trpc.subscriptions.subscribe.mutate({ issueId: issue.id })
      } else {
        await trpc.subscriptions.unsubscribe.mutate({ issueId: issue.id })
      }
      return { issue: issue.identifier, subscribed: input.subscribed }
    },
  })

  useWebMCP({
    name: `mark_notifications_read`,
    description: `Mark one notification (by id from list_notifications) or ALL notifications as read.`,
    inputSchema: {
      id: z
        .string()
        .optional()
        .describe(`Notification id; omit to mark everything read`),
    },
    annotations: { idempotentHint: true },
    handler: async (input) => {
      if (input.id !== undefined) {
        await trpc.notifications.markRead.mutate({ id: input.id })
        return { markedRead: input.id }
      }
      await trpc.notifications.markAllRead.mutate()
      return { markedRead: `all` }
    },
  })

  return null
}
