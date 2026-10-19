import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { contract } from "@exp/domain-contract"
import {
  actionInputsSchema,
  CATEGORY_ANCHOR,
  customizableStatusCategoryValues,
  dateOnlySchema,
  DEFAULT_ACCENT_COLOR,
  hexColorSchema,
  issueEstimateSchema,
  issueEstimationValues,
  MAX_ISSUE_DESCRIPTION,
  MAX_MCP_SERVER_NAME,
  MESSAGE_BODY_MAX,
  PLAN_CARD_DESCRIPTION_MAX,
  PLAN_CARD_LABEL_KEY_MAX,
  PLAN_CARD_LABEL_MAX,
  PLAN_CARD_OPTIONS_MAX,
  PLAN_CARD_PICTURES_MAX,
  PLAN_CARD_PROMPT_MAX,
  PLAN_CARD_QUESTIONS_MAX,
  pictureLabelSchema,
  QUESTION_CARD_OPTIONS_MAX,
  SESSION_RESULT_REPORT_MAX,
  SESSIONS_SEND_TEXT_MAX,
  sessionKindValues,
  sessionsSendInputSchema,
  REPLIES_MAX,
  SUGGESTIONS_MAX,
  replyInputSchema,
  suggestionInputSchema,
  UUID_RE,
} from "@exp/db-schema/domain"
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import {
  and,
  asc,
  desc,
  eq,
  gte,
  inArray,
  isNull,
  lte,
  notInArray,
  or,
  sql,
  type SQL,
} from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { db } from "@/db/connection"
import {
  actions,
  attachments,
  comments,
  devices,
  issueEvents,
  issueLabels,
  issueRelations,
  issues,
  issueStatuses,
  labels,
  mcpServers,
  messages,
  notifications,
  boards,
  sessions,
  users,
  teamInvites,
  teamMembers,
  teams,
} from "@/db/schema"
import {
  commentAudienceValues,
  issuePriorityValues,
  issueRelationTypeValues,
  issueStatusValues,
  issueStatusCategoryDisplayOrder,
  issueStatusCategoryValues,
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
import {
  canonicalizeRelation,
  loadIssueRelations,
} from "@/lib/issue-relations"
import { resolveIssueReference } from "@/lib/issue-resolver"
import {
  issueWireColumns,
  withTruncatedDescriptions,
} from "@/lib/issue-columns"
import { deleteObject, getObject, uploadObject } from "@/lib/storage"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  canonicalizeContentType,
  getMaxUploadBytesForContentType,
  isAcceptedImageContentType,
  isInlineMediaContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import {
  getVideoMetadata,
  isProbeableVideoContentType,
} from "@/lib/storage/video-metadata"
import { mintAttachmentToken } from "@/lib/storage/attachment-token"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { assertWithinStorageLimit } from "@/lib/billing"
import { appRouter } from "@/routes/api/trpc/$"
import type { Context } from "@/lib/trpc"
import { activityWire } from "@/lib/activity/wire"
import { noteAgentIssueActivity } from "@/lib/integrations/pr-actor-claims"
import { composeDeviceList } from "@/lib/steer-devices"
import {
  ISSUE_SEARCH_SCAN_CAP,
  issueSearchMatchIds,
} from "@/lib/issue-search-sql"
import { buildRuntimeConfig } from "@/lib/runtime-config"
import { createAgentBugReport } from "@/lib/widget/agent-report"
import { TokenBucketLimiter } from "@/lib/widget/rate-limit"
import { loadRepositoryForTeam } from "@/lib/trpc/repositories"
import { visibleDeviceRows } from "@/lib/trpc/devices"
// EXP-1291: the session tree's core (lib/sessions/**) and the PR model
// (lib/pull-requests.ts). These tools only add grants and resolve the caller.
import { sendSession } from "@/lib/sessions/send"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"
import { getSessionTree, listSessions } from "@/lib/sessions/tree"
import { endSession, stopSession } from "@/lib/sessions/lifecycle"
import { linkIssues, unlinkIssue } from "@/lib/sessions/issues"
import { writeGuide } from "@/lib/sessions/guide"
import { postReplies, postSuggestions } from "@/lib/sessions/suggestions"
import { buildPlanCard, buildQuestionCard, postAskCard } from "@/lib/sessions/plan"
import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"
import {
  closePullRequestTx,
  mergePullRequest,
  openPullRequest,
  pullRequestFiles,
  resolvePullRequest,
  retargetPullRequest,
  updatePullRequest,
} from "@/lib/pull-requests"
import { err, ok } from "./helpers"
import { inlineImageForContext } from "./inline-image"
// EXP-988: the per-tool handler files. The registry owns the registration,
// the zod schema and the access checks; each file owns ONE tool's behaviour.
import {
  countIssueAttachments,
  listIssueAttachments,
} from "./handlers/attachments-list"
import {
  assertAttachmentUploadAccess,
  finalizeSignedAttachmentUpload,
  mintSignedAttachmentUpload,
} from "./handlers/attachments-upload"
import { requestSessionCompaction } from "./handlers/sessions-compact"
import { deviceAccountLogin } from "./handlers/device-account-login"
import { ALWAYS_LOAD_META } from "./always-load"
import { mcpAppToolMeta, type McpAppView } from "./apps"
import {
  ALL_MCP_TOOL_GATES,
  hostFeedsOwnCards,
  type McpToolGates,
} from "./gates"
import type { McpUser } from "./server"
import {
  assertFullAccess,
  assertBoardGranted,
  assertTeamFullyGranted,
  assertTeamVisible,
  filterVisibleTeamIds,
  grantScopeFilter,
  isBoardGranted,
  isTeamVisible,
  GRANT_MATCHES_NOTHING,
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

// Resolve a UUID or human identifier ("MET-12") to an issue UUID via the
// shared resolver (lib/issue-resolver.ts — EXP-707: one resolver, both
// layers, deterministic newest-wins), intersected with the connection's
// OAuth grant. The team-level access check still runs in the caller — this
// only maps the friendly identifier the coding agent knows to the row id.
async function resolveIssueId(
  idOrIdentifier: string,
  userId: string,
  access: McpAccess
): Promise<string> {
  if (UUID_RE.test(idOrIdentifier)) return idOrIdentifier
  let grantedBoardIds: string[] | undefined
  if (!access.full) {
    const teamIds = await getUserTeamIds(userId)
    const boardRows =
      teamIds.length > 0
        ? await db
            .select({ id: boards.id, teamId: boards.teamId })
            .from(boards)
            .where(and(inArray(boards.teamId, teamIds), boardVisible()))
        : []
    grantedBoardIds = boardRows
      .filter((r) => isBoardGranted(access, r.id, r.teamId))
      .map((r) => r.id)
  }
  return resolveIssueReference(userId, idOrIdentifier, { grantedBoardIds })
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

// Label id → its team (EXP-707: row mutations never require a derivable
// teamId — the MCP layer derives it for the router).
async function getLabelContext(id: string) {
  const [row] = await db
    .select({ teamId: labels.teamId })
    .from(labels)
    .where(eq(labels.id, id))
    .limit(1)
  if (!row) throw new Error(`Label not found`)
  return row
}

// Status id → its team (same derivation rule as labels).
async function getStatusContext(id: string) {
  const [row] = await db
    .select({ teamId: issueStatuses.teamId })
    .from(issueStatuses)
    .where(eq(issueStatuses.id, id))
    .limit(1)
  if (!row) throw new Error(`Status not found`)
  return row
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

// EXP-1291: the caller's own session (the X-Exp-Session-Id row) as the tools
// need it: its thread root, whether that root is a team agent, and whether
// nobody is watching (a child, or a root another session spawned).
interface CallerSession {
  id: string
  teamId: string
  rootId: string
  rootKind: string
  unattended: boolean
  /** EXP-1280: the session plans (`exponential_pr_open` refuses). */
  planMode: boolean
  /** EXP-1280: the host device feeds a card the session posts on ITSELF
   *  back as its next message (`hostFeedsOwnCards`). */
  cards: boolean
}


// EXP-1280: `hostFeedsOwnCards` lives with the gates (gates.ts resolves
// `cards` from the same devices join); re-exported for the tests.
export { hostFeedsOwnCards }

/** What `exponential_plan_propose` / `exponential_sessions_ask` answer on
 *  such a host: the agent asks inline instead of waiting on a card. */
export const CARDS_NEED_HOST_UPDATE = `This session's device runs a build that cannot deliver a card's answer (it needs 0.14.73 or later): ask the question in your reply and end your turn instead.`
/** What a session-scoped tool answers a caller with no session of its own. */
function noSessionMessage(tool: string, param?: string): string {
  return `No session: ${tool} ${param ? `needs ${param} outside` : `only works inside`} a session started by the Exponential launcher (missing X-Exp-Session-Id).`
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === `object` && value !== null && !Array.isArray(value)
}

const codingAgentValues = contract.codingAgent.values as [string, ...string[]]

// EXP-704: a text-ish attachment at or under this size ALSO comes back inline
// in attachments_get's JSON payload (anything bigger — or any other type —
// rides only the signed downloadUrl, which has no size cap).
const MAX_INLINE_TEXT_BYTES = 32 * 1024

// EXP-705: every tool takes a STRICT object — an unknown key is an immediate
// "unrecognized key" error the agent can self-correct on, never a silent drop.
// Tool defs are deferred behind tool search, so agents guess param names from
// adjacent evidence; the server is the only party that always knows the shape.
// Serializes as additionalProperties:false (gated by api-conventions.test.ts).
const strictInput = <S extends z.ZodRawShape>(shape: S) => z.strictObject(shape)

// EXP-1280: the plan / question card's input parts (caps = contract
// `planCard`; the behaviour = fixtures/plan-card.json).
const cardPicturesInput = z
  .array(
    z.strictObject({
      label: pictureLabelSchema,
      caption: z.string().max(PLAN_CARD_DESCRIPTION_MAX).optional(),
    })
  )
  .max(PLAN_CARD_PICTURES_MAX)
  .optional()
const cardOptionInput = z.strictObject({
  label: z.string().trim().min(1).max(PLAN_CARD_LABEL_MAX),
  description: z.string().max(PLAN_CARD_DESCRIPTION_MAX).optional(),
  picture: pictureLabelSchema.optional(),
})
const cardSurfaceInput = z
  .strictObject({
    root: z.record(z.string(), z.unknown()),
    data: z.record(z.string(), z.unknown()).optional(),
  })
  .optional()

// EXP-1251 → EXP-1291: the Guide tool's description (always-loaded bytes).
const GUIDE_TOOL_DESCRIPTION = `Your thread's GUIDE = the body of every PR in it. Per topic: text (2-3 GFM sentences) + files (touched paths); 'Summary' first. Sections together cover every changed file; unassigned files land in Other changes. prUrl = the topic's PR. label = picture: uploadUrl + curl (PNG/JPEG/WebP, 10 MB). remove drops a label, text (text: '') or topic.`
// EXP-1262: the call only mints the upload grant; nothing renders until the
// curl runs.
export const GUIDE_PICTURE_NEXT_STEP = `Run the curl line now: the picture shows only once it uploads.`

// FEED-25: every read declares MCP's `readOnlyHint`. Without it claude's
// plan mode (and any "ask before side effects" posture) raises a permission
// card for a plain `*_get`/`*_list` — the batch run behind FEED-25 sat on one
// such card for two hours. Gated by api-conventions.test.ts: reads carry it,
// nothing else does.
const READ_ONLY = { readOnlyHint: true } as const

// EXP-847: what "closed" means for `exponential_issues_list`'s default —
// the three terminal status CATEGORIES and the anchors they dual-write
// (CATEGORY_ANCHOR), so the filter works on custom statuses too.
const CLOSED_STATUS_CATEGORIES = [
  `completed`,
  `cancelled`,
  `duplicate`,
] as const satisfies ReadonlyArray<(typeof issueStatusCategoryValues)[number]>
const CLOSED_STATUS_ANCHORS = CLOSED_STATUS_CATEGORIES.map(
  (category) => CATEGORY_ANCHOR[category]
)

// EXP-707: the ONE pagination model — every *_list tool declares limit/offset
// (default 50, cap 200; gated by api-conventions.test.ts). Small-table tools
// slice after their existing filters rather than in SQL.
const pageInput = {
  limit: z.number().int().min(1).max(200).default(50),
  offset: z.number().int().min(0).default(0),
}
// Defaults repeated here because the zod defaults live in the schema layer —
// a caller invoking a handler directly (the test harness) bypasses them.
const page = <T>(rows: T[], limit?: number, offset?: number) =>
  rows.slice(offset ?? 0, (offset ?? 0) + (limit ?? 50))

// EXP-707: MCP reads ship the same pinned columns as the Electric shapes —
// never a bare select() that would leak the REV2-5/EXP-500 scoping mirrors
// (or any future server-only column) to agents unreviewed. Issues use the
// shared lib/issue-columns.ts mirror; these are the camelCase mirrors of the
// other shapes' allowlists (routes/api/shapes/*).
const boardWireColumns = {
  id: boards.id,
  teamId: boards.teamId,
  name: boards.name,
  slug: boards.slug,
  prefix: boards.prefix,
  color: boards.color,
  icon: boards.icon,
  repositoryId: boards.repositoryId,
  sortOrder: boards.sortOrder,
  createdAt: boards.createdAt,
  updatedAt: boards.updatedAt,
}
const commentWireColumns = {
  id: comments.id,
  issueId: comments.issueId,
  teamId: comments.teamId,
  boardId: comments.boardId,
  authorId: comments.authorId,
  parentId: comments.parentId,
  source: comments.source,
  body: comments.body,
  editedAt: comments.editedAt,
  createdAt: comments.createdAt,
  updatedAt: comments.updatedAt,
}
const notificationWireColumns = {
  id: notifications.id,
  userId: notifications.userId,
  issueId: notifications.issueId,
  teamId: notifications.teamId,
  type: notifications.type,
  title: notifications.title,
  body: notifications.body,
  readAt: notifications.readAt,
  pushedAt: notifications.pushedAt,
  createdAt: notifications.createdAt,
  updatedAt: notifications.updatedAt,
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
const DATE_ONLY_RE = /^\d{4}-\d{2}-\d{2}$/
const dateOnly = dateOnlySchema
const issueEstimate = issueEstimateSchema
// Same contract, no inline pattern (budget, see looseEnum below).
const dateOnlyLoose = z
  .string()
  .refine((v) => DATE_ONLY_RE.test(v), `Expected YYYY-MM-DD`)
// EXP-684: created/updated range bounds. Anything Date.parse accepts — a bare
// YYYY-MM-DD reads as midnight UTC, so "createdAfter: 2026-08-29" is the
// whole of that day onward.
const isoDateTime = z
  .string()
  .refine((v) => !Number.isNaN(Date.parse(v)), `Expected an ISO date or datetime`)
// Enum validated at runtime only (no inline JSON-schema enum) — the budget
// trick above, for a value list the same tool already spells out once.
const looseEnum = <T extends string>(values: ReadonlyArray<T>) =>
  z
    .string()
    .refine(
      (v) => (values as ReadonlyArray<string>).includes(v),
      `Expected one of: ${values.join(`, `)}`
    )
    .transform((v) => v as T)
const issueListSortFields = [`createdAt`, `updatedAt`, `priority`] as const
const issueListSort = z
  .string()
  .refine(
    (v) =>
      (issueListSortFields as ReadonlyArray<string>).includes(
        v.replace(/^-/, ``)
      ),
    `Expected createdAt, updatedAt or priority, optionally -prefixed`
  )
  .default(`-createdAt`)
// Sort rank for priority — the pg enum is declared none-first, which is not
// an order anyone wants to sort by.
const issuePriorityRank = sql<number>`case ${issues.priority} when 'urgent' then 4 when 'high' then 3 when 'medium' then 2 when 'low' then 1 else 0 end`

// EXP-1183: shared by exponential_issues_list and exponential_issues_show.
const issuesListInput = strictInput({
  boardId: uuidString.optional(),
  boardIds: z.array(uuidString).optional(),
  teamId: uuidString.optional(),
  status: z.array(looseEnum(issueStatusValues)).optional(),
  statusId: z.array(uuidString).optional(),
  statusCategory: z.array(looseEnum(issueStatusCategoryValues)).optional(),
  excludeStatus: z.array(looseEnum(issueStatusValues)).optional(),
  excludeStatusId: z.array(uuidString).optional(),
  excludeStatusCategory: z
    .array(looseEnum(issueStatusCategoryValues))
    .optional(),
  includeClosed: z.boolean().default(false),
  priority: z.array(looseEnum(issuePriorityValues)).optional(),
  assigneeId: z.string().nullable().optional(),
  source: looseEnum([`user`, `widget`] as const).optional(),
  labelIds: z.array(uuidString).optional(),
  labelMatch: z.enum([`any`, `all`]).optional(),
  unlabeled: z.boolean().optional(),
  hasComments: z.boolean().optional(),
  commentedBy: z.string().optional(),
  notCommentedBy: z.string().optional(),
  createdAfter: isoDateTime.optional(),
  createdBefore: isoDateTime.optional(),
  updatedAfter: isoDateTime.optional(),
  updatedBefore: isoDateTime.optional(),
  dueAfter: dateOnlyLoose.optional(),
  dueBefore: dateOnlyLoose.optional(),
  search: z
    .string()
    .refine((v) => v.length >= 1 && v.length <= 256, `1-256 chars`)
    .optional(),
  sort: issueListSort,
  limit: z.number().int().min(1).max(1000).default(50),
  offset: z.number().int().min(0).default(0),
})



export function registerExponentialTools(
  server: McpServer,
  user: McpUser,
  request: Request,
  access: McpAccess,
  // EXP-637: the `sessions` row this MCP request runs inside, parsed
  // from the launcher-injected X-Exp-Session-Id header. Null for every caller
  // that is not a launched agent.
  sessionId: string | null = null,
  // EXP-660: which conditional tool families register for this caller
  // (resolved per request by the route). Defaults to everything so tests and
  // the context budget see the whole surface.
  gates: McpToolGates = ALL_MCP_TOOL_GATES
) {
  // EXP-1212: a coding run gets no MCP Apps binding (server.ts registers no
  // `ui://` resource for it either); the tools answer with their JSON.
  const appMeta = (view: McpAppView) =>
    sessionId ? undefined : mcpAppToolMeta(view)
  // EXP-637 → EXP-1291: the header names a `sessions` row. It counts only
  // when it is really THIS caller's session — its starter, or the device
  // owner hosting a teammate's start (EXP-432: the hosting daemon's key
  // authenticates the agent). A foreign or vanished id resolves to null and
  // every tool degrades to its header-less behaviour; the header is an
  // identifier, never a credential.
  async function loadCallerSession(): Promise<CallerSession | null> {
    if (!sessionId) return null
    const root = alias(sessions, `root`)
    // The host device's row (devices are per (user, deviceId)): its
    // reported version says whether it feeds the session's own cards.
    const host = alias(devices, `host`)
    const [row] = await db
      .select({
        id: sessions.id,
        teamId: sessions.teamId,
        rootId: sessions.rootId,
        parentId: sessions.parentId,
        userId: sessions.userId,
        hostUserId: sessions.hostUserId,
        rootKind: root.kind,
        rootSpawnedBy: root.spawnedBySessionId,
        planMode: sessions.planMode,
        hostVersion: host.version,
      })
      .from(sessions)
      .leftJoin(root, eq(root.id, sessions.rootId))
      .leftJoin(
        host,
        and(
          eq(host.deviceId, sessions.deviceId),
          eq(
            host.userId,
            sql`coalesce(${sessions.hostUserId}, ${sessions.userId})`
          )
        )
      )
      .where(eq(sessions.id, sessionId))
      .limit(1)
    if (!row) return null
    if (row.userId !== user.id && row.hostUserId !== user.id) return null
    return {
      id: row.id,
      teamId: row.teamId,
      rootId: row.rootId,
      rootKind: row.rootKind ?? `thread`,
      unattended: row.parentId !== null || row.rootSpawnedBy != null,
      planMode: row.planMode === true,
      cards: hostFeedsOwnCards(row.hostVersion),
    }
  }



  // -----------------------------------------------------------------------
  // Teams
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_teams_list`,
    {
      annotations: READ_ONLY,
      description: `List teams the MCP user is a member of.`,
      inputSchema: strictInput({ ...pageInput }),
    },
    async ({ limit, offset }) => {
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
        return ok(
          page(
            memberRows.filter((row) => isTeamVisible(access, row.id)),
            limit,
            offset
          )
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_teams_get`,
    {
      annotations: READ_ONLY,
      description: `Get a single team by id.`,
      inputSchema: strictInput({ id: uuidString }),
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
      annotations: READ_ONLY,
      description: `List boards in a team, or across all teams the user belongs to.`,
      inputSchema: strictInput({
        teamId: uuidString.optional(),
        ...pageInput,
      }),
    },
    async ({ teamId, limit, offset }) => {
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
          .select(boardWireColumns)
          .from(boards)
          .where(and(inArray(boards.teamId, allowedTeamIds), boardVisible()))
          .orderBy(asc(boards.sortOrder), asc(boards.name))

        const filtered = rows.filter((row) =>
          isBoardGranted(access, row.id, row.teamId)
        )
        return ok(page(filtered, limit, offset))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_get`,
    {
      annotations: READ_ONLY,
      description: `Get a single board by id.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        const board = await getBoardTeamId(id)
        assertBoardGranted(access, board.id, board.teamId)
        await resolveTeamAccess(user.id, board.teamId)
        const [row] = await db
          .select(boardWireColumns)
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
      description: `Create a board in a team (member; owner/admin to connect a new repo). The repository is optional. Coding features gate on repo presence. Pass repository.repositoryId (registry repo) or repository.fullName ("owner/name") to connect one inline; defaultBranch pins the branch this board's coding sessions branch from and its PRs target (omit = the repo's default). icon is a curated icon name.`,
      inputSchema: strictInput({
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
        color: hexColorSchema.optional(),
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
        defaultBranch: z.string().min(1).max(255).optional(),
      }),
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
      description: `Update a board's name, color, icon, or defaultBranch (the branch its coding sessions branch from and its PRs target; null = follow the repo's default).`,
      inputSchema: strictInput({
        id: uuidString,
        icon: boardIconEnumSchema.nullable().optional(),
        name: z.string().min(1).max(255).optional(),
        color: hexColorSchema.optional(),
        defaultBranch: z.string().min(1).max(255).nullable().optional(),
      }),
    },
    async ({ id, ...rest }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(id)
          assertBoardGranted(access, board.id, board.teamId)
        }
        const result = await caller(user, request).boards.update({
          boardId: id,
          ...rest,
        })
        return ok(result.board)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Issues
  // -----------------------------------------------------------------------

  const listIssues = async ({
    boardId,
    boardIds,
    teamId,
    status,
    statusId,
    statusCategory,
    excludeStatus,
    excludeStatusId,
    excludeStatusCategory,
    includeClosed,
    priority,
    assigneeId,
    source,
    labelIds,
    labelMatch,
    unlabeled,
    hasComments,
    commentedBy,
    notCommentedBy,
    createdAfter,
    createdBefore,
    updatedAfter,
    updatedBefore,
    dueAfter,
    dueBefore,
    search,
    sort,
    limit,
    offset,
  }: z.output<typeof issuesListInput>) => {
    try {
      let allowedBoardIds: Array<string>

      // EXP-684: boardIds is the multi-board form of boardId — every named
      // board is access-checked exactly like the single one.
      const requestedBoardIds = boardId
        ? [boardId]
        : boardIds && boardIds.length > 0
          ? [...new Set(boardIds)]
          : null

      if (requestedBoardIds) {
        for (const id of requestedBoardIds) {
          const board = await getBoardTeamId(id)
          assertBoardGranted(access, board.id, board.teamId)
          await resolveTeamAccess(user.id, board.teamId)
        }
        allowedBoardIds = requestedBoardIds
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

      const conditions: Array<SQL> = [
        inArray(issues.boardId, allowedBoardIds),
      ]

      // Status: the builtin anchor enum, the precise per-team row, or the
      // row's category (a custom "Ideas" has no builtin key, so only the
      // latter two can name it). status_id is trigger-populated for every
      // writer (populate_issue_status_id), so the row filters key on it
      // alone; an exclude keeps the (theoretical) NULL row rather than
      // dropping it into nowhere.
      const statusIdsInCategories = (
        categories: Array<(typeof issueStatusCategoryValues)[number]>
      ) =>
        sql`(select ${issueStatuses.id} from ${issueStatuses} where ${inArray(issueStatuses.category, categories)})`
      if (status && status.length > 0) {
        conditions.push(inArray(issues.status, status))
      }
      if (statusId && statusId.length > 0) {
        conditions.push(inArray(issues.statusId, statusId))
      }
      if (statusCategory && statusCategory.length > 0) {
        conditions.push(
          sql`${issues.statusId} in ${statusIdsInCategories(statusCategory)}`
        )
      }
      if (excludeStatus && excludeStatus.length > 0) {
        conditions.push(notInArray(issues.status, excludeStatus))
      }
      if (excludeStatusId && excludeStatusId.length > 0) {
        conditions.push(
          or(
            isNull(issues.statusId),
            notInArray(issues.statusId, excludeStatusId)
          )!
        )
      }
      if (excludeStatusCategory && excludeStatusCategory.length > 0) {
        conditions.push(
          or(
            isNull(issues.statusId),
            sql`${issues.statusId} not in ${statusIdsInCategories(excludeStatusCategory)}`
          )!
        )
      }
      // EXP-847: a listing is about OPEN work. With no status filter of any
      // kind the closed categories drop out — `includeClosed: true` (or any
      // explicit status/statusId/statusCategory) asks for them back. The
      // predicate keys on the status ROW's category (customs included) and
      // falls back to the dual-written anchor for a (theoretical) NULL
      // status_id, so nothing is silently hidden or silently kept.
      const statusFiltered =
        (status && status.length > 0) ||
        (statusId && statusId.length > 0) ||
        (statusCategory && statusCategory.length > 0)
      if (!includeClosed && !statusFiltered) {
        conditions.push(
          or(
            sql`${issues.statusId} not in ${statusIdsInCategories([
              ...CLOSED_STATUS_CATEGORIES,
            ])}`,
            and(
              isNull(issues.statusId),
              notInArray(issues.status, [...CLOSED_STATUS_ANCHORS])
            )
          )!
        )
      }

      if (priority && priority.length > 0) {
        conditions.push(inArray(issues.priority, priority))
      }
      if (assigneeId === null) {
        conditions.push(isNull(issues.assigneeId))
      } else if (assigneeId !== undefined) {
        conditions.push(eq(issues.assigneeId, assigneeId))
      }
      // SLOP-4: widget-filed reports vs member-created issues.
      if (source) {
        conditions.push(eq(issues.source, source))
      }

      // Labels: any-of / all-of over issue_labels, plus the explicit
      // "no labels at all" the triage sweeps key on.
      const labelLink = sql`select 1 from ${issueLabels} where ${issueLabels.issueId} = ${issues.id}`
      if (unlabeled === true) {
        conditions.push(sql`not exists (${labelLink})`)
      } else if (unlabeled === false) {
        conditions.push(sql`exists (${labelLink})`)
      }
      if (labelIds && labelIds.length > 0) {
        const wanted = [...new Set(labelIds)]
        if (labelMatch === `all`) {
          conditions.push(
            sql`(select count(distinct ${issueLabels.labelId}) from ${issueLabels} where ${issueLabels.issueId} = ${issues.id} and ${inArray(issueLabels.labelId, wanted)}) = ${wanted.length}`
          )
        } else {
          conditions.push(
            sql`exists (${labelLink} and ${inArray(issueLabels.labelId, wanted)})`
          )
        }
      }

      // Comments: presence, and "has/hasn't this user already replied"
      // so a recurring run does not comment on the same issue twice.
      const commentLink = sql`select 1 from ${comments} where ${comments.issueId} = ${issues.id}`
      if (hasComments === true) {
        conditions.push(sql`exists (${commentLink})`)
      } else if (hasComments === false) {
        conditions.push(sql`not exists (${commentLink})`)
      }
      if (commentedBy) {
        conditions.push(
          sql`exists (${commentLink} and ${eq(comments.authorId, commentedBy)})`
        )
      }
      if (notCommentedBy) {
        conditions.push(
          sql`not exists (${commentLink} and ${eq(comments.authorId, notCommentedBy)})`
        )
      }

      if (createdAfter) {
        conditions.push(gte(issues.createdAt, new Date(createdAfter)))
      }
      if (createdBefore) {
        conditions.push(lte(issues.createdAt, new Date(createdBefore)))
      }
      if (updatedAfter) {
        conditions.push(gte(issues.updatedAt, new Date(updatedAfter)))
      }
      if (updatedBefore) {
        conditions.push(lte(issues.updatedAt, new Date(updatedBefore)))
      }
      if (dueAfter) conditions.push(gte(issues.dueDate, dueAfter))
      if (dueBefore) conditions.push(lte(issues.dueDate, dueBefore))
      // EXP-892: the same full-text + identifier predicate every client's
      // search box runs (lib/issue-search-sql.ts), over the boards this
      // call may see. Capped: unlike the team-scoped search box this spans
      // EVERY granted board, and a limit-1000 list must not scan them all.
      if (search) {
        conditions.push(
          sql`${issues.id} in (${issueSearchMatchIds(
            search,
            { boardIds: allowedBoardIds },
            { limit: ISSUE_SEARCH_SCAN_CAP }
          )})`
        )
      }

      const dir = sort.startsWith(`-`) ? desc : asc
      const sortField = sort.replace(/^-/, ``)
      const sortExpr =
        sortField === `updatedAt`
          ? issues.updatedAt
          : sortField === `priority`
            ? issuePriorityRank
            : issues.createdAt

      const rows = await db
        .select(issueWireColumns)
        .from(issues)
        .where(and(...conditions))
        // createdAt then id break ties so pages never overlap.
        .orderBy(dir(sortExpr), dir(issues.createdAt), dir(issues.id))
        .limit(limit)
        .offset(offset)

      // EXP-847: list rows carry a description HEAD, never whole bodies.
      return ok(withTruncatedDescriptions(rows))
    } catch (e) {
      return err(e)
    }
  }

  server.registerTool(
    `exponential_issues_list`,
    {
      annotations: READ_ONLY,
      // EXP-684: every filter a scheduled sweep needs server-side. The schema
      // is budget-trimmed (context-budget.test.ts): EXP-847 took the last two
      // inline value lists out too, so status/statusCategory, their exclude*
      // twins and priority all validate at runtime (the refusal names the
      // values; issues_create spells the status enum out).
      description: `List issues, OPEN only: completed/cancelled/duplicate need includeClosed or a status* filter. Descriptions cut at 200. statusId: exponential_statuses_list; exclude* invert. created*/updated*: ISO. sort: [-]createdAt|updatedAt|priority. search: text + identifier; assigneeId null = unassigned; source: user|widget.`,
      inputSchema: issuesListInput,
    },
    listIssues
  )

  // EXP-1183: the issue list as an MCP Apps view (lib/mcp/apps.ts) — its
  // own small tool because exponential_issues_list has no per-tool budget
  // left for the `_meta.ui` binding. Same query, same rows.
  server.registerTool(
    `exponential_issues_show`,
    {
      // The app's name in an MCP Apps host's catalog (OpenClaw's Apps page).
      title: `Exponential issues`,
      annotations: READ_ONLY,
      description: `Show issues as an interactive list where the client renders MCP Apps (OpenClaw, Claude, ChatGPT); elsewhere the same JSON as exponential_issues_list. OPEN only unless includeClosed.`,
      _meta: appMeta(`issues`),
      inputSchema: strictInput({
        boardId: uuidString.optional(),
        teamId: uuidString.optional(),
        assigneeId: z.string().nullable().optional(),
        search: z
          .string()
          .refine((v) => v.length >= 1 && v.length <= 256, `1-256 chars`)
          .optional(),
        includeClosed: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
      }),
    },
    (args) => listIssues(issuesListInput.parse({ ...args, sort: `-updatedAt` }))
  )

  server.registerTool(
    `exponential_issues_get`,
    {
      annotations: READ_ONLY,
      description: `Get a single issue by UUID or identifier (e.g. "MET-12"), including its label ids and latest comments (newest first, capped at 50; commentsLimit overrides).`,
      // EXP-1183: no MCP Apps view here — the always-loaded set has no room
      // for the `_meta.ui` binding (context-budget.test.ts); the issues view
      // opens an issue itself through the host's tools/call.
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        id: z.string().min(1),
        commentsLimit: z.number().int().min(0).max(200).optional(),
      }),
    },
    async ({ id: idInput, commentsLimit }) => {
      try {
        const id = await resolveIssueId(idInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(id)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        const [issue] = await db
          .select(issueWireColumns)
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
            parentId: comments.parentId,
            source: comments.source,
            body: comments.body,
            createdAt: comments.createdAt,
            editedAt: comments.editedAt,
          })
          .from(comments)
          .where(eq(comments.issueId, id))
          .orderBy(desc(comments.createdAt))
          .limit(commentsLimit ?? 50)
        // EXP-736: both sides of the relation graph, folded to this issue's
        // point of view (direction + otherIdentifier). loadIssueRelations
        // already drops rows whose far board is trashed or archived; a
        // grant-confined token additionally sees only the boards its consent
        // named, so the far side is filtered here rather than leaking an
        // identifier from a board this connection was never granted.
        const relations = (await loadIssueRelations(db, id)).filter(
          (relation) =>
            isBoardGranted(access, relation.otherBoardId, relation.otherTeamId)
        )
        // EXP-1183: the issue's page in the app (the MCP Apps view's "Open").
        const [slugs] = await db
          .select({ teamSlug: teams.slug, boardSlug: boards.slug })
          .from(boards)
          .innerJoin(teams, eq(teams.id, boards.teamId))
          .where(eq(boards.id, ctxIssue.boardId))
          .limit(1)
        const origin = process.env.BETTER_AUTH_URL
          ? appBaseUrl()
          : new URL(request.url).origin
        return ok({
          ...issue,
          url: slugs
            ? `${origin}/t/${encodeURIComponent(slugs.teamSlug)}/boards/${encodeURIComponent(slugs.boardSlug)}/issues/${encodeURIComponent(issue.identifier)}`
            : null,
          labelIds: labelRows.map((r) => r.labelId),
          relations,
          recentComments,
          // EXP-988/EXP-979: how many files the issue carries, so an agent
          // knows whether exponential_attachments_list is worth a call.
          attachmentCount: await countIssueAttachments(id),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_create`,
    {
      description: `Create a new issue in a board the MCP user has access to. Description must be plain text (no embedded images on creation). For a custom status pass statusId (not status); see exponential_statuses_list. parentId files it as a sub-issue of that issue. sessionId links it to that session's thread (default: your own session's thread).`,
      inputSchema: strictInput({
        boardId: uuidString,
        title: z.string().min(1).max(500),
        parentId: uuidString.optional(),
        sessionId: uuidString.optional(),
        status: issueStatusEnumSchema.optional(),
        statusId: uuidString.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        description: z
          .string()
          .max(MAX_ISSUE_DESCRIPTION)
          .nullable()
          .optional()
          .describe(`Plain GFM text; no embedded images on creation`),
        dueDate: dateOnly.nullable().optional(),
        estimate: issueEstimate.nullable().optional(),
        labelIds: z.array(uuidString).optional(),
      }),
    },
    async ({ description, sessionId: sessionIdArg, ...rest }) => {
      try {
        const board = await getBoardTeamId(rest.boardId)
        if (!access.full) {
          assertBoardGranted(access, board.id, board.teamId)
          if (rest.parentId) {
            const parent = await getIssueTeamContext(rest.parentId)
            assertBoardGranted(access, parent.boardId, parent.teamId)
          }
        }
        // EXP-1291: the thread the issue is filed into. A named session is
        // checked BEFORE the issue exists (so a bad id never leaves an issue
        // filed nowhere the agent meant); the caller's own thread is the
        // default when it is a thread of the board's team (never an agent
        // channel).
        const own = await loadCallerSession()
        let rootId: string | null = null
        if (sessionIdArg) {
          const target = await loadSessionForUser(user.id, sessionIdArg)
          if (!access.full) assertTeamFullyGranted(access, target.teamId)
          await assertCanSteer(user.id, target)
          if (target.teamId !== board.teamId) {
            throw new Error(`Session not found in this board's team`)
          }
          rootId = target.rootId
        } else if (own && own.teamId === board.teamId && own.rootKind === `thread`) {
          rootId = own.rootId
        }
        const result = await caller(user, request).issues.create({
          ...rest,
          description: description ? description : undefined,
        })
        // EXP-617: exclusion-only (see noteAgentIssueActivity).
        noteAgentIssueActivity(result.issue.id, user.id)
        if (!rootId) return ok(result.issue)
        // The issue exists either way; a failed link says so instead of
        // failing the call.
        try {
          await db.transaction((tx) =>
            linkIssues(tx, {
              rootId: rootId!,
              issueIds: [result.issue.id],
              source: `agent`,
              addedBy: user.id,
              addedBySessionId: own?.id ?? null,
              filed: true,
            })
          )
          return ok({ ...result.issue, sessionId: rootId })
        } catch (e) {
          return ok({
            ...result.issue,
            warning: `Created, but not linked to session ${rootId} (${e instanceof Error ? e.message : String(e)}). Retry with exponential_sessions_link.`,
          })
        }
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_update`,
    {
      description: `Update an issue's fields (by UUID or identifier, e.g. "MET-12"). Pass only the fields you want to change. For a custom status pass statusId (not status); see exponential_statuses_list.`,
      inputSchema: strictInput({
        id: z.string().min(1),
        title: z.string().min(1).max(500).optional(),
        status: issueStatusEnumSchema.optional(),
        statusId: uuidString.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        description: z
          .string()
          .max(MAX_ISSUE_DESCRIPTION)
          .nullable()
          .optional()
          .describe(`Plain GFM text; null clears`),
        dueDate: dateOnly.nullable().optional(),
        estimate: issueEstimate.nullable().optional(),
      }),
    },
    async ({ id: idOrIdentifier, ...rest }) => {
      try {
        const id = await resolveIssueId(idOrIdentifier, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).issues.update({
          id,
          ...rest,
        })
        noteAgentIssueActivity(id, user.id)
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_delete`,
    {
      description: `Permanently delete an issue (by UUID or identifier). Cascades to its labels, attachments, comments, and relations. Attachment storage objects are also removed.`,
      inputSchema: strictInput({ id: z.string().min(1) }),
    },
    async (rawInput) => {
      try {
        const input = {
          id: await resolveIssueId(rawInput.id, user.id, access),
        }
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

  // EXP-988/EXP-979: the issue's Files list. Access = attachments_get's rule
  // (grant on the issue's board + team membership); the query lives in the
  // handler file. Metadata only — bytes ride exponential_attachments_get.
  server.registerTool(
    `exponential_attachments_list`,
    {
      annotations: READ_ONLY,
      description: `List an issue's attachments (UUID or identifier), newest first: id, filename, contentType, sizeBytes, createdAt and the commentId when a file hangs on a comment. Metadata only; exponential_attachments_get fetches one file's bytes. Paged with limit/offset; total counts every row.`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ issueId: issueIdInput, limit, offset }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(issueId)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        return ok(await listIssueAttachments({ issueId, limit, offset }))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_attachments_get`,
    {
      annotations: READ_ONLY,
      description: `Fetch an attachment by id — every content type. Markdown embeds look like ![alt](/api/attachments/{id}); pass that {id}. Always returns metadata plus a short-lived signed downloadUrl: fetch it (curl/wget) into your working directory to read non-image files (xlsx, PDF, CSV, ...) with real tooling. Images additionally come back as inline image content, downscaled when large (the JSON's inline field reports what was sent; downloadUrl always has the original); small text files include their text inline.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        // Issue attachments first; EXP-1291: a conversation message's file
        // (`attachments.message_id`) shares the /api/attachments/{id} url
        // shape, so an agent's fetch of a message embed resolves here too,
        // gated by the message's session visibility.
        const attachment = await getAttachmentTeamContext(id).then(
          (issueAttachment) => {
            assertBoardGranted(
              access,
              issueAttachment.boardId,
              issueAttachment.teamId
            )
            return issueAttachment
          },
          async (error: unknown) => {
            if (!(error instanceof TRPCError && error.code === `NOT_FOUND`)) {
              throw error
            }
            const [messageAttachment] = await db
              .select({
                teamId: attachments.teamId,
                sessionId: messages.sessionId,
                storageKey: attachments.storageKey,
                contentType: attachments.contentType,
                filename: attachments.filename,
                sizeBytes: attachments.sizeBytes,
                durationMs: attachments.durationMs,
              })
              .from(attachments)
              .innerJoin(messages, eq(messages.id, attachments.messageId))
              .where(eq(attachments.id, id))
              .limit(1)
            if (!messageAttachment) throw error
            await loadSessionForUser(user.id, messageAttachment.sessionId)
            assertTeamFullyGranted(access, messageAttachment.teamId)
            return messageAttachment
          }
        )
        await resolveTeamAccess(user.id, attachment.teamId)

        // EXP-704: the token is minted only after the grant + membership
        // checks above, bound to this one attachment — the URL is a complete
        // credential the route accepts without a session, so any agent
        // (launcher or external MCP client) can download any content type
        // with no size cap and no base64 in context. The origin is
        // `appBaseUrl()` like every other outbound link: behind a TLS-
        // terminating proxy Bun sees plain HTTP, so the request origin mints
        // an `http://` URL that a plain `curl -o` (no `-L`) saves the
        // redirect body of. Falls back to the request origin only when
        // BETTER_AUTH_URL is unset (the caller reached us on this host, so
        // that URL is reachable too).
        const { token, expiresAt } = mintAttachmentToken(id, user.id)
        const origin = process.env.BETTER_AUTH_URL
          ? appBaseUrl()
          : new URL(request.url).origin
        const payload: Record<string, unknown> = {
          id,
          filename: attachment.filename,
          contentType: attachment.contentType,
          sizeBytes: attachment.sizeBytes,
          downloadUrl: `${origin}/api/attachments/${id}?token=${token}`,
          expiresAt: expiresAt.toISOString(),
        }
        if (`durationMs` in attachment && attachment.durationMs != null) {
          payload.durationMs = attachment.durationMs
        }

        const contentType = attachment.contentType
        // EXP-853 review: the inline copy goes through sharp, so classify
        // with the SAME accepted-image set the uploads use — a stored
        // `image/svg+xml` is an `image/` type sharp would hand to librsvg.
        const isImage = isAcceptedImageContentType(contentType)
        const isTextLike =
          contentType.startsWith(`text/`) ||
          contentType === `application/json` ||
          contentType.endsWith(`+json`) ||
          contentType === `application/csv`

        if (isTextLike && attachment.sizeBytes <= MAX_INLINE_TEXT_BYTES) {
          const object = await getObject(attachment.storageKey)
          if (!object?.Body) throw new Error(`Attachment object not found`)
          const bytes = await object.Body.transformToByteArray()
          payload.text = Buffer.from(bytes).toString(`utf8`)
        }

        if (isImage) {
          const object = await getObject(attachment.storageKey)
          if (!object?.Body) throw new Error(`Attachment object not found`)
          const bytes = await object.Body.transformToByteArray()
          // EXP-854: the inline copy is bounded (1280 px / 300 KB WebP) —
          // base64 of a raw 12 MP screenshot is tens of thousands of tokens
          // out of the window the run still needs for its work. The original
          // stays one `downloadUrl` fetch away. A null here means the bytes
          // could not be decoded (or sharp is unavailable): inline the
          // original rather than send no picture at all (EXP-511).
          const inlined = await inlineImageForContext(bytes, contentType)
          const inlineBytes = inlined?.data ?? bytes
          const inlineType = inlined?.mimeType ?? contentType
          payload.inline = {
            mimeType: inlineType,
            width: inlined?.width,
            height: inlined?.height,
            bytes: inlineBytes.byteLength,
            downscaled: inlined?.downscaled ?? false,
          }
          return {
            content: [
              {
                type: `image` as const,
                data: Buffer.from(inlineBytes).toString(`base64`),
                mimeType: inlineType,
              },
              {
                type: `text` as const,
                text: JSON.stringify(payload, null, 2),
              },
            ],
          }
        }

        return ok(payload)
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
      annotations: READ_ONLY,
      description: `List labels for a team.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
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
          .limit(limit)
          .offset(offset)
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_get`,
    {
      annotations: READ_ONLY,
      description: `Get a label by id (must be in a team the user belongs to).`,
      inputSchema: strictInput({ id: uuidString }),
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
      inputSchema: strictInput({
        teamId: uuidString,
        name: z.string().min(1).max(255),
        color: hexColorSchema.default(DEFAULT_ACCENT_COLOR),
      }),
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
      description: `Update a label's name or color (by its UUID). Returns the updated label.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        color: hexColorSchema.optional(),
      }),
    },
    async ({ id, ...rest }) => {
      try {
        const { teamId } = await getLabelContext(id)
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).labels.update({
          teamId,
          labelId: id,
          ...rest,
        })
        return ok(result.label)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_delete`,
    {
      description: `Delete a label (by its UUID).`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        const { teamId } = await getLabelContext(id)
        assertTeamFullyGranted(access, teamId)
        await caller(user, request).labels.delete({ teamId, labelId: id })
        return ok({ ok: true, id })
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
      description: `Attach a label to an issue (UUID or identifier; teams must match).`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        labelId: uuidString,
      }),
    },
    async ({ issueId: issueIdInput, labelId }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).issueLabels.add({ issueId, labelId })
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issue_labels_remove`,
    {
      description: `Detach a label from an issue (UUID or identifier).`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        labelId: uuidString,
      }),
    },
    async ({ issueId: issueIdInput, labelId }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).issueLabels.remove({ issueId, labelId })
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Issue ↔ Issue (EXP-736)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issue_relations_add`,
    {
      description: `Link two issues (UUIDs or identifiers, same team). Stored one way: blocks = issueId blocks relatedIssueId, parent = issueId is the parent, duplicate = issueId duplicates relatedIssueId, related is symmetric. Pass inverse:true to state it the other way round (blocked by / sub-issue of / duplicated by).`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        relatedIssueId: z.string().min(1),
        type: z.enum(issueRelationTypeValues),
        inverse: z.boolean().optional(),
      }),
    },
    async ({ issueId: issueIdInput, relatedIssueId: relatedInput, type, inverse }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const relatedIssueId = await resolveIssueId(
          relatedInput,
          user.id,
          access
        )
        if (!access.full) {
          for (const id of [issueId, relatedIssueId]) {
            const ctxIssue = await getIssueTeamContext(id)
            assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
          }
        }
        const result = await caller(user, request).relations.create({
          issueId,
          relatedIssueId,
          type,
          inverse,
        })
        noteAgentIssueActivity(issueId, user.id)
        noteAgentIssueActivity(relatedIssueId, user.id)
        return ok({ ok: true, txId: result.txId })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issue_relations_remove`,
    {
      description: `Unlink two issues. Name the pair in the direction the link is stored (see exponential_issue_relations_add); exponential_issues_get lists them.`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        relatedIssueId: z.string().min(1),
        type: z.enum(issueRelationTypeValues),
      }),
    },
    async ({ issueId: issueIdInput, relatedIssueId: relatedInput, type }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const relatedIssueId = await resolveIssueId(
          relatedInput,
          user.id,
          access
        )
        if (!access.full) {
          // BOTH sides, exactly like _add: unlinking mutates the far issue's
          // graph too, so a token granted only one of the two boards must not
          // reach through the row to the other.
          for (const id of [issueId, relatedIssueId]) {
            const ctxIssue = await getIssueTeamContext(id)
            assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
          }
        }
        const canonical = canonicalizeRelation(issueId, relatedIssueId, type)
        const [row] = await db
          .select({ id: issueRelations.id })
          .from(issueRelations)
          .where(
            and(
              eq(issueRelations.issueId, canonical.issueId),
              eq(issueRelations.relatedIssueId, canonical.relatedIssueId),
              eq(issueRelations.type, canonical.type)
            )
          )
          .limit(1)
        if (!row) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Relation not found`,
          })
        }
        const result = await caller(user, request).relations.delete({
          id: row.id,
        })
        noteAgentIssueActivity(issueId, user.id)
        noteAgentIssueActivity(relatedIssueId, user.id)
        return ok({ ok: true, id: row.id, txId: result.txId })
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
      annotations: READ_ONLY,
      description: `List comments on an issue (oldest first) by UUID or human identifier (e.g. "MET-12"). Rows include their linked attachments. The MCP user must have access to the issue's team.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        limit: z.number().int().min(1).max(200).default(100),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ issueId: issueIdInput, limit, offset }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(issueId)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        const rows = await db
          .select(commentWireColumns)
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

  // FEED-87: the issue's activity log (the timeline's events, unfolded) as
  // per-field history: who changed what and when, agent runs included.
  server.registerTool(
    `exponential_issues_activity`,
    {
      annotations: READ_ONLY,
      description: `Issue activity history, oldest first: {field, from, to, actorId, actorKind user|agent|automation, sessionId, at} per change (status ids, assignee, label, priority, estimate, title, board, pr, relation). since: ISO, at or after (the last row's at; rows sharing that instant repeat, dedupe by id).`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        since: isoDateTime.optional(),
        limit: z.number().int().min(1).max(500).default(200),
      }),
    },
    async ({ issueId: issueIdInput, since, limit }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(issueId)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        const rows = await db
          .select({
            id: issueEvents.id,
            type: issueEvents.type,
            payload: issueEvents.payload,
            actorUserId: issueEvents.actorUserId,
            viaMcp: issueEvents.viaMcp,
            sessionId: issueEvents.sessionId,
            createdAt: issueEvents.createdAt,
          })
          .from(issueEvents)
          .where(
            and(
              eq(issueEvents.issueId, issueId),
              // At-or-after, never strictly after: one `issues.update` writes
              // its events with ONE `now()`, and `at` is serialized at ms
              // precision, so a strict cut at the last row's instant would
              // drop its siblings. A repeat is deduped by id; a skip is lost.
              since ? gte(issueEvents.createdAt, new Date(since)) : undefined
            )
          )
          .orderBy(asc(issueEvents.createdAt), asc(issueEvents.id))
          .limit(limit)
        return ok(rows.map(activityWire))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_comments_create`,
    {
      description: `Comment on an issue (UUID or identifier, e.g. "MET-12") as the MCP user ("via MCP"); body = plain GFM. parentId (from exponential_comments_list) replies under that comment; threads are one level deep. audience "reporter" (top-level, a widget issue with a reporter email) also emails it; reporterEmailed says whether it went out.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        body: z.string().trim().min(1).max(10_000).describe(`Plain GFM text`),
        attachmentIds: z.array(uuidString).max(10).optional(),
        parentId: uuidString.optional(),
        audience: z.enum(commentAudienceValues).optional(),
      }),
    },
    async ({ issueId: issueIdInput, body, attachmentIds, parentId, audience }) => {
      try {
        // SLOP-4: a reporter reply EMAILS an outside address with text the
        // reporter chose, so it needs a person behind it: never a
        // board-confined OAuth grant or scoped key, never an unattended run.
        if (audience === `reporter`) {
          if (!access.full) {
            throw new Error(
              `audience "reporter" needs a full-access credential: this connection is confined to chosen teams/boards. Post a team comment instead.`
            )
          }
          const own = await loadCallerSession()
          if (own?.unattended) {
            throw new Error(
              `audience "reporter" is refused in an unattended session: a person must send replies to a reporter. Post a team comment instead.`
            )
          }
        }
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).comments.create({
          issueId,
          body,
          ...(attachmentIds ? { attachmentIds } : {}),
          ...(parentId ? { parentId } : {}),
          ...(audience ? { audience } : {}),
        })
        noteAgentIssueActivity(issueId, user.id)
        // EXP-1241: the issue's identifier rides beside the row, so the
        // run's comment card links `<issue>?comment=<id>` (the preview's
        // `issue@` ref reads it as the issue's identifier twin).
        const [issue] = await db
          .select({ identifier: issues.identifier })
          .from(issues)
          .where(eq(issues.id, issueId))
          .limit(1)
        return ok({
          ...result.comment,
          ...(issue?.identifier ? { issueIdentifier: issue.identifier } : {}),
          reporterEmailed: result.reporterEmailed,
        })
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
      description: `Set an issue's status (UUID or identifier). Pass status (builtin enum) or statusId (a team status row from exponential_statuses_list). Status changes are normally AUTOMATIC — PR open/merge apply the team's configured status automation — so set one directly only when the user explicitly asks.`,
      inputSchema: strictInput({
        id: z.string().min(1),
        status: issueStatusEnumSchema.optional(),
        statusId: uuidString.optional(),
      }),
    },
    async ({ id: idOrIdentifier, status, statusId }) => {
      try {
        if ((status === undefined) === (statusId === undefined)) {
          throw new Error(`Pass exactly one of status or statusId`)
        }
        const id = await resolveIssueId(idOrIdentifier, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).issues.update({
          id,
          status,
          statusId,
        })
        noteAgentIssueActivity(id, user.id)
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-660: custom status CRUD (FEED-17). Membership and every invariant
  // (started ≤ 4, locked builtins, unique names, reassign-before-delete) live
  // in the statuses router — these only add the grant check and project.
  server.registerTool(
    `exponential_statuses_create`,
    {
      description: `Create a custom issue status in a category (never duplicate; started allows at most 4 rows per team). Names are unique per team, color is #rrggbb. Team members only; ids via exponential_statuses_list.`,
      inputSchema: strictInput({
        teamId: uuidString,
        category: z.enum(customizableStatusCategoryValues),
        name: z.string().min(1).max(255),
        color: hexColorSchema,
      }),
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).statuses.create(input)
        return ok(result.status)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_statuses_update`,
    {
      description: `Rename or recolor a custom issue status (by its UUID). Builtin statuses (builtinKey set) are locked and the category is immutable. Team members only. Returns the updated status.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        color: hexColorSchema.optional(),
      }),
    },
    async ({ id, ...rest }) => {
      try {
        const { teamId } = await getStatusContext(id)
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).statuses.update({
          teamId,
          statusId: id,
          ...rest,
        })
        return ok(result.status)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_statuses_delete`,
    {
      description: `Delete a custom issue status (by its UUID). Builtins refuse. When issues still use it the call fails with their count until reassignToId names a same-team replacement (not duplicate). Team members only.`,
      inputSchema: strictInput({
        id: uuidString,
        reassignToId: uuidString.optional(),
      }),
    },
    async ({ id, reassignToId }) => {
      try {
        const { teamId } = await getStatusContext(id)
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).statuses.delete({
          teamId,
          statusId: id,
          reassignToId,
        })
        return ok({
          ok: true,
          id,
          reassigned: result.reassigned,
          reassignedToId: result.reassignedToId,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_statuses_list`,
    {
      annotations: READ_ONLY,
      description: `List a team's issue statuses (id, name, category, color, position, builtinKey). Use id as statusId in exponential_issues_update.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
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
            color: issueStatuses.color,
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
          page(
            rows.map((row) => {
              const position = (positions.get(row.category) ?? 0) + 1
              positions.set(row.category, position)
              return {
                id: row.id,
                name: row.name,
                category: row.category,
                color: row.color,
                position,
                builtinKey: row.builtinKey,
              }
            }),
            limit,
            offset
          )
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Sessions (EXP-1291 Threads v2: ONE session tree; the agent layer)
  // -----------------------------------------------------------------------
  // A thread is a ROOT session (kind thread), a team agent a root of kind
  // agent; children are sub-sessions. Every start and message is ONE call,
  // `exponential_sessions_send` (= `sessions.send`); the database is the bus.
  // Visibility, steering rights and the tree rules live in lib/sessions/**;
  // these tools add the OAuth grant check and resolve the caller's session.

  /** The confined-grant check for a session the caller names: the whole
   * team must be granted (sessions are team-level operational data, like
   * actions). Visibility itself is `loadSessionForUser`'s. */
  async function loadGrantedSession(id: string) {
    const session = await loadSessionForUser(user.id, id)
    if (!access.full) assertTeamFullyGranted(access, session.teamId)
    return session
  }

  /** The thread root a link tool writes on: the named session's root (a
   * steer right on it), else the caller's own root. */
  async function linkTarget(
    sessionIdArg: string | undefined,
    tool: string
  ): Promise<{ rootId: string; teamId: string }> {
    if (sessionIdArg) {
      const session = await loadGrantedSession(sessionIdArg)
      await assertCanSteer(user.id, session)
      return { rootId: session.rootId, teamId: session.teamId }
    }
    const own = await loadCallerSession()
    if (!own) throw new Error(noSessionMessage(tool, `sessionId`))
    return { rootId: own.rootId, teamId: own.teamId }
  }

  /** Issue refs (UUID or identifier) → ids, grant-checked and confined to
   * one team. */
  async function resolveTeamIssues(
    refs: readonly string[],
    teamId: string
  ): Promise<string[]> {
    const ids: string[] = []
    for (const ref of refs) {
      const id = await resolveIssueId(ref, user.id, access)
      const ctxIssue = await getIssueTeamContext(id)
      assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
      if (ctxIssue.teamId !== teamId) {
        throw new Error(`Issue ${ref} is not in this session's team`)
      }
      if (!ids.includes(id)) ids.push(id)
    }
    return ids
  }

  server.registerTool(
    `exponential_sessions_send`,
    {
      description: `THE one call for sessions. sessionId = message it (idle resumes, an ended one continues in a child; account alone = switch its claude login; deviceId = move it there after its turn; issueIds = forward them). parentId = start a child (issueIds = its share). Neither = a new thread (text, issueIds, actionId+inputs, repositoryId) or, with agent {name, icon, color, instructions?, triggers?}, a team agent. name = a new session's title. New sessions need deviceId (exponential_devices_list); the other fields pick the runner. teamId defaults to your session's team; @email in text mentions. Queued at once: never resend.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        teamId: uuidString.optional(),
        sessionId: uuidString.optional(),
        parentId: uuidString.optional(),
        deviceId: z.string().optional(),
        cli: z.string().optional(),
        account: z.string().optional(),
        model: z.string().optional(),
        effort: z.string().optional(),
        planMode: z.boolean().optional(),
        computerUse: z.boolean().optional(),
        ultracode: z.boolean().optional(),
        mcpServerIds: z.array(uuidString).max(20).optional(),
        name: z.string().optional(),
        text: z.string().max(SESSIONS_SEND_TEXT_MAX).optional(),
        attachmentIds: z.array(uuidString).max(10).optional(),
        issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
        actionId: uuidString.optional(),
        inputs: z.record(z.string(), z.string()).optional(),
        repositoryId: uuidString.optional(),
        // Loose for the context budget; sessionsSendInputSchema validates all.
        agent: z.record(z.string(), z.unknown()).optional(),
      }),
    },
    async ({ issueIds: issueRefs, ...rest }) => {
      try {
        const own = await loadCallerSession()
        // A confined grant: the target session's team, else the named team,
        // else the caller's (the default the core applies).
        if (!access.full) {
          const target = rest.sessionId ?? rest.parentId
          const teamId = target
            ? (await loadGrantedSession(target)).teamId
            : (rest.teamId ?? own?.teamId)
          if (!teamId) throw new Error(`Pass teamId: this connection is confined to chosen teams.`)
          assertTeamFullyGranted(access, teamId)
        }
        const issueIds = issueRefs
          ? await Promise.all(
              issueRefs.map(async (ref) => {
                const id = await resolveIssueId(ref, user.id, access)
                if (!access.full) {
                  const ctxIssue = await getIssueTeamContext(id)
                  assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
                }
                return id
              })
            )
          : undefined
        const input = sessionsSendInputSchema.parse({
          ...rest,
          ...(issueIds ? { issueIds } : {}),
        })
        const { txid: _txid, ...result } = await sendSession({
          userId: user.id,
          input,
          callerSessionId: own?.id ?? null,
          // An agent's own key: its message into another session is THAT
          // session's agent-authored input (never over tRPC).
          viaMcp: true,
        })
        return ok(result)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_get`,
    {
      annotations: READ_ONLY,
      description: `Read a session tree: the root (thread or agent) with its facets (title, issues, PRs, Guide, action), its child sessions and the last messages (count = messages, default 20). id = any session in the tree; omitted = your own session's root.`,
      inputSchema: strictInput({
        id: uuidString.optional(),
        messages: z.number().int().min(0).max(100).default(20),
      }),
    },
    async ({ id, messages }) => {
      try {
        // Any session of a tree resolves to its root (the core's rule).
        let target: string
        if (id) {
          if (!access.full) await loadGrantedSession(id)
          target = id
        } else {
          const own = await loadCallerSession()
          if (!own) throw new Error(noSessionMessage(`exponential_sessions_get`, `id`))
          target = own.rootId
        }
        return ok(await getSessionTree(user.id, target, { messages: messages ?? 20 }))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_list`,
    {
      annotations: READ_ONLY,
      description: `List root sessions you can see, newest first: kind thread (default) or agent (the team's agents). teamId narrows to one team; live: true = not ended.`,
      inputSchema: strictInput({
        kind: z.enum(sessionKindValues).default(`thread`),
        teamId: uuidString.optional(),
        live: z.boolean().optional(),
        ...pageInput,
      }),
    },
    async ({ kind, teamId, live, limit, offset }) => {
      try {
        if (!access.full) {
          if (!teamId) throw new Error(`Pass teamId: this connection is confined to chosen teams.`)
          assertTeamFullyGranted(access, teamId)
        }
        return ok(
          await listSessions(user.id, {
            kind: kind ?? `thread`,
            ...(teamId ? { teamId } : {}),
            ...(live !== undefined ? { live } : {}),
            limit: limit ?? 50,
            offset: offset ?? 0,
          })
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_stop`,
    {
      description: `Stop a session's running turn (by UUID); the session stays, idle, and a message resumes it.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) await loadGrantedSession(id)
        await stopSession({ userId: user.id, sessionId: id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_end`,
    {
      description: `End a session for good (by UUID; omitted = your own). summary (one paragraph: finished, stopped for a human, or changed nothing) becomes its final message. Agents never end.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        id: uuidString.optional(),
        summary: z.string().trim().min(1).max(4000).optional(),
      }),
    },
    async ({ id, summary }) => {
      try {
        let target = id ?? null
        if (target) {
          if (!access.full) await loadGrantedSession(target)
        } else {
          target = (await loadCallerSession())?.id ?? null
          if (!target) throw new Error(noSessionMessage(`exponential_sessions_end`, `id`))
        }
        await endSession({ userId: user.id, sessionId: target, summary: summary ?? null })
        return ok({ ok: true, id: target })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-988/EXP-936: the run asks its HOST to compact its context. It acts
  // on the caller's own session, so a caller with none never sees it. The
  // handler file owns ownership, the refusal codes and the relay hop.
  if (gates.inSession) {
    server.registerTool(
      `exponential_sessions_compact`,
      {
        description: `Ask the host to compact this session's context at the next turn boundary. reason is logged on the session; keep names what the summary must preserve (open threads, decisions, file paths). Returns accepted, or refusedBecause: too_early (under half the context used), cooldown (compacted within the last 20 turns), not_own_session, unsupported_agent. Only your own session.`,
        inputSchema: strictInput({
          reason: z.string().trim().min(1).max(500),
          keep: z.string().trim().min(1).max(2_000).optional(),
        }),
      },
      async ({ reason, keep }) => {
        try {
          if (!sessionId) {
            throw new Error(noSessionMessage(`exponential_sessions_compact`))
          }
          return ok(
            await requestSessionCompaction({
              sessionId,
              userId: user.id,
              reason,
              keep,
            })
          )
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // EXP-1328: follow-ups for a person to start, as ONE suggestions card on
  // the caller's own session (1:1 with the card; it wakes nothing). EXP-1211:
  // `replies` = a replies card, quick replies above the caller's composer.
  if (gates.inSession) {
    server.registerTool(
      `exponential_sessions_suggest`,
      {
        description: `Propose follow-ups on your own session. tasks = one card in your conversation; a person starts, files or dismisses each (title = a short name, text = the request a started session gets, issueIds = issues it is about). replies = quick replies above the composer: likely next messages; a tap sends one to you as the next message; your newest replies replace older ones.`,
        inputSchema: strictInput({
          tasks: z
            .array(
              z.strictObject({
                ...suggestionInputSchema.shape,
                issueIds: z.array(z.string().min(1)).max(10).optional(),
              })
            )
            .min(1)
            .max(SUGGESTIONS_MAX)
            .optional(),
          replies: z.array(replyInputSchema).min(1).max(REPLIES_MAX).optional(),
        }),
      },
      async ({ tasks, replies }) => {
        try {
          if (!tasks?.length && !replies?.length) throw new Error(`Pass tasks, replies or both`)
          const own = await loadCallerSession()
          if (!own) throw new Error(noSessionMessage(`exponential_sessions_suggest`))
          if (!access.full) assertTeamFullyGranted(access, own.teamId)
          const posted: {
            tasks?: { messageId: string; itemIds: string[] }
            replies?: { messageId: string; itemIds: string[] }
          } = {}
          if (tasks?.length) {
            const resolved = await Promise.all(
              tasks.map(async (task) => ({
                title: task.title,
                text: task.text,
                ...(task.issueIds?.length
                  ? { issueIds: await resolveTeamIssues(task.issueIds, own.teamId) }
                  : {}),
              }))
            )
            posted.tasks = await postSuggestions({
              userId: user.id,
              callerSessionId: own.id,
              tasks: resolved,
            })
          }
          if (replies?.length) {
            posted.replies = await postReplies({ userId: user.id, callerSessionId: own.id, replies })
          }
          return ok(posted)
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // EXP-1280 custom plan mode: the session posts ONE rich card on itself
  // (lib/sessions/plan.ts) and stops; a person answers IN the card and the
  // answer arrives as the session's next message. Not always-loaded: the
  // runbook names both, tool search finds them. A host that cannot feed the
  // answer back (`hostFeedsOwnCards`) gets neither: `gates.cards === false`
  // (resolved with the gates) drops the registration, and a call on such a
  // host is refused with the inline alternative either way.
  const cards = gates.cards !== false
  if (gates.inSession && cards) {
    server.registerTool(
      `exponential_plan_propose`,
      {
        description: `Plan mode: post your plan as one card for a person to approve, then end your turn. plan = GFM; a picture = ![alt](upload:<label>) declared in pictures (run each returned curl). questions = choices answered in the card (option.picture = a label). surface = an Exponential UI core tree {root, data}. The answer arrives as your next message.`,
        inputSchema: strictInput({
          plan: z.string().trim().min(1).max(MESSAGE_BODY_MAX),
          pictures: cardPicturesInput,
          questions: z
            .array(
              z.strictObject({
                id: z.string().trim().min(1).max(PLAN_CARD_LABEL_KEY_MAX),
                prompt: z.string().trim().min(1).max(PLAN_CARD_PROMPT_MAX),
                multi: z.boolean().optional(),
                custom: z.boolean().optional(),
                options: z.array(cardOptionInput).max(PLAN_CARD_OPTIONS_MAX),
              })
            )
            .max(PLAN_CARD_QUESTIONS_MAX)
            .optional(),
          surface: cardSurfaceInput,
        }),
      },
      async (input) => {
        try {
          const own = await loadCallerSession()
          if (!own) throw new Error(noSessionMessage(`exponential_plan_propose`))
          if (!own.cards) throw new Error(CARDS_NEED_HOST_UPDATE)
          if (!access.full) assertTeamFullyGranted(access, own.teamId)
          const card = buildPlanCard(input)
          return ok(await postAskCard({ userId: user.id, sessionId: own.id, card }))
        } catch (e) {
          return err(e)
        }
      }
    )

    server.registerTool(
      `exponential_sessions_ask`,
      {
        description: `Ask a person ONE question as a card in your conversation, then end your turn. options = text or picture choices (picture = a label in pictures; run each returned curl), multi = several picks, custom = free text allowed (default true). surface = an Exponential UI core tree {root, data}. The answer arrives as your next message.`,
        inputSchema: strictInput({
          question: z.string().trim().min(1).max(PLAN_CARD_PROMPT_MAX),
          options: z.array(cardOptionInput).max(QUESTION_CARD_OPTIONS_MAX),
          multi: z.boolean().optional(),
          custom: z.boolean().optional(),
          pictures: cardPicturesInput,
          surface: cardSurfaceInput,
        }),
      },
      async (input) => {
        try {
          const own = await loadCallerSession()
          if (!own) throw new Error(noSessionMessage(`exponential_sessions_ask`))
          if (!own.cards) throw new Error(CARDS_NEED_HOST_UPDATE)
          if (!access.full) assertTeamFullyGranted(access, own.teamId)
          const card = buildQuestionCard(input)
          return ok(await postAskCard({ userId: user.id, sessionId: own.id, card }))
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  server.registerTool(
    `exponential_sessions_link`,
    {
      description: `Link issues (UUIDs or identifiers) to a thread: the root of sessionId, default your own session's. Same team only.`,
      inputSchema: strictInput({
        issueIds: z.array(z.string().min(1)).min(1).max(30),
        sessionId: uuidString.optional(),
      }),
    },
    async ({ issueIds: refs, sessionId: sessionIdArg }) => {
      try {
        const { rootId, teamId } = await linkTarget(sessionIdArg, `exponential_sessions_link`)
        const issueIds = await resolveTeamIssues(refs, teamId)
        const own = await loadCallerSession()
        const result = await db.transaction((tx) =>
          linkIssues(tx, {
            rootId,
            issueIds,
            source: own ? `agent` : `user`,
            addedBy: user.id,
            addedBySessionId: own?.id ?? null,
          })
        )
        for (const id of issueIds) noteAgentIssueActivity(id, user.id)
        return ok({ sessionId: rootId, issueIds, ...(isRecord(result) ? result : {}) })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_unlink`,
    {
      description: `Unlink an issue (UUID or identifier) from a thread: the root of sessionId, default your own session's.`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        sessionId: uuidString.optional(),
      }),
    },
    async ({ issueId: ref, sessionId: sessionIdArg }) => {
      try {
        const { rootId, teamId } = await linkTarget(sessionIdArg, `exponential_sessions_unlink`)
        const [issueId] = await resolveTeamIssues([ref], teamId)
        const own = await loadCallerSession()
        const result = await db.transaction((tx) =>
          unlinkIssue(tx, {
            rootId,
            issueId: issueId!,
            removedBySessionId: own?.id ?? null,
          })
        )
        return ok({ sessionId: rootId, issueId, ...(isRecord(result) ? result : {}) })
      } catch (e) {
        return err(e)
      }
    }
  )

  // The thread's ONE Guide = the PR body of every PR in it. Writes on the
  // caller's own session (its root's Guide), so it needs one.
  if (gates.inSession) {
    server.registerTool(
      `exponential_guide_write`,
      {
        description: GUIDE_TOOL_DESCRIPTION,
        _meta: ALWAYS_LOAD_META,
        inputSchema: strictInput({
          topic: z.string().trim().min(1).max(80),
          text: z.string().max(SESSION_RESULT_REPORT_MAX).optional(),
          files: z.array(z.string().min(1).max(512)).max(200).optional(),
          prUrl: z.string().url().max(512).optional(),
          remove: z.boolean().optional(),
          label: z.string().trim().min(1).max(80).optional(),
        }),
      },
      async ({ topic, text, files, prUrl, remove, label }) => {
        try {
          const own = await loadCallerSession()
          if (!own) throw new Error(noSessionMessage(`exponential_guide_write`))
          const result = await writeGuide({
            userId: user.id,
            sessionId: own.id,
            topic,
            text: text ?? null,
            files: files ?? null,
            prUrl: prUrl ?? null,
            remove: remove ?? false,
            label: label ?? null,
          })
          return ok(result.curl ? { ...result, next: GUIDE_PICTURE_NEXT_STEP } : result)
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // -----------------------------------------------------------------------
  // Pull requests (EXP-1291: `pull_requests` rows; lib/pull-requests.ts)
  // -----------------------------------------------------------------------
  // Every PR tool names its PR the same way: pullRequestId, issueId (the
  // issue's open PR, `pickIssuePullRequest`: several = refused, FEED-111) or
  // repositoryId + prNumber; none = the caller session's newest open PR. The
  // PR's team must be fully granted.

  const prSelector = {
    pullRequestId: uuidString.optional(),
    issueId: z.string().min(1).optional(),
    repositoryId: uuidString.optional(),
    prNumber: z.number().int().positive().optional(),
  }

  async function selectPr(args: {
    pullRequestId?: string
    issueId?: string
    repositoryId?: string
    prNumber?: number
  }) {
    if (args.prNumber !== undefined && !args.repositoryId) {
      throw new Error(`prNumber needs repositoryId`)
    }
    const own = await loadCallerSession()
    const issueId = args.issueId
      ? await resolveIssueId(args.issueId, user.id, access)
      : undefined
    const pr = await resolvePullRequest({
      userId: user.id,
      ...(args.pullRequestId ? { pullRequestId: args.pullRequestId } : {}),
      ...(issueId ? { issueId } : {}),
      ...(args.repositoryId ? { repositoryId: args.repositoryId } : {}),
      ...(args.prNumber !== undefined ? { number: args.prNumber } : {}),
      callerSessionId: own?.id ?? null,
    })
    if (!access.full) assertTeamFullyGranted(access, pr.teamId)
    return { pr, callerSessionId: own?.id ?? null }
  }

  const prRef = (pr: { id: string; number: number; url: string }) => ({
    pullRequestId: pr.id,
    number: pr.number,
    url: pr.url,
  })

  server.registerTool(
    `exponential_pr_open`,
    {
      description: `Open a GitHub PR (via the GitHub App, never 'gh') from head (default your session's branch) into base (default: the open PR below head, else the default branch; an open PR's branch joins its stack). issueIds (UUIDs or identifiers) = the issues it closes. Omitted = your session's SHARE of the thread, not the root's whole list: the issues your session or its children linked plus the root issues your start message names as #IDENT (the root's own PR takes them all; a work session with no share takes the root issues no other work session linked; issues a merged PR closed are left out). repositoryId when there are none. Body = your thread's Guide once written. A diff touching UI needs a Guide picture per platform (label starts web, web-mobile, ios, android or desktop) or notRun naming the screen you could not run. Linked issues take the team's PR-open status.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        title: z.string().min(1).max(255),
        body: z.string().max(60_000).optional(),
        head: z.string().min(1).max(255).optional(),
        base: z.string().min(1).max(255).optional(),
        draft: z.boolean().optional(),
        repositoryId: uuidString.optional(),
        issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
        notRun: z.string().trim().min(1).max(300).optional(),
      }),
    },
    async ({ issueIds: refs, ...rest }) => {
      try {
        const own = await loadCallerSession()
        // EXP-1280: a session in plan mode opens no PR until a person
        // approves its plan (approval flips plan_mode off).
        if (own?.planMode) {
          throw new TRPCError({ code: `FORBIDDEN`, message: PLAN_CARD_COPY.planModeGate })
        }
        const issueIds: string[] = []
        for (const ref of refs ?? []) {
          const id = await resolveIssueId(ref, user.id, access)
          if (!access.full) {
            const ctxIssue = await getIssueTeamContext(id)
            assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
          }
          if (!issueIds.includes(id)) issueIds.push(id)
        }
        if (!access.full && issueIds.length === 0) {
          const teamId = rest.repositoryId
            ? (await loadRepositoryForTeam(rest.repositoryId)).teamId
            : own?.teamId
          if (!teamId) throw new Error(`Pass issueIds or repositoryId.`)
          assertTeamFullyGranted(access, teamId)
        }
        const result = await openPullRequest({
          userId: user.id,
          callerSessionId: own?.id ?? null,
          ...rest,
          ...(issueIds.length > 0 ? { issueIds } : {}),
        })
        for (const id of issueIds) noteAgentIssueActivity(id, user.id)
        return ok(result)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_merge`,
    {
      description: `Squash-merge an open PR (via the GitHub App). Name it by pullRequestId, issueId or repositoryId + prNumber; default = your session's newest open PR. stack lands a stack member and every open PR beneath it. Merging your own PR never ends your session. Merge a tree root first.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        ...prSelector,
        stack: z.boolean().optional(),
      }),
    },
    async ({ stack, ...selector }) => {
      try {
        const { pr, callerSessionId } = await selectPr(selector)
        const result = await mergePullRequest({
          userId: user.id,
          pullRequestId: pr.id,
          ...(stack !== undefined ? { stack } : {}),
          callerSessionId,
        })
        return ok({ ...prRef(pr), ...result })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_close`,
    {
      description: `Close an open PR without merging (via the GitHub App), e.g. one superseded by another PR. Its issues keep their status. Name it by pullRequestId, issueId or repositoryId + prNumber; default = your session's newest open PR. Only when a person asks.`,
      inputSchema: strictInput({ ...prSelector }),
    },
    async (selector) => {
      try {
        const { pr } = await selectPr(selector)
        await closePullRequestTx({ userId: user.id, pullRequestId: pr.id })
        return ok({ ...prRef(pr), closed: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_retarget`,
    {
      description: `Change an open PR's base branch (omitted = the repo's default branch), e.g. when its base PR merged. Name it by pullRequestId, issueId or repositoryId + prNumber; default = your session's newest open PR. Then rebase onto the new base and push with --force-with-lease.`,
      inputSchema: strictInput({
        ...prSelector,
        base: z.string().min(1).max(255).optional(),
      }),
    },
    async ({ base, ...selector }) => {
      try {
        const { pr } = await selectPr(selector)
        const result = await retargetPullRequest({
          userId: user.id,
          pullRequestId: pr.id,
          ...(base ? { base } : {}),
        })
        return ok({ ...prRef(pr), ...result })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_update`,
    {
      description: `Rewrite an open PR's title or body, or flip draft. Name it by pullRequestId, issueId or repositoryId + prNumber; default = your session's newest open PR.`,
      inputSchema: strictInput({
        ...prSelector,
        title: z.string().min(1).max(255).optional(),
        body: z.string().max(60_000).optional(),
        draft: z.boolean().optional(),
      }),
    },
    async ({ title, body, draft, ...selector }) => {
      try {
        if (title === undefined && body === undefined && draft === undefined) {
          throw new Error(`Pass title, body or draft`)
        }
        const { pr } = await selectPr(selector)
        const result = await updatePullRequest({
          userId: user.id,
          pullRequestId: pr.id,
          ...(title !== undefined ? { title } : {}),
          ...(body !== undefined ? { body } : {}),
          ...(draft !== undefined ? { draft } : {}),
        })
        return ok({ ...prRef(pr), ...(isRecord(result) ? result : {}) })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_files`,
    {
      annotations: READ_ONLY,
      description: `List a PR's changed files with patches and add/delete counts. Name it by pullRequestId, issueId or repositoryId + prNumber; default = your session's newest open PR.`,
      inputSchema: strictInput({ ...prSelector }),
    },
    async (selector) => {
      try {
        const { pr } = await selectPr(selector)
        const result = await pullRequestFiles({ userId: user.id, pullRequestId: pr.id })
        return ok({ ...prRef(pr), ...result })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Devices (EXP-660: the picker for exponential_sessions_send's deviceId)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_devices_list`,
    {
      annotations: READ_ONLY,
      description: `List your registered machines (desktop app / CLI daemon), plus servers teammates shared with teamId. Pick an online device whose agents includes the agent you want. agentUsage.<agent> = its last used login: windows[] (percent + resetsAt), fetchedAt = when those numbers were read, stale: true = the last refresh failed and they are as old as fetchedAt; agentUsageAt = when the device last reported. A session running on another account moves that account's own row under agentAccounts.<agent>.profiles[].usage instead. A live session refreshes only the account it runs on, per turn; once it ends — or a window's resetsAt passes — that login returns to the polled cadence. doctor = its readiness report: items[] {key, state ok|action|missing|off|error, detail, action} (null = not reported yet).`,
      // EXP-1183: the MCP Apps devices view (lib/mcp/apps.ts).
      _meta: appMeta(`devices`),
      inputSchema: strictInput({
        teamId: uuidString.optional(),
        ...pageInput,
      }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        // Shared rows name teammates' machines and agents — team-level
        // operational data, gated like actions_list, and readable only by a
        // member of that team.
        if (teamId) {
          assertTeamFullyGranted(access, teamId)
          await assertTeamMember(user.id, teamId)
        }

        // Own rows plus the team's shared servers — the ONE encoding of
        // that query lives in the devices router (visibleDeviceRows).
        const { rows, ownerNames } = await visibleDeviceRows(
          db,
          user.id,
          teamId
        )
        // `online` is last_seen_at freshness (contract onlineWindowSeconds),
        // not relay presence — the same rule every synced client applies.
        const list = composeDeviceList(
          rows,
          ownerNames,
          new Date(),
          user.id,
          teamId
        )

        return ok(
          page(list, limit, offset).map((device) => ({
            deviceId: device.deviceId,
            label: device.deviceLabel,
            kind: device.kind,
            platform: device.platform ?? null,
            online: device.online,
            lastSeenAt: device.lastSeenAt,
            agents: device.agents,
            unauthedAgents: device.unauthedAgents,
            caps: device.caps,
            version: device.version,
            sharedTeamIds: device.sharedTeamIds,
            isDefault: device.isDefault,
            // EXP-484: per-agent sign-in status and usage windows as the
            // machine last probed them (absent on builds without the
            // collector).
            agentAccounts: device.agentAccounts ?? null,
            agentUsage: device.agentUsage ?? null,
            agentUsageAt: device.agentUsageAt ?? null,
            // EXP-1196: the readiness report (device-doctor.json).
            doctor: device.doctor ?? null,
            ...(device.owner ? { owner: device.owner } : {}),
          }))
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-1199: sign an agent account in on one of YOUR machines — the
  // clients' Add account / Sign in, over the same device commands.
  server.registerTool(
    `exponential_devices_account_login`,
    {
      description: `Sign an agent account in on one of your own machines (exponential_devices_list: online). The machine runs the agent CLI's own login; only the sign-in URL and the code you type travel, the credential never leaves the machine. Start: {deviceId, agent} adds an account, or profileId re-signs an existing login (agentAccounts.<agent>.profiles[].id, e.g. one that needs a re-login); the login lands on the profile with its email. Returns status url + url (+ code for codex: enter it at url). Claude: the browser then shows a code, call again with {deviceId, agent, code}. status pending = the machine has not answered yet, call again with commandId. The login appears in exponential_devices_list after the machine's next heartbeat.`,
      inputSchema: strictInput({
        deviceId: z.string().min(1).max(128),
        agent: z.enum(codingAgentValues),
        profileId: z.string().min(1).max(64).optional(),
        code: z.string().trim().min(1).max(512).optional(),
        commandId: uuidString.optional(),
      }),
    },
    async (input) => {
      try {
        // A machine's logins sit outside any selectable grant — full access only.
        assertFullAccess(access)
        const trpc = caller(user, request)
        return ok(
          await deviceAccountLogin(input, {
            createCommand: (command) => trpc.devices.createCommand(command),
            getCommand: (commandId) => trpc.devices.getCommand({ commandId }),
          })
        )
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
      inputSchema: strictInput({
        id: uuidString,
        body: z.string().trim().min(1).max(10_000).describe(`Plain GFM text`),
        attachmentIds: z.array(uuidString).max(10).optional(),
      }),
    },
    async ({ id, body, attachmentIds }) => {
      try {
        if (!access.full) {
          const ctxIssue = await getCommentIssueContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).comments.update({
          id,
          body,
          ...(attachmentIds ? { attachmentIds } : {}),
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
      inputSchema: strictInput({ id: uuidString }),
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
      inputSchema: strictInput({ issueId: z.string().min(1) }),
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
      inputSchema: strictInput({ issueId: z.string().min(1) }),
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
      annotations: READ_ONLY,
      description: `List the MCP user's own notifications, newest first. Set unreadOnly to show only those not yet read.`,
      // EXP-1183: the MCP Apps inbox (lib/mcp/apps.ts).
      _meta: appMeta(`inbox`),
      inputSchema: strictInput({
        unreadOnly: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
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
            .select(notificationWireColumns)
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
        // No owner columns: the inner join already drops issue-less rows,
        // so there is no board-less arm to admit here.
        const grantFilter = grantScopeFilter(access, {
          boardCol: issues.boardId,
          teamCol: boards.teamId,
        })
        if (grantFilter === GRANT_MATCHES_NOTHING) return ok([])
        if (grantFilter) conditions.push(grantFilter)
        const rows = await db
          .select({ notification: notificationWireColumns })
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
      inputSchema: strictInput({
        id: uuidString.optional(),
        all: z.boolean().default(false),
      }),
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
      annotations: READ_ONLY,
      description: `List the members of a team. id is the USER id (use it for assigneeId); memberId is the team_members row id. placeholder: no account yet (added or invited, not joined).`,
      inputSchema: strictInput({
        teamId: uuidString,
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        assertTeamVisible(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const rows = await db
          .select({
            id: users.id,
            memberId: teamMembers.id,
            name: users.name,
            email: users.email,
            image: users.image,
            role: teamMembers.role,
            // FEED-108: a placeholder member (EXP-630) has no account — the
            // flag itself is server-only, so this is where agents learn it.
            placeholder: sql<boolean>`${users.placeholderAt} is not null`,
          })
          .from(teamMembers)
          .innerJoin(users, eq(users.id, teamMembers.userId))
          .where(eq(teamMembers.teamId, teamId))
          .orderBy(asc(users.name))
          .limit(limit)
          .offset(offset)
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  // FEED-108: the by-hand placeholder member — what the Linear import does
  // for the people it finds, for ONE person: on the roster at once, assignable,
  // no mail, no seat. Owner-only (teamMembers.addPlaceholder).
  server.registerTool(
    `exponential_members_add`,
    {
      description: `Add a person to a team WITHOUT inviting them: a placeholder member (no account) to assign issues to; id is the assigneeId. No mail, no seat; Send invite brings them in later. Owner only. Fails for an address that has an account: use exponential_invites_create then.`,
      inputSchema: strictInput({
        teamId: uuidString,
        email: z.string().email().max(255),
        name: z.string().trim().max(180).optional(),
      }),
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).teamMembers.addPlaceholder(
          input
        )
        return ok(result.member)
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
      annotations: READ_ONLY,
      description: `List the repositories registered in a team, each with the boards it backs. The MCP user must be a member of the team.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        assertTeamVisible(access, teamId)
        const result = await caller(user, request).repositories.list({
          teamId,
        })
        if (access.full) return ok(page(result, limit, offset))
        // Each repo rides with the boards it backs — a board-scoped
        // grant must not enumerate ungranted sibling boards through them.
        return ok(
          page(
            result.map((repo) => ({
              ...repo,
              boards: repo.boards.filter((p) =>
                isBoardGranted(access, p.id, teamId)
              ),
            })),
            limit,
            offset
          )
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
      inputSchema: strictInput({
        teamId: uuidString,
        fullName: z
          .string()
          .min(1)
          .max(255)
          .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`),
        defaultBranch: z.string().min(1).max(255).optional(),
        private: z.boolean().optional(),
        installationId: z.number().int().optional(),
      }),
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

  // -----------------------------------------------------------------------
  // Actions (EXP-1291: SAVED PROMPTS; run one with exponential_sessions_send
  // {actionId, inputs})
  // -----------------------------------------------------------------------
  // FULL team grant even for reads: action bodies are executed prompts, not
  // board-workflow aux data.

  server.registerTool(
    `exponential_actions_list`,
    {
      annotations: READ_ONLY,
      description: `List a team's actions: saved prompts with typed pick inputs. Run one with exponential_sessions_send {actionId, inputs}; exponential_actions_get reads its body.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        if (!access.full) assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).actions.list({ teamId })
        return ok(page(result.actions, limit, offset))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_get`,
    {
      annotations: READ_ONLY,
      description: `Read one action by UUID, its markdown body included.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          assertTeamFullyGranted(access, (await getActionContext(id)).teamId)
        }
        const result = await caller(user, request).actions.get({ id })
        return ok(result.action)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_upsert`,
    {
      description: `Create (no id: teamId, name, body required) or update (id; only the fields you pass) a team action. Owner only. body = the markdown prompt; icon = a curated icon name (null clears); inputs = typed picks {key, label, type: repo|board|pr|issue, required?, placeholder?}, whole-array replace; promptPlaceholder = the composer's hint.`,
      inputSchema: strictInput({
        id: uuidString.optional(),
        teamId: uuidString.optional(),
        name: z.string().min(1).max(255).optional(),
        icon: boardIconEnumSchema.nullable().optional(),
        body: z.string().min(1).optional(),
        inputs: actionInputsSchema.optional(),
        promptPlaceholder: z.string().max(200).nullable().optional(),
        sortOrder: z.number().finite().optional(),
      }),
    },
    async ({ id, teamId, ...fields }) => {
      try {
        const trpc = caller(user, request)
        if (id) {
          if (teamId) throw new Error(`teamId is fixed: an update takes id only`)
          if (!access.full) {
            assertTeamFullyGranted(access, (await getActionContext(id)).teamId)
          }
          return ok((await trpc.actions.update({ id, ...fields })).action)
        }
        const { name, body, sortOrder: _sortOrder, ...rest } = fields
        if (!teamId || !name || !body) {
          throw new Error(`A new action needs teamId, name and body`)
        }
        if (!access.full) assertTeamFullyGranted(access, teamId)
        return ok((await trpc.actions.create({ teamId, name, body, ...rest })).action)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_delete`,
    {
      description: `Delete an action by UUID (owner only). Threads that ran it keep its name.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          assertTeamFullyGranted(access, (await getActionContext(id)).teamId)
        }
        await caller(user, request).actions.delete({ id })
        return ok({ ok: true, id })
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
      description: `Move a board to the trash (owner only; by its UUID). Purged with all issues after 48 hours; owners can restore from web settings before then.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(id)
          assertBoardGranted(access, board.id, board.teamId)
        }
        await caller(user, request).boards.delete({ boardId: id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_set_repository`,
    {
      description: `Point a board (by its UUID) at a different registered repository (both must be in the same team), or pass repositoryId: null to detach it. Owner/admin only. Existing worktrees keep working; new coding sessions use the new repo. The board's branch pin resets unless defaultBranch is passed.`,
      inputSchema: strictInput({
        id: uuidString,
        repositoryId: uuidString.nullable(),
        defaultBranch: z.string().min(1).max(255).optional(),
      }),
    },
    async ({ id, repositoryId, defaultBranch }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(id)
          // Retargeting widens the token's GitHub reach to ANY repo in the
          // team registry (pr_open / pr_files then reach
          // the new repo through the granted board's issues) — so this is
          // a team-registry mutation, gated like repositories_add, not
          // a board-scoped one.
          assertTeamFullyGranted(access, board.teamId)
        }
        const result = await caller(user, request).boards.setRepository({
          boardId: id,
          repositoryId,
          defaultBranch,
        })
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
      inputSchema: strictInput({
        name: z.string().min(1).max(255),
        iconUrl: z.string().url().max(2048).optional(),
      }),
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
      description: `Update a team's name, icon, estimate scale or yolo mode (by its UUID). Team owner only. Teams are always private.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        iconUrl: z.string().url().max(2048).nullable().optional(),
        estimationType: z.enum(issueEstimationValues).optional(),
        // EXP-1105: true = every PR exponential_pr_open opens merges at once.
        yoloMode: z.boolean().optional(),
      }),
    },
    async ({ id, ...rest }) => {
      try {
        assertTeamFullyGranted(access, id)
        const result = await caller(user, request).teams.update({
          teamId: id,
          ...rest,
        })
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
      description: `Create an invite link for a team, returning the token to share. Owner only. Pass email to have the server mail the link (emailDelivered reports the attempt); an email invite also adds the person to the team at once as a placeholder member (memberUserId, assignable now; their content carries over when they join). name labels that member.`,
      inputSchema: strictInput({
        teamId: uuidString,
        role: z.enum([`owner`, `member`]).default(`member`),
        email: z.string().email().max(255).optional(),
        name: z.string().trim().max(180).optional(),
      }),
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).teamInvites.create(input)
        return ok({
          invite: result.invite,
          token: result.token,
          emailDelivered: result.emailDelivered,
          memberUserId: result.memberUserId,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_invites_list`,
    {
      annotations: READ_ONLY,
      description: `List the pending (unaccepted) invites for a team. The MCP user must be a member of the team.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).teamInvites.list({
          teamId,
        })
        return ok(page(result.invites, limit, offset))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_invites_revoke`,
    {
      description: `Revoke a pending invite by its UUID. Owner only.`,
      inputSchema: strictInput({ id: uuidString }),
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
  // Team MCP servers (EXP-792): the ONE registry a coding run's extra MCP
  // servers come from. Each member connects once in Settings (the server
  // holds the credential), so the tools list/add/remove and hand back a
  // settings deep link for the connect step — never a credential.
  // -----------------------------------------------------------------------

  /** Settings → MCP servers, auto-starting the connect for `serverId`. */
  const mcpConnectUrl = (teamSlug: string, serverId: string) =>
    `${appBaseUrl()}/t/${encodeURIComponent(teamSlug)}/settings/mcp-servers?connect=${serverId}`

  const loadTeamSlug = async (teamId: string) => {
    const [team] = await db
      .select({ slug: teams.slug })
      .from(teams)
      .where(eq(teams.id, teamId))
      .limit(1)
    if (!team) throw new Error(`Team not found`)
    return team.slug
  }

  server.registerTool(
    `exponential_mcp_servers_list`,
    {
      annotations: READ_ONLY,
      description: `List a team's MCP servers (Linear, Sentry, ...) that coding sessions can connect to, with YOUR connection status (connected | not_connected | expired | error | not_needed), who connected and who shared it with the team's action sessions, and connectUrl: the settings page where you (a person) connect it. Team members only.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        assertTeamFullyGranted(access, teamId)
        const rows = await caller(user, request).mcpServers.list({ teamId })
        const slug = await loadTeamSlug(teamId)
        return ok(
          page(rows, limit, offset).map((row) => ({
            id: row.id,
            name: row.name,
            transport: row.transport,
            url: row.url,
            command: row.command,
            auth: row.auth,
            scopes: row.scopes,
            enabledByDefault: row.enabledByDefault,
            connection: row.connection,
            connectedCount: row.connectedCount,
            memberCount: row.memberCount,
            sharedCount: row.sharedCount,
            sharedUserIds: row.sharedUserIds,
            connectedUserIds: row.connectedUserIds,
            connectUrl: row.auth === `none` ? null : mcpConnectUrl(slug, row.id),
          }))
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_mcp_servers_add`,
    {
      description: `Add a remote (https) MCP server to a team's registry, never a repo .mcp.json. Owner only. Probes the URL to detect OAuth; name defaults from the host. Returns the row and connectUrl, where each member connects once.`,
      inputSchema: strictInput({
        teamId: uuidString,
        url: z.string().url().max(2048),
        name: z.string().trim().min(1).max(MAX_MCP_SERVER_NAME).optional(),
      }),
    },
    async ({ teamId, url, name }) => {
      try {
        assertTeamFullyGranted(access, teamId)
        const trpc = caller(user, request)
        const probe = await trpc.mcpServers.probe({ teamId, url })
        if (!probe.reachable || probe.error) {
          throw new Error(
            `Could not add ${url}: ${probe.error ?? `unreachable`}`
          )
        }
        const row = await trpc.mcpServers.create({
          teamId,
          name: name ?? probe.suggestedName,
          transport: `http`,
          url: probe.url,
          auth: probe.auth,
          scopes: probe.scopes,
        })
        const slug = await loadTeamSlug(teamId)
        return ok({
          ...row,
          connectUrl: row.auth === `none` ? null : mcpConnectUrl(slug, row.id),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_mcp_servers_remove`,
    {
      description: `Remove an MCP server from its team's registry (by UUID); every member's stored connection to it goes too. Owner only.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const [row] = await db
            .select({ teamId: mcpServers.teamId })
            .from(mcpServers)
            .where(eq(mcpServers.id, id))
            .limit(1)
          if (!row) throw new Error(`MCP server not found`)
          assertTeamFullyGranted(access, row.teamId)
        }
        await caller(user, request).mcpServers.remove({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Attachments upload (base64 payload → S3 → attachments row)
  // -----------------------------------------------------------------------
  // EXP-988/EXP-929: three shapes of ONE tool. `dataBase64` present = the
  // inline path below (unchanged). Absent = a SIGNED upload: the call returns
  // an upload URL + curl line (the guide_write shape) and a later call
  // with `attachmentId` alone finalizes the row. The signed halves live in
  // handlers/attachments-upload.ts; the access checks stay here.

  server.registerTool(
    `exponential_attachments_upload`,
    {
      description: `Attach a file to an issue (UUID or identifier). With dataBase64: uploads the bytes now (base64 inflates ~33%). Without it: returns attachmentId, a 10-minute signed uploadUrl and a ready curl line for filename/contentType (+ optional commentId); then call again with attachmentId alone to finalize. Images (png/jpeg/webp/gif/avif, 10 MB) and video/audio (50 MB) return "markdown" to embed; other types (50 MB) land in Files, no markdown, never embed. Storage limits apply.`,
      inputSchema: strictInput({
        issueId: z.string().min(1).optional(),
        filename: z.string().min(1).max(255).optional(),
        contentType: z.string().min(1).max(255).optional(),
        dataBase64: z.string().min(1).optional(),
        alt: z.string().max(500).optional(),
        // EXP-929: the signed path's two extra keys.
        commentId: uuidString.optional(),
        attachmentId: uuidString.optional(),
      }),
    },
    async ({
      issueId: issueIdInput,
      filename: filenameInput,
      contentType: contentTypeInput,
      dataBase64,
      alt,
      commentId,
      // Renamed: the inline path below mints its own `attachmentId`.
      attachmentId: finalizeId,
    }) => {
      try {
        // The finalize call: attachmentId ALONE.
        if (finalizeId !== undefined) {
          if (
            issueIdInput !== undefined ||
            filenameInput !== undefined ||
            contentTypeInput !== undefined ||
            dataBase64 !== undefined ||
            commentId !== undefined
          ) {
            throw new Error(
              `Pass attachmentId alone to finalize a signed upload; the other fields belong to the first call.`
            )
          }
          return ok(
            await finalizeSignedAttachmentUpload({
              attachmentId: finalizeId,
              userId: user.id,
              access,
            })
          )
        }
        if (
          issueIdInput === undefined ||
          filenameInput === undefined ||
          contentTypeInput === undefined
        ) {
          throw new Error(
            `issueId, filename and contentType are required (with dataBase64 to upload now, without it to get a signed upload URL).`
          )
        }
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
        const issueCtx = await assertAttachmentUploadAccess({
          issueId,
          userId: user.id,
          access,
        })

        // The signed path: no bytes in context, an upload URL instead.
        if (dataBase64 === undefined) {
          const origin = process.env.BETTER_AUTH_URL
            ? appBaseUrl()
            : new URL(request.url).origin
          return ok(
            await mintSignedAttachmentUpload({
              issueId,
              teamId: issueCtx.teamId,
              boardId: issueCtx.boardId,
              userId: user.id,
              filename,
              contentType,
              commentId,
              origin,
            })
          )
        }
        if (commentId !== undefined) {
          throw new Error(
            `commentId is for the signed upload; attach an inline upload to a comment through exponential_comments_create's attachmentIds.`
          )
        }

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
        // Inline images and (EXP-824) MP4/MOV media are probed for their
        // size; media also for its duration. A pdf/zip has neither.
        const isMedia = isInlineMediaContentType(contentType)
        const media =
          isMedia && isProbeableVideoContentType(contentType)
            ? getVideoMetadata(body)
            : null
        const dimensions = isImage ? getImageDimensions(body) : media

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
            durationMs: media?.durationMs ?? null,
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
          // Images embed as `![]()`; video/audio (EXP-824) as a plain link
          // on its own paragraph. Other files are NOT markdown-embeddable —
          // they live in the issue's Files list.
          ...(isImage
            ? { markdown: `![${alt ?? ``}](${url})` }
            : isMedia
              ? { markdown: `[${filename}](${url})` }
              : {}),
          filename,
          contentType,
          sizeBytes: body.byteLength,
          width: dimensions?.width ?? null,
          height: dimensions?.height ?? null,
          durationMs: media?.durationMs ?? null,
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
      inputSchema: strictInput({ id: uuidString }),
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
        description: `File a bug report about Exponential itself (the issue tracker — any client, these MCP tools, sync, relays) to the Exponential team. Use it the moment Exponential misbehaves or a tool result misleads you mid-task. Not for issues in the user's own project.`,
        inputSchema: strictInput({
          title: z.string().min(1).max(500),
          description: z.string().min(1).max(10_000),
        }),
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
