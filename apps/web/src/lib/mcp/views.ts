// EXP-1153: the MCP App views (SEP-1865) — what turns a tool result into UI
// in a host that renders MCP Apps (ChatGPT, Claude, VS Code…).
//
// Two halves:
//
// 1. The RESOURCES: `ui://exponential/<view>`, one self-contained HTML
//    document each, built by packages/mcp-views (`bun run build:mcp-views`)
//    into its dist/ and read from there at request time. A host fetches the
//    resource with `resources/read` when it first renders the linked tool
//    and caches it. Missing dist = a clear error, never a broken iframe.
//
// 2. The DATA a view renders: `loadBoardViewData` / `loadIssueViewData`
//    assemble the `structuredContent` the contract (@exp/mcp-views/contract)
//    describes, plus the TEXT half of the same result — what the model reads,
//    and all a client without MCP Apps ever sees. The tool handlers in
//    tools.ts do the access checks first; nothing here consults the grant.

import { existsSync, readFileSync } from "node:fs"
import { resolve } from "node:path"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray, notInArray } from "drizzle-orm"
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import {
  VIEW_MIME_TYPE,
  VIEW_NAMES,
  VIEW_TITLES,
  viewResourceUri,
  type BoardViewData,
  type IssueViewData,
  type ViewBoard,
  type ViewIssue,
  type ViewLabel,
  type ViewName,
  type ViewStatus,
  type ViewUser,
} from "@exp/mcp-views/contract"
import { CATEGORY_ANCHOR } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import {
  boards,
  comments,
  issueLabels,
  issueStatuses,
  issues,
  labels,
  teams,
} from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"
import { issueWireColumns } from "@/lib/issue-columns"
import {
  loadIssueRelations,
  type IssueRelationView,
} from "@/lib/issue-relations"
import {
  buildStatusOptions,
  resolveIssueStatus,
  type StatusRowOption,
} from "@/lib/team-statuses"

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

const htmlCache = new Map<ViewName, string>()

/** Where the built views live: an explicit dir, else the monorepo's
 *  packages/mcp-views/dist from the repo root or from apps/web. */
function viewsDistDir(): string | null {
  const candidates = [
    process.env.MCP_VIEWS_DIR,
    resolve(process.cwd(), `packages/mcp-views/dist`),
    resolve(process.cwd(), `../../packages/mcp-views/dist`),
  ].filter((p): p is string => Boolean(p))
  return candidates.find((dir) => existsSync(dir)) ?? null
}

export function loadViewHtml(view: ViewName): string {
  const cached = htmlCache.get(view)
  if (cached) return cached
  const dir = viewsDistDir()
  const file = dir ? resolve(dir, `${view}.html`) : null
  if (!file || !existsSync(file)) {
    throw new Error(
      `MCP view "${view}" is not built: run \`bun run build:mcp-views\` (or set MCP_VIEWS_DIR)`
    )
  }
  const html = readFileSync(file, `utf8`)
  // Cache in production only; a dev server picks up a rebuild on the next read.
  if (process.env.NODE_ENV === `production`) htmlCache.set(view, html)
  return html
}

/** The tool `_meta` that links a tool to its view: the MCP Apps field
 *  (ChatGPT reads the standard one; its `openai/outputTemplate` alias is the
 *  legacy Apps SDK spelling and stays out of the budget), and optionally the
 *  ChatGPT thread-panel entrypoint (the tool must then accept `{}`). */
export function viewToolMeta(
  view: ViewName,
  opts: { threadEntrypoint?: boolean } = {}
): Record<string, unknown> {
  const resourceUri = viewResourceUri(view)
  return {
    ui: { resourceUri, visibility: [`model`, `app`] },
    ...(opts.threadEntrypoint
      ? { "openai/ui": { entrypoints: [{ type: `thread` }] } }
      : {}),
  }
}

/** Register every view as a `ui://` resource on a real McpServer. */
export function registerViewResources(server: McpServer) {
  for (const view of VIEW_NAMES) {
    const uri = viewResourceUri(view)
    server.registerResource(
      `exponential-${view}-view`,
      uri,
      {
        title: VIEW_TITLES[view],
        description: `The Exponential ${view} view (MCP App).`,
        mimeType: VIEW_MIME_TYPE,
      },
      async () => ({
        contents: [{ uri, mimeType: VIEW_MIME_TYPE, text: loadViewHtml(view) }],
      })
    )
  }
}

/** A view tool's result: the text for the model, the data for the view. */
export function viewResult(text: string, data: unknown) {
  return {
    content: [{ type: `text` as const, text }],
    structuredContent: data as Record<string, unknown>,
  }
}

export function viewBaseUrl(): string {
  return process.env.BETTER_AUTH_URL?.replace(/\/$/, ``) ?? ``
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

const CLOSED_ANCHORS = [`completed`, `cancelled`, `duplicate`].map(
  (category) => CATEGORY_ANCHOR[category as keyof typeof CATEGORY_ANCHOR]
)
const BOARD_VIEW_MAX_ISSUES = 200
const ISSUE_VIEW_MAX_COMMENTS = 20

function toViewStatus(option: StatusRowOption): ViewStatus {
  return {
    id: option.id,
    name: option.name,
    color: option.colorHex,
    category: option.category,
    icon: option.icon,
  }
}

async function loadTeamStatuses(teamId: string): Promise<StatusRowOption[]> {
  const rows = await db
    .select({
      id: issueStatuses.id,
      name: issueStatuses.name,
      category: issueStatuses.category,
      color: issueStatuses.color,
      builtinKey: issueStatuses.builtinKey,
      sortOrder: issueStatuses.sortOrder,
      createdAt: issueStatuses.createdAt,
    })
    .from(issueStatuses)
    .where(eq(issueStatuses.teamId, teamId))
  return buildStatusOptions(rows)
}

async function loadViewBoard(boardId: string, baseUrl: string): Promise<ViewBoard & { teamId: string }> {
  const [board] = await db
    .select({
      id: boards.id,
      teamId: boards.teamId,
      name: boards.name,
      slug: boards.slug,
      prefix: boards.prefix,
      color: boards.color,
      icon: boards.icon,
      repositoryId: boards.repositoryId,
      teamSlug: teams.slug,
    })
    .from(boards)
    .innerJoin(teams, eq(teams.id, boards.teamId))
    .where(and(eq(boards.id, boardId), boardVisible()))
    .limit(1)
  if (!board) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Board not found` })
  }
  return {
    id: board.id,
    teamId: board.teamId,
    name: board.name,
    prefix: board.prefix,
    color: board.color,
    icon: board.icon,
    repositoryId: board.repositoryId,
    url: `${baseUrl}/t/${board.teamSlug}/boards/${board.slug}`,
  }
}

async function loadUsers(ids: readonly string[]): Promise<Map<string, ViewUser>> {
  const wanted = [...new Set(ids)]
  if (wanted.length === 0) return new Map()
  const rows = await db
    .select({ id: users.id, name: users.name, email: users.email })
    .from(users)
    .where(inArray(users.id, wanted))
  // No `image`: the view's CSP names no picture origin, and Google avatars
  // live on one; initials render everywhere.
  return new Map(rows.map((row) => [row.id, { id: row.id, name: row.name, email: row.email }]))
}

async function loadLabels(issueIds: readonly string[]): Promise<Map<string, ViewLabel[]>> {
  if (issueIds.length === 0) return new Map()
  const rows = await db
    .select({
      issueId: issueLabels.issueId,
      id: labels.id,
      name: labels.name,
      color: labels.color,
    })
    .from(issueLabels)
    .innerJoin(labels, eq(labels.id, issueLabels.labelId))
    .where(inArray(issueLabels.issueId, [...issueIds]))
  const map = new Map<string, ViewLabel[]>()
  for (const row of rows) {
    const list = map.get(row.issueId) ?? []
    list.push({ id: row.id, name: row.name, color: row.color })
    map.set(row.issueId, list)
  }
  return map
}

type IssueRow = Pick<typeof issues.$inferSelect, keyof typeof issueWireColumns>

function toViewIssue(
  row: IssueRow,
  status: StatusRowOption,
  assignee: ViewUser | null,
  issueLabelsOf: ViewLabel[],
  boardUrl: string
): ViewIssue {
  return {
    id: row.id,
    identifier: row.identifier,
    title: row.title,
    priority: row.priority,
    statusId: status.id,
    assignee,
    labels: issueLabelsOf,
    pr: row.prState
      ? { state: row.prState, url: row.prUrl ?? null, number: row.prNumber ?? null }
      : null,
    dueDate: row.dueDate ?? null,
    url: `${boardUrl}/issues/${row.identifier}`,
  }
}

/** The board view: the board, the team's statuses, its OPEN issues. */
export async function loadBoardViewData(
  boardId: string,
  baseUrl: string
): Promise<{ data: BoardViewData; text: string }> {
  const board = await loadViewBoard(boardId, baseUrl)
  const options = await loadTeamStatuses(board.teamId)
  const byId = new Map(options.map((o) => [o.id, o]))
  const rows = await db
    .select(issueWireColumns)
    .from(issues)
    .where(
      and(eq(issues.boardId, board.id), notInArray(issues.status, CLOSED_ANCHORS))
    )
    .orderBy(desc(issues.createdAt))
    .limit(BOARD_VIEW_MAX_ISSUES)
  const labelMap = await loadLabels(rows.map((r) => r.id))
  const userMap = await loadUsers(
    rows.map((r) => r.assigneeId).filter((id): id is string => Boolean(id))
  )
  const seen = new Set<string>()
  const viewIssues = rows.map((row) => {
    const status = resolveIssueStatus(row, options, byId)
    seen.add(status.id)
    return toViewIssue(
      row,
      status,
      row.assigneeId ? (userMap.get(row.assigneeId) ?? null) : null,
      labelMap.get(row.id) ?? [],
      board.url
    )
  })
  const { teamId: _teamId, ...viewBoard } = board
  const data: BoardViewData = {
    board: viewBoard,
    // Every team row (so an empty group still names its status) in display order.
    statuses: options.map(toViewStatus),
    issues: viewIssues,
  }
  const lines = viewIssues.map((issue) => {
    const status = byId.get(issue.statusId)?.name ?? `?`
    const who = issue.assignee ? ` @${issue.assignee.email ?? issue.assignee.name}` : ``
    return `${issue.identifier}  ${issue.title}  [${status}${issue.priority !== `none` ? `, ${issue.priority}` : ``}]${who}`
  })
  const text =
    `${board.name} (${board.prefix}): ${viewIssues.length} open issue${viewIssues.length === 1 ? `` : `s`}` +
    (viewIssues.length === BOARD_VIEW_MAX_ISSUES ? ` (first ${BOARD_VIEW_MAX_ISSUES})` : ``) +
    `\n${board.url}\n\n${lines.join(`\n`)}`
  return { data, text }
}

/** The issue view: one issue with comments, relations and its PR. `keepRelation`
 *  is the caller's grant filter on the far side of each relation. */
export async function loadIssueViewData(
  issueId: string,
  baseUrl: string,
  keepRelation: (relation: IssueRelationView) => boolean
): Promise<{ data: IssueViewData; text: string }> {
  const [row] = await db
    .select(issueWireColumns)
    .from(issues)
    .where(eq(issues.id, issueId))
    .limit(1)
  if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
  const board = await loadViewBoard(row.boardId, baseUrl)
  const options = await loadTeamStatuses(board.teamId)
  const status = resolveIssueStatus(row, options)
  const labelMap = await loadLabels([row.id])
  const commentRows = await db
    .select({
      id: comments.id,
      authorId: comments.authorId,
      body: comments.body,
      createdAt: comments.createdAt,
      source: comments.source,
    })
    .from(comments)
    .where(eq(comments.issueId, row.id))
    .orderBy(desc(comments.createdAt))
    .limit(ISSUE_VIEW_MAX_COMMENTS)
  const userMap = await loadUsers([
    ...(row.assigneeId ? [row.assigneeId] : []),
    ...commentRows.map((c) => c.authorId).filter((id): id is string => Boolean(id)),
  ])
  const relations = (await loadIssueRelations(db, row.id)).filter(keepRelation)
  const otherIds = relations.map((r) => r.otherIssueId)
  const titles = new Map<string, string>(
    otherIds.length === 0
      ? []
      : (
          await db
            .select({ id: issues.id, title: issues.title })
            .from(issues)
            .where(inArray(issues.id, otherIds))
        ).map((r) => [r.id, r.title])
  )
  const assignee = row.assigneeId ? (userMap.get(row.assigneeId) ?? null) : null
  const base = toViewIssue(row, status, assignee, labelMap.get(row.id) ?? [], board.url)
  const { teamId: _teamId, ...viewBoard } = board
  const data: IssueViewData = {
    issue: {
      ...base,
      description: row.description ?? null,
      estimate: row.estimate ?? null,
      createdAt: toIso(row.createdAt),
      updatedAt: toIso(row.updatedAt),
    },
    status: toViewStatus(status),
    board: viewBoard,
    comments: commentRows.map((c) => ({
      id: c.id,
      author: c.authorId ? (userMap.get(c.authorId) ?? null) : null,
      body: c.body,
      createdAt: toIso(c.createdAt),
      source: c.source,
    })),
    relations: relations.map((r) => ({
      type: r.type,
      direction: r.direction,
      otherIdentifier: r.otherIdentifier,
      otherTitle: titles.get(r.otherIssueId) ?? null,
    })),
  }
  const text = [
    `${base.identifier}: ${base.title}`,
    `Status: ${status.name}${base.priority !== `none` ? ` · Priority: ${base.priority}` : ``}${assignee ? ` · Assignee: ${assignee.name ?? assignee.email}` : ``}${base.pr ? ` · PR ${base.pr.state}${base.pr.url ? ` ${base.pr.url}` : ``}` : ``}`,
    base.labels.length > 0 ? `Labels: ${base.labels.map((l) => l.name).join(`, `)}` : ``,
    base.url,
    ``,
    row.description?.trim() || `(no description)`,
    ``,
    data.comments.length > 0
      ? `${data.comments.length} comment${data.comments.length === 1 ? `` : `s`} (newest first):\n` +
        data.comments
          .map((c) => `- ${c.author?.name ?? c.author?.email ?? `someone`}: ${c.body}`)
          .join(`\n`)
      : `No comments.`,
  ]
    .filter((line) => line !== undefined)
    .join(`\n`)
  return { data, text }
}

function toIso(value: Date | string | null | undefined): string {
  if (!value) return ``
  return value instanceof Date ? value.toISOString() : new Date(value).toISOString()
}
