import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { contract } from "@exp/domain-contract"
import {
  actionInputsSchema,
  automationTriggerSchema,
  customizableStatusCategoryValues,
  dateOnlySchema,
  DEFAULT_ACCENT_COLOR,
  hexColorSchema,
  MAX_ISSUE_DESCRIPTION,
  UUID_RE,
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
  isNotNull,
  isNull,
  lte,
  notInArray,
  or,
  sql,
  type SQL,
} from "drizzle-orm"
import { db } from "@/db/connection"
import {
  actions,
  attachments,
  automations,
  codingSessions,
  comments,
  issueLabels,
  issues,
  issueStatuses,
  labels,
  notifications,
  boards,
  supportThreads,
  users,
  teamInvites,
  teamMembers,
  teams,
} from "@/db/schema"
import {
  issuePriorityValues,
  issueStatusValues,
  issueStatusCategoryDisplayOrder,
  issueStatusCategoryValues,
  boardIconValues,
} from "@/lib/domain"
import { teamColumns } from "@/lib/team-columns"
import {
  builtinCreateAction,
  builtinFixConflictsAction,
  isBuiltinActionId,
} from "@/lib/builtin-actions"
import {
  assertTeamMember,
  getAttachmentTeamContext,
  getSessionAttachmentTeamContext,
  getIssueTeamContext,
  getBoardTeamId,
  getUserTeamIds,
  resolveTeamAccess,
} from "@/lib/team-membership"
import { boardVisible } from "@/lib/board-visibility"
import { resolveIssueReference } from "@/lib/issue-resolver"
import { issueWireColumns } from "@/lib/issue-columns"
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
import { mintAttachmentToken } from "@/lib/storage/attachment-token"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { assertWithinStorageLimit } from "@/lib/billing"
import { appRouter } from "@/routes/api/trpc/$"
import type { Context } from "@/lib/trpc"
import { createPullRequest, getPullRequest } from "@/lib/integrations/github-pr"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import { isInstallationLinkedToTeam } from "@/lib/trpc/integrations"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { applyPrLifecycleStatusInTx } from "@/lib/integrations/pr-sync"
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"
import {
  claimPrOpen,
  noteAgentIssueActivity,
  releasePrOpenClaim,
} from "@/lib/integrations/pr-actor-claims"
import { composeDeviceList } from "@/lib/steer-devices"
import { escapeLikePattern } from "@/lib/like-pattern"
import { buildRuntimeConfig } from "@/lib/runtime-config"
import { createAgentBugReport } from "@/lib/widget/agent-report"
import { TokenBucketLimiter } from "@/lib/widget/rate-limit"
import { loadRepositoryForTeam } from "@/lib/trpc/repositories"
import { visibleDeviceRows } from "@/lib/trpc/devices"
import { endSessionByAgent } from "@/lib/coding-session-end"
import { getSteerRelayConfig, relayPostInput } from "@/lib/steer"
import {
  formatChildQuestion,
  formatParentAnswer,
  formatStarterMessage,
  loadChildParentContext,
  notifyParentOfChildEnd,
  PARENT_LIVE_STATUSES,
} from "@/lib/steer-child-messages"
import { err, ok } from "./helpers"
import { ALWAYS_LOAD_META } from "./always-load"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "./gates"
import type { McpUser } from "./server"
import {
  assertFullAccess,
  assertBoardGranted,
  assertTeamFullyGranted,
  assertTeamVisible,
  filterVisibleTeamIds,
  grantScopeFilter,
  isBoardGranted,
  isRowGranted,
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

// Automation id → its team, for grant checks on update/toggle/delete
// (EXP-660; owner-ship itself is enforced in the automations router).
async function getAutomationContext(id: string) {
  const [row] = await db
    .select({ teamId: automations.teamId })
    .from(automations)
    .where(eq(automations.id, id))
    .limit(1)
  if (!row) throw new Error(`Automation not found`)
  return row
}

// Support thread id → its team plus that team's helpdesk switch, in ONE
// select (EXP-660). The helpdesk router deliberately never reads the flag
// (REV2-23: disabling freezes threads rather than hiding them), so the MCP
// layer is where a switched-off team refuses the agent.
async function getSupportThreadContext(threadId: string) {
  const [row] = await db
    .select({
      teamId: supportThreads.teamId,
      helpdeskEnabled: teams.helpdeskEnabled,
    })
    .from(supportThreads)
    .innerJoin(teams, eq(teams.id, supportThreads.teamId))
    .where(eq(supportThreads.id, threadId))
    .limit(1)
  if (!row) throw new Error(`Thread not found`)
  return row
}

function assertHelpdeskEnabled(enabled: boolean) {
  if (!enabled) throw new Error(`Helpdesk is not enabled for this team`)
}

// EXP-660: the coding_sessions projection the session tools return — the
// Electric shape allowlist (routes/api/shapes/coding-sessions.ts), camelCased,
// plus the linked issue's identifier/title. `host_user_id` and
// `merged_own_pr` are server-only and stay out; the board mirrors are
// WHERE-only. `acked_at` (EXP-701) is server-only too but IS returned here —
// orchestrating agents are exactly who needs the device's pickup ack.
const sessionColumns = {
  id: codingSessions.id,
  issueId: codingSessions.issueId,
  issueIdentifier: issues.identifier,
  issueTitle: issues.title,
  teamId: codingSessions.teamId,
  boardId: codingSessions.boardId,
  actionId: codingSessions.actionId,
  actionName: codingSessions.actionName,
  startedReason: codingSessions.startedReason,
  automationId: codingSessions.automationId,
  userId: codingSessions.userId,
  deviceLabel: codingSessions.deviceLabel,
  deviceId: codingSessions.deviceId,
  agent: codingSessions.agent,
  status: codingSessions.status,
  branch: codingSessions.branch,
  summary: codingSessions.summary,
  endedBy: codingSessions.endedBy,
  resumedFromId: codingSessions.resumedFromId,
  parentSessionId: codingSessions.parentSessionId,
  needsInput: codingSessions.needsInput,
  ackedAt: codingSessions.ackedAt,
  startedAt: codingSessions.startedAt,
  endedAt: codingSessions.endedAt,
  createdAt: codingSessions.createdAt,
  updatedAt: codingSessions.updatedAt,
}

// How long exponential_sessions_start waits for the device to report the
// row it created off the relay frame (heartbeat-class latency: the frame is
// pushed, the desktop registers the run via codingSessions.start).
const SESSION_START_POLL_MS = 10_000
const SESSION_START_POLL_STEP_MS = 500

function sleep(ms: number) {
  return new Promise<void>((resolve) => setTimeout(resolve, ms))
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
const issueStatusCategoryEnumSchema = z.enum(issueStatusCategoryValues)
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

export function registerExponentialTools(
  server: McpServer,
  user: McpUser,
  request: Request,
  access: McpAccess,
  // EXP-637: the coding_sessions row this MCP request runs inside, parsed
  // from the launcher-injected X-Exp-Session-Id header. Null for every caller
  // that is not a launched agent.
  sessionId: string | null = null,
  // EXP-660: which conditional tool families register for this caller
  // (resolved per request by the route). Defaults to everything so tests and
  // the context budget see the whole surface.
  gates: McpToolGates = ALL_MCP_TOOL_GATES
) {
  // The header session, but only when it is really THIS caller's run — owner
  // or host (EXP-432: a shared-device run is requester-owned while the
  // hosting daemon's key authenticates the agent). A foreign or vanished id
  // resolves to null and every caller degrades to its pre-EXP-637 behaviour;
  // the header is an identifier, never a credential.
  async function loadCallerSession(): Promise<{
    id: string
    teamId: string | null
    // EXP-639: what pr_merge needs to tell the run's OWN PR from any other —
    // its issue, the branch pr_open stamped on it, and the state to restore
    // when a merge it stamped for fails.
    issueId: string | null
    branch: string | null
    status: string
    needsInput: boolean
    mergedOwnPr: boolean
  } | null> {
    if (!sessionId) return null
    const [row] = await db
      .select({
        id: codingSessions.id,
        teamId: codingSessions.teamId,
        issueId: codingSessions.issueId,
        branch: codingSessions.branch,
        status: codingSessions.status,
        needsInput: codingSessions.needsInput,
        mergedOwnPr: codingSessions.mergedOwnPr,
        userId: codingSessions.userId,
        hostUserId: codingSessions.hostUserId,
      })
      .from(codingSessions)
      .where(eq(codingSessions.id, sessionId))
      .limit(1)
    if (!row) return null
    if (row.userId !== user.id && row.hostUserId !== user.id) return null
    return {
      id: row.id,
      teamId: row.teamId,
      issueId: row.issueId ?? null,
      branch: row.branch ?? null,
      status: row.status,
      needsInput: Boolean(row.needsInput),
      mergedOwnPr: Boolean(row.mergedOwnPr),
    }
  }

  // Park the run that just opened a PR in `in_review` and stamp the PR's head
  // branch on it (EXP-545: the row↔PR linkage clients tie their Merge
  // shortcut to). The EXP-637 session header names the EXACT row; a call
  // without one parks NOTHING (EXP-710 removed the pre-EXP-637 heuristic
  // over the caller's issue-less running rows — two concurrent batch runs by
  // one user in one team were indistinguishable to it). `needsInput` resets
  // with the flip like the per-issue path (EXP-531).
  async function parkSessionInReview(
    tx: Parameters<Parameters<typeof db.transaction>[0]>[0],
    opts: {
      callerSessionId: string | null
      headBranch: string
    }
  ): Promise<void> {
    if (!opts.callerSessionId) return
    await tx
      .update(codingSessions)
      .set({
        status: `in_review` as const,
        branch: opts.headBranch,
        needsInput: false,
        updatedAt: new Date(),
      })
      .where(
        and(
          eq(codingSessions.id, opts.callerSessionId),
          eq(codingSessions.status, `running`)
        )
      )
  }

  // -----------------------------------------------------------------------
  // Teams
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_teams_list`,
    {
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

  server.registerTool(
    `exponential_issues_list`,
    {
      // EXP-684: every filter a scheduled sweep needs server-side. The schema
      // is budget-trimmed (context-budget.test.ts): value lists appear once,
      // their exclude* twins and priority validate at runtime.
      description: `List issues. Custom statuses filter by statusId/statusCategory (exponential_statuses_list); exclude* invert. created*/updated*: ISO datetime. sort: [-]createdAt|updatedAt|priority. search: title substring; assigneeId null = unassigned.`,
      inputSchema: strictInput({
        boardId: uuidString.optional(),
        boardIds: z.array(uuidString).optional(),
        teamId: uuidString.optional(),
        status: z.array(issueStatusEnumSchema).optional(),
        statusId: z.array(uuidString).optional(),
        statusCategory: z.array(issueStatusCategoryEnumSchema).optional(),
        excludeStatus: z.array(looseEnum(issueStatusValues)).optional(),
        excludeStatusId: z.array(uuidString).optional(),
        excludeStatusCategory: z
          .array(looseEnum(issueStatusCategoryValues))
          .optional(),
        priority: z.array(looseEnum(issuePriorityValues)).optional(),
        assigneeId: z.string().nullable().optional(),
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
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({
      boardId,
      boardIds,
      teamId,
      status,
      statusId,
      statusCategory,
      excludeStatus,
      excludeStatusId,
      excludeStatusCategory,
      priority,
      assigneeId,
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
    }) => {
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

        if (priority && priority.length > 0) {
          conditions.push(inArray(issues.priority, priority))
        }
        if (assigneeId === null) {
          conditions.push(isNull(issues.assigneeId))
        } else if (assigneeId !== undefined) {
          conditions.push(eq(issues.assigneeId, assigneeId))
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
        if (search) {
          conditions.push(ilike(issues.title, `%${escapeLikePattern(search)}%`))
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
      description: `Create a new issue in a board the MCP user has access to. Description must be plain text (no embedded images on creation). For a custom status pass statusId (not status); see exponential_statuses_list.`,
      inputSchema: strictInput({
        boardId: uuidString,
        title: z.string().min(1).max(500),
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
        labelIds: z.array(uuidString).optional(),
      }),
    },
    async ({ description, ...rest }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(rest.boardId)
          assertBoardGranted(access, board.id, board.teamId)
        }
        const result = await caller(user, request).issues.create({
          ...rest,
          description: description ? description : undefined,
        })
        // EXP-617: an issue filed mid-session has no coding_sessions row of
        // its own, so nothing else ties its later PR back to the human whose
        // agent wrote it. Exclusion-only (see noteAgentIssueActivity).
        noteAgentIssueActivity(result.issue.id, user.id)
        return ok(result.issue)
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

  server.registerTool(
    `exponential_attachments_get`,
    {
      description: `Fetch an attachment by id — every content type. Markdown embeds look like ![alt](/api/attachments/{id}); pass that {id}. Always returns metadata plus a short-lived signed downloadUrl: fetch it (curl/wget) into your working directory to read non-image files (xlsx, PDF, CSV, ...) with real tooling. Images additionally come back as inline image content; small text files include their text inline.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        // Issue attachments first; steer images (EXP-702) live in the
        // server-only session_attachments table but
        // share the /api/attachments/{id} url shape, so an agent's fallback
        // fetch of a steered embed must resolve here too.
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
            if (error instanceof TRPCError && error.code === `NOT_FOUND`) {
              const sessionAttachment =
                await getSessionAttachmentTeamContext(id)
              assertTeamFullyGranted(access, sessionAttachment.teamId)
              return sessionAttachment
            }
            throw error
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

        const contentType = attachment.contentType
        const isImage = contentType.startsWith(`image/`)
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
          return {
            content: [
              {
                type: `image` as const,
                data: Buffer.from(bytes).toString(`base64`),
                mimeType: contentType,
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
  // Comments
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_comments_list`,
    {
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

  server.registerTool(
    `exponential_comments_create`,
    {
      description: `Post a regular comment on an issue (by UUID or human identifier, e.g. "MET-12") authored by the MCP user. Body is plain text.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        body: z.string().trim().min(1).max(10_000).describe(`Plain GFM text`),
        attachmentIds: z.array(uuidString).max(10).optional(),
      }),
    },
    async ({ issueId: issueIdInput, body, attachmentIds }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).comments.create({
          issueId,
          body,
          ...(attachmentIds ? { attachmentIds } : {}),
        })
        noteAgentIssueActivity(issueId, user.id)
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
      description: `Set an issue's status (UUID or identifier). Pass status (builtin enum) or statusId (a team status row from exponential_statuses_list). Status changes are normally AUTOMATIC — PR open/merge apply the team's configured status automation — so set one directly only when the user explicitly asks.`,
      _meta: ALWAYS_LOAD_META,
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

  server.registerTool(
    `exponential_pr_open`,
    {
      description: `Open a GitHub PR on the linked repository via the GitHub App (no 'gh' or token) and link it to the issue(s). Pass EXACTLY ONE of 'issueId', 'issueIds' (batch: ONE combined PR for all listed issues, same repo; 'head' then REQUIRED, e.g. 'exp/batch-<id>'), or 'repositoryId' + 'head' for a PR with no issue (nothing is linked or moved). Single issue: 'head' defaults to the issue's branch or 'exp/<IDENTIFIER>'; 'base' to the repo default branch. Linked issues record prUrl/prNumber/prState/branch and move to the team's PR-open status (default 'in_review'); merging later moves them to the PR-merge status (default 'done'). Accepts UUIDs or identifiers ("MET-12").`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1).optional(),
        issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
        repositoryId: uuidString.optional(),
        title: z.string().min(1).max(255),
        body: z.string().max(60_000).optional(),
        head: z.string().max(255).optional(),
        base: z.string().max(255).optional(),
      }),
    },
    async ({ issueId, issueIds, repositoryId, title, body, head, base }) => {
      try {
        const subjects = [
          Boolean(issueId),
          Boolean(issueIds?.length),
          Boolean(repositoryId),
        ].filter(Boolean).length
        if (subjects !== 1) {
          throw new Error(
            `Provide exactly one of issueId, issueIds or repositoryId`
          )
        }
        if ((issueIds?.length || repositoryId) && !head) {
          throw new Error(
            `'head' is required with issueIds or repositoryId. Pass the pushed branch.`
          )
        }

        // EXP-626: the issue-LESS chore PR. Nothing is linked and nothing
        // moves — no issue events, no PR-open status flip, no notifications,
        // no agent-activity note (there is no issue to note against). The
        // only side effect beyond the PR itself is parking the CALLING
        // session in `in_review`, and that needs the session header — there
        // is no batch heuristic to fall back on here.
        if (repositoryId) {
          const repo = await loadRepositoryForTeam(repositoryId)
          assertTeamFullyGranted(access, repo.teamId)
          await resolveTeamAccess(user.id, repo.teamId)

          const resolvedRepo = await resolveRepoInstallationTokenInfo(
            repo.fullName
          )
          if (!resolvedRepo) {
            throw new Error(
              `The Exponential GitHub App is not installed on ${repo.fullName}.`
            )
          }
          if (
            !(await isInstallationLinkedToTeam(
              repo.teamId,
              resolvedRepo.installationId
            ))
          ) {
            throw new Error(
              `${repo.fullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`
            )
          }

          claimPrOpen(repo.fullName, head!, {
            userId: user.id,
            viaAgent: true,
          })
          let createdPr: Awaited<ReturnType<typeof createPullRequest>>
          try {
            createdPr = await createPullRequest({
              repo: repo.fullName,
              head: head!,
              base: base ?? repo.defaultBranch,
              title,
              body: body ?? ``,
              token: resolvedRepo.token,
            })
          } catch (e) {
            releasePrOpenClaim(repo.fullName, head!)
            throw e
          }

          const callerSession = await loadCallerSession()
          if (callerSession) {
            await db.transaction(async (tx) => {
              await parkSessionInReview(tx, {
                callerSessionId: callerSession.id,
                headBranch: head!,
              })
            })
          }

          return ok({ url: createdPr.url, number: createdPr.number })
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
          // EXP-712: boards on one repo may develop on different branches —
          // a combined PR has exactly one base.
          if (!base && repo && repo.defaultBranch !== issueRepo.defaultBranch) {
            throw new Error(
              `All issues in a batch PR must share one base branch (${repo.defaultBranch} vs ${issueRepo.defaultBranch}). Pass 'base' to pick one.`
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
        // EXP-617 backstop: the claim is keyed on the head branch, so it is
        // lost whenever the branch we compute here is not byte-identical to
        // the `head` GitHub reports back. The issue-keyed record has no such
        // dependency, and it only ever suppresses — never names.
        for (const id of ids) noteAgentIssueActivity(id, user.id)
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

        const callerSession = await loadCallerSession()
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
          // them — park the CALLER's batch session instead (EXP-194), with
          // the PR's head branch stamped on it (EXP-545: the row↔PR linkage
          // clients tie their Merge shortcut to). The EXP-637 session header
          // names the exact row; a headerless caller parks nothing.
          if (issueIds?.length) {
            await parkSessionInReview(tx, {
              callerSessionId: callerSession?.id ?? null,
              headBranch,
            })
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
      description: `Squash-merge open PRs via the GitHub App (no 'gh' or token). Pass EXACTLY ONE of 'issueId', 'issueIds' (one merge per distinct prUrl, so issues sharing a batch PR merge once), or 'repositoryId' + 'prNumber' for a PR with no issue. Linked issues flip to prState='merged' and move to the team's PR-merge status (default 'done'); live coding sessions on them end unless the team's "end sessions on merge" setting is off — 'endSessions' overrides that setting for this call (false keeps them running) — and YOUR OWN session always keeps running (it ends on its own exit or close-out). Merges run sequentially; each results[] element carries 'merged' + optional 'error', plus issueId/identifier (issue path) or repositoryId/prNumber (chore path) — one unmergeable PR never blocks the rest. A merge rejected for a stale base: fix with exponential_pr_retarget first. Idempotent for already-merged PRs.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1).optional(),
        issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
        repositoryId: uuidString.optional(),
        prNumber: z.number().int().positive().optional(),
        endSessions: z.boolean().optional(),
      }),
    },
    async ({ issueId, issueIds, repositoryId, prNumber, endSessions }) => {
      // EXP-711: only forwarded when given, so the tRPC input stays byte-equal
      // to the pre-override shape for every caller that never passes it.
      const endSessionsInput =
        endSessions !== undefined ? { endSessions } : {}
      try {
        const subjects = [
          Boolean(issueId),
          Boolean(issueIds?.length),
          Boolean(repositoryId) || prNumber !== undefined,
        ].filter(Boolean).length
        if (subjects !== 1) {
          throw new Error(
            `Provide exactly one of issueId, issueIds or repositoryId + prNumber`
          )
        }
        if (Boolean(repositoryId) !== (prNumber !== undefined)) {
          throw new Error(`repositoryId and prNumber must be passed together`)
        }

        // EXP-637 decision 6, corrected in EXP-639. A run that merges the PR
        // IT opened must survive its own merge: the durable `merged_own_pr`
        // spare filters every merge-driven end (this call's in-tx sweep,
        // GitHub's webhook, the outbound poller), and the run ends later
        // through exponential_sessions_end or its own exit. `running` is
        // restored with it so the badge reads "coding" again instead of
        // staying parked in review. Two rules the first cut missed:
        //   * ONLY the run's OWN PR may stamp it. The column is durable, so
        //     stamping it while landing a teammate's PR would also spare the
        //     row from the later merge of its own PR — a run nothing ends.
        //   * A merge that never happened may not leave the stamp (nor the
        //     in_review → running flip) behind.
        // The ORDER is forced by the issue path: issues.mergePr runs
        // applyPrMergeState in the SAME call and that in-tx sweep filters on
        // `merged_own_pr = false`, so the stamp has to be committed BEFORE
        // the merge or the run is ended by its own success. Hence
        // stamp-then-merge behind the own-PR check, reverted when the merge
        // it was stamped for does not land.
        const callerSession = await loadCallerSession()
        const stampable =
          callerSession &&
          callerSession.status !== `ended` &&
          !callerSession.mergedOwnPr
            ? callerSession
            : null
        // The state to restore on revert (the stamp only ever applies to
        // these two, and `ended` rows are excluded above).
        const priorStatus =
          stampable?.status === `in_review`
            ? (`in_review` as const)
            : (`running` as const)
        const stampMergedOwnPr = async () => {
          await db
            .update(codingSessions)
            .set({
              mergedOwnPr: true,
              status: `running`,
              needsInput: false,
              updatedAt: new Date(),
            })
            .where(
              and(
                eq(codingSessions.id, stampable!.id),
                inArray(codingSessions.status, [`running`, `in_review`])
              )
            )
        }
        const revertMergedOwnPr = async () => {
          // Put the row back exactly as it was, so the run is still ended by
          // the merge it did NOT perform. Guarded on the stamp itself and on
          // the status this call wrote — a concurrent kill or close-out is
          // never resurrected.
          await db
            .update(codingSessions)
            .set({
              mergedOwnPr: false,
              status: priorStatus,
              needsInput: stampable!.needsInput,
              updatedAt: new Date(),
            })
            .where(
              and(
                eq(codingSessions.id, stampable!.id),
                eq(codingSessions.mergedOwnPr, true),
                eq(codingSessions.status, `running`)
              )
            )
        }

        // EXP-626: the issue-LESS chore PR. repositories.mergePull owns the
        // guards (membership, App config, installation link-gate) and the
        // merge itself; there is no issue row to sync.
        if (repositoryId) {
          // Own-PR test. The ONLY merge-driven end that can reach an
          // issue-less row is the webhook's endSessionsOnMergedBranch, keyed
          // on the `branch` exponential_pr_open stamped when it parked this
          // run on the PR it opened — so the spare is for exactly that row
          // shape: an issue-less run sitting on a branch of its own. But
          // "issue-less run with a branch" is not enough: `repositoryId +
          // prNumber` names no branch, and a chat/batch/action run landing
          // SOMEBODY ELSE'S chore PR would stamp a DURABLE spare that also
          // filters the later merge of its own PR, leaving a run nothing
          // ends. So read the PR's head ref from GitHub and stamp only when
          // it IS the caller's branch. A lookup that cannot answer leaves the
          // stamp off: being ended by a merge is recoverable, a run that
          // never ends is not.
          let ownChorePr = false
          if (stampable && !stampable.issueId && stampable.branch) {
            try {
              const choreRepo = await loadRepositoryForTeam(repositoryId)
              await resolveTeamAccess(user.id, choreRepo.teamId)
              const resolvedRepo = await resolveRepoInstallationTokenInfo(
                choreRepo.fullName
              )
              const pull = await getPullRequest(
                choreRepo.fullName,
                prNumber!,
                resolvedRepo?.token
              )
              ownChorePr = pull.headRef === stampable.branch
            } catch {
              ownChorePr = false
            }
          }
          if (ownChorePr) await stampMergedOwnPr()
          try {
            await caller(user, request).repositories.mergePull({
              repositoryId,
              prNumber: prNumber!,
              ...endSessionsInput,
            })
          } catch (e) {
            if (ownChorePr) await revertMergedOwnPr()
            throw e
          }
          return ok({
            results: [{ repositoryId, prNumber: prNumber!, merged: true }],
          })
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
            // EXP-639: the own-PR test below — a batch/chore run's row carries
            // the head branch, its issues carry the same one.
            branch: issues.branch,
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

        // The PR this run owns: the one on the issue it was launched on, or
        // the one on the branch pr_open stamped on a batch/chore row. Matched
        // over every REQUESTED issue rather than the deduped targets, so a
        // batch PR still counts when a sibling issue ended up representing
        // it; a target then owns the merge when it carries that same prUrl.
        const ownPrUrls = new Set<string>()
        if (stampable) {
          for (const row of rows) {
            const own =
              row.id === stampable.issueId ||
              (stampable.branch !== null && row.branch === stampable.branch)
            if (own && row.prUrl) ownPrUrls.add(row.prUrl)
          }
        }
        const ownTargetIds = new Set(
          targets
            .filter((target) => {
              const prUrl = rowById.get(target.id)?.prUrl
              return Boolean(prUrl && ownPrUrls.has(prUrl))
            })
            .map((target) => target.id)
        )
        if (ownTargetIds.size > 0) await stampMergedOwnPr()

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
            await trpcCaller.issues.mergePr({
              issueId: target.id,
              ...endSessionsInput,
            })
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
        // Only a merge that actually landed earns the spare — an unmergeable
        // PR leaves the row exactly as this call found it.
        if (
          ownTargetIds.size > 0 &&
          !results.some(
            (result) => result.merged && ownTargetIds.has(result.issueId)
          )
        ) {
          await revertMergedOwnPr()
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
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        base: z.string().min(1).max(255).optional(),
      }),
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
        noteAgentIssueActivity(id, user.id)
        return ok({ ok: true, base: result.base })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Sessions (EXP-637)
  // -----------------------------------------------------------------------

  // EXP-679: only an UNATTENDED run gets the close-out tool (gates.sessionsEnd
  // = the header's run is the caller's and carries a started_reason). A
  // person-started run is a conversation — the call would not end it anyway,
  // and offering it just invites the agent to sign off mid-chat.
  if (gates.sessionsEnd) {
    server.registerTool(
      `exponential_sessions_end`,
      {
        description: `Report this run's close-out, shown on the run to the team: a one-paragraph 'summary' of what you did, whether you finished, stopped for a human or changed nothing. Call it LAST, after exponential_pr_open, with the worktree clean: it ends this run. Merging your own PR never ends it; this call does.`,
        _meta: ALWAYS_LOAD_META,
        inputSchema: strictInput({
          summary: z.string().min(1).max(4_000),
        }),
      },
      async ({ summary }) => {
        try {
          if (!sessionId) {
            return err(
              new Error(
                `No coding session: exponential_sessions_end only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
              )
            )
          }
          // Ownership is enforced inside endSessionByAgent (owner or host),
          // which also makes a repeated call idempotent instead of blanking
          // an earlier close-out.
          const result = await endSessionByAgent(db, sessionId, user.id, {
            summary,
          })
          // EXP-700: a just-ended agent-started child reports into its live
          // parent's channel. Only a FIRST real end notifies — alreadyEnded
          // (retries, lost races) never does.
          let reportedToParent = false
          if (result.status === `ended` && !result.alreadyEnded) {
            const { delivered } = await notifyParentOfChildEnd(db, sessionId, {
              summary,
              endedBy: `agent`,
            })
            reportedToParent = delivered
          }
          return ok({ ...result, reportedToParent })
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // EXP-700: only an agent-started run (started_reason='agent') can ask its
  // starter a question — the gate does NOT wait for `parent_session_id`,
  // which the parent stamps only after its sessions_start poll returns; the
  // handler below re-checks the linkage. NON-blocking on purpose — agent
  // CLIs time out long-held tool calls — so the answer arrives later as an
  // injected user message, the same rail a human steers with.
  if (gates.askParent) {
    server.registerTool(
      `exponential_sessions_ask_parent`,
      {
        description: `Ask the run that started this one a question only it can answer; it lands in that run's channel. Non-blocking: on success STOP working and end your turn — the answer arrives later as a user message. Act on it, then still finish with exponential_sessions_end. If delivery fails, finish anyway and note the open question in your summary.`,
        _meta: ALWAYS_LOAD_META,
        inputSchema: strictInput({
          question: z.string().min(1).max(4_000),
        }),
      },
      async ({ question }) => {
        const fallback = `Do not wait for an answer: finish your work, then call exponential_sessions_end and include the open question in your summary.`
        try {
          if (!sessionId) {
            return err(
              new Error(
                `No coding session: exponential_sessions_ask_parent only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
              )
            )
          }
          // The gate is context hygiene; re-check ownership and the linkage.
          const child = await loadChildParentContext(db, sessionId)
          if (
            !child ||
            (child.userId !== user.id && child.hostUserId !== user.id) ||
            child.startedReason !== `agent` ||
            !child.parentSessionId
          ) {
            return err(new Error(`This run has no live starter to ask.`))
          }
          if (
            !child.parentStatus ||
            !(PARENT_LIVE_STATUSES as readonly string[]).includes(
              child.parentStatus
            )
          ) {
            return err(
              new Error(`Your starter's session has ended. ${fallback}`)
            )
          }
          const config = getSteerRelayConfig()
          if (!config) {
            return err(
              new Error(
                `The steer relay is not configured, so the question cannot be delivered. ${fallback}`
              )
            )
          }
          const { delivered } = await relayPostInput(
            config,
            child.parentSessionId,
            formatChildQuestion(child, question)
          )
          if (!delivered) {
            return err(
              new Error(
                `The question could not be delivered to your starter (its session is not reachable). ${fallback}`
              )
            )
          }
          return ok({
            delivered: true,
            note: `Question delivered. Stop working NOW and end your turn; the answer will arrive as a user message. After acting on it, still finish with exponential_sessions_end.`,
          })
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // EXP-660: the session read side. No tRPC list/get exists (clients read the
  // Electric shape), so these are direct reads over the SAME predicate the
  // shape uses: the caller's teams minus trashed/archived boards.
  server.registerTool(
    `exponential_sessions_list`,
    {
      description: `List coding sessions (newest first) across your teams or one team: status, issue, action, branch, device, and once ended the run's own summary/endedBy. mine limits to runs you started or host.`,
      inputSchema: strictInput({
        teamId: uuidString.optional(),
        status: z.enum([`running`, `in_review`, `ended`]).optional(),
        mine: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ teamId, status, mine, limit, offset }) => {
      try {
        let teamIds: string[]
        if (teamId) {
          assertTeamVisible(access, teamId)
          await resolveTeamAccess(user.id, teamId)
          teamIds = [teamId]
        } else {
          teamIds = filterVisibleTeamIds(access, await getUserTeamIds(user.id))
          if (teamIds.length === 0) return ok([])
        }
        // EXP-639: a board-confined grant sees THAT board's runs, not the
        // team's other boards' — team visibility alone is the host-team read
        // the grant hands out for aux lookups, never a licence to list. The
        // ONE encoding lives in scope.ts (grantScopeFilter/isRowGranted); the
        // owner columns are what keeps the board-less runs such a grant may
        // START (a batch spanning its boards) readable by their starter.
        const grantFilter = grantScopeFilter(access, {
          boardCol: codingSessions.boardId,
          teamCol: codingSessions.teamId,
          ownerCols: [codingSessions.userId, codingSessions.hostUserId],
          userId: user.id,
        })
        if (grantFilter === GRANT_MATCHES_NOTHING) return ok([])
        const rows = await db
          .select(sessionColumns)
          .from(codingSessions)
          .leftJoin(issues, eq(issues.id, codingSessions.issueId))
          .where(
            and(
              inArray(codingSessions.teamId, teamIds),
              isNull(codingSessions.boardDeletedAt),
              isNull(codingSessions.boardArchivedAt),
              grantFilter,
              status ? eq(codingSessions.status, status) : undefined,
              mine
                ? or(
                    eq(codingSessions.userId, user.id),
                    eq(codingSessions.hostUserId, user.id)
                  )
                : undefined
            )
          )
          .orderBy(desc(codingSessions.startedAt))
          .limit(limit)
          .offset(offset)
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_get`,
    {
      description: `Get one coding session by id. Poll it after exponential_sessions_start: status running → in_review (PR open) → ended, then summary is the run's own close-out. ackedAt is the device's liveness ack; a null ackedAt does NOT mean dead — devices older than 0.14.29 leave it null for up to 30 minutes. Read ackedAt null with a recent updatedAt as unknown, not failed.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        // `hostUserId` is read for the grant predicate only — it is a
        // server-only column and never reaches the response.
        const [row] = await db
          .select({
            ...sessionColumns,
            hostUserId: codingSessions.hostUserId,
          })
          .from(codingSessions)
          .leftJoin(issues, eq(issues.id, codingSessions.issueId))
          .where(
            and(
              eq(codingSessions.id, id),
              isNull(codingSessions.boardDeletedAt),
              isNull(codingSessions.boardArchivedAt)
            )
          )
          .limit(1)
        if (!row) throw new Error(`Session not found`)
        // EXP-639: the grant confines every read — a connection consented to
        // one board must not read the run its teammate (or it, from another
        // client) started on a sibling board. Board-less runs of the caller's
        // own stay readable inside a visible team, because such a grant can
        // START them. Denied reads as not-found, like a trashed board's row.
        if (!isRowGranted(access, row, user.id)) {
          throw new Error(`Session not found`)
        }
        const { hostUserId: _hostUserId, ...session } = row
        if (row.userId !== user.id) {
          if (!row.teamId) throw new Error(`Session not found`)
          await resolveTeamAccess(user.id, row.teamId)
        }
        return ok(session)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_kill`,
    {
      description: `Abort a live coding session you own or host: the row flips to ended (endedBy user) and the device tears the agent down. Idempotent. Never your own run — it ends on its own exit or close-out.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        // The desktop reads its own row's →ended edge as the kill switch, so
        // an agent killing itself would vanish mid-call without its
        // close-out — refuse and point at the proper exit.
        if (sessionId && id === sessionId) {
          throw new Error(
            `That is your own session: finish your work and exit instead of killing it.`
          )
        }
        if (!access.full) {
          // Same grant predicate as the read side (EXP-639): killing a run
          // on a board this connection was never granted is out of scope,
          // even inside a team it can otherwise see.
          const [row] = await db
            .select({
              teamId: codingSessions.teamId,
              boardId: codingSessions.boardId,
              userId: codingSessions.userId,
              hostUserId: codingSessions.hostUserId,
            })
            .from(codingSessions)
            .where(eq(codingSessions.id, id))
            .limit(1)
          if (!row) throw new Error(`Session not found`)
          if (!isRowGranted(access, row, user.id)) {
            throw new Error(`Session not found`)
          }
        }
        // Owner-or-host, idempotency and the best-effort relay kill all live
        // in steer.killSession; its row is projected — never returned raw.
        const result = await caller(user, request).steer.killSession({
          sessionId: id,
        })
        // EXP-700: a killed run is an agent-started child that will never
        // send its close-out — tell a live parent instead of leaving it
        // waiting forever. `txId` is set only by the call that actually
        // flipped the row, so a repeated (idempotent) kill notifies once.
        // Best-effort: internally caught and relay-timeout bounded.
        if (result.txId != null) {
          await notifyParentOfChildEnd(db, id, {
            summary: null,
            endedBy: `user`,
          })
        }
        return ok({
          ok: true,
          id,
          status: result.session.status,
          endedAt: result.session.endedAt,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-700: send text into a live session's agent — the parent's half of
  // the ask/answer rail (a child asks via exponential_sessions_ask_parent),
  // and a generic owner-scoped steer. Unconditional and deferred on purpose:
  // a header-less expu_ orchestrator must be able to answer runs it started.
  server.registerTool(
    `exponential_sessions_message`,
    {
      description: `Send text into a live coding session you own or host; it arrives as user input to that agent, prefixed with its source. Use it to answer a child run's exponential_sessions_ask_parent question (id = the child's session UUID from the bracketed message) or to steer a run you started. Never your own session.`,
      inputSchema: strictInput({
        id: uuidString,
        message: z.string().min(1).max(4_000),
      }),
    },
    async ({ id: targetId, message }) => {
      try {
        if (sessionId && targetId === sessionId) {
          throw new Error(
            `That is your own session: you cannot message yourself.`
          )
        }
        const [row] = await db
          .select({
            teamId: codingSessions.teamId,
            boardId: codingSessions.boardId,
            userId: codingSessions.userId,
            hostUserId: codingSessions.hostUserId,
            status: codingSessions.status,
            parentSessionId: codingSessions.parentSessionId,
          })
          .from(codingSessions)
          .where(eq(codingSessions.id, targetId))
          .limit(1)
        if (!row) throw new Error(`Session not found`)
        // Same grant predicate as kill (EXP-639): out-of-grant reads as
        // not found, never as forbidden.
        if (!access.full && !isRowGranted(access, row, user.id)) {
          throw new Error(`Session not found`)
        }
        if (row.userId !== user.id && row.hostUserId !== user.id) {
          throw new Error(`Only the session owner or host can message it`)
        }
        if (!(PARENT_LIVE_STATUSES as readonly string[]).includes(row.status)) {
          throw new Error(`Session is not live`)
        }
        // A parent answering its own child gets the answer prefix the ask
        // told the child to expect; every other caller is "your starter".
        const text =
          sessionId && row.parentSessionId === sessionId
            ? formatParentAnswer(sessionId, message)
            : formatStarterMessage(message)
        const config = getSteerRelayConfig()
        if (!config) throw new Error(`The steer relay is not configured`)
        const { delivered } = await relayPostInput(config, targetId, text)
        if (!delivered) {
          throw new Error(
            `Not delivered: the session's device is not connected to the relay. The run may still be starting or its device offline; retry, or fall back to exponential_sessions_get.`
          )
        }
        return ok({ ok: true, id: targetId, delivered: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-552: remote start over the steer rails, the way the Start-coding
  // dialog and the mobile clients do it. steer.startSession validates the
  // one-of rule, the per-agent model/effort vocabulary, the device's caps and
  // installed agents, and resolves repos; a 404 from the relay means the
  // device is offline. The device then creates the coding_sessions row
  // itself, so the tool waits briefly for it to appear and hands back its id.
  server.registerTool(
    `exponential_sessions_start`,
    {
      description: `Start a coding session on a registered ONLINE device (exponential_devices_list; agents includes the agent). Offline devices are refused: starts are live, never queued. Exactly one subject: issueId (UUID or identifier), issueIds (one batch PR), actionId (+teamId for builtins, inputs) or resumeSessionId (relaunch an ended run). The run gets its own worktree and PR; track it with exponential_sessions_get. The device creates the session row itself; sessionId null = it never reported the run, treat the start as lost. ackedAt null is NOT dead: devices older than 0.14.29 leave it null up to 30 min — with a recent updatedAt read it as unknown. Started from inside a run, the child is unattended: its question or its finish lands in THIS session as '[Exponential child run ...]' user input — answer with exponential_sessions_message. Read its report before merging its PR (merging first ends the run unreported); polling sessions_get is only a fallback.`,
      inputSchema: strictInput({
        deviceId: z.string().min(1).max(128),
        issueId: z.string().min(1).optional(),
        issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
        actionId: z.string().min(1).optional(),
        teamId: uuidString.optional(),
        inputs: z.record(z.string(), z.string()).optional(),
        resumeSessionId: uuidString.optional(),
        agent: z.enum(codingAgentValues).optional(),
        model: z.string().max(64).optional(),
        effort: z.string().max(32).optional(),
        planMode: z.boolean().optional(),
        ultracode: z.boolean().optional(),
      }),
    },
    async (input) => {
      try {
        const startedAfter = new Date()
        let issueId: string | undefined
        let issueIds: string[] | undefined
        // The row the device will create, by subject — what the poll below
        // keys on besides user, device and freshness.
        let match: SQL | undefined
        if (input.issueId) {
          issueId = await resolveIssueId(input.issueId, user.id, access)
          const ctx = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctx.boardId, ctx.teamId)
          match = eq(codingSessions.issueId, issueId)
        } else if (input.issueIds) {
          // EXP-707: identifiers accepted here like pr_open/pr_merge.
          issueIds = await Promise.all(
            input.issueIds.map((id) => resolveIssueId(id, user.id, access))
          )
          const contexts = await Promise.all(
            issueIds.map((id) => getIssueTeamContext(id))
          )
          for (const ctx of contexts) {
            assertBoardGranted(access, ctx.boardId, ctx.teamId)
          }
          // A batch row is issue-less and action-less in the batch's team.
          match = and(
            inArray(codingSessions.teamId, [
              ...new Set(contexts.map((ctx) => ctx.teamId)),
            ]),
            isNull(codingSessions.issueId),
            isNull(codingSessions.actionId)
          )
        } else if (input.actionId) {
          if (isBuiltinActionId(input.actionId)) {
            // The router requires teamId here; a builtin run is an issue-less
            // row carrying the action name snapshot but no action FK.
            if (input.teamId) assertTeamFullyGranted(access, input.teamId)
            match = and(
              input.teamId ? eq(codingSessions.teamId, input.teamId) : undefined,
              isNull(codingSessions.issueId),
              isNotNull(codingSessions.actionName)
            )
          } else {
            const action = await getActionContext(input.actionId)
            assertTeamFullyGranted(access, action.teamId)
            match = eq(codingSessions.actionId, input.actionId)
          }
        } else if (input.resumeSessionId) {
          // Same predicate as the read side: a board-confined grant may
          // relaunch a run it can also get/list/kill — including the
          // board-less ones it started itself (steer.startSession keeps the
          // resume owner-only on top).
          const [row] = await db
            .select({
              teamId: codingSessions.teamId,
              boardId: codingSessions.boardId,
              userId: codingSessions.userId,
              hostUserId: codingSessions.hostUserId,
            })
            .from(codingSessions)
            .where(eq(codingSessions.id, input.resumeSessionId))
            .limit(1)
          if (!row) throw new Error(`Session not found`)
          if (!isRowGranted(access, row, user.id)) {
            throw new Error(`Session not found`)
          }
          match = eq(codingSessions.resumedFromId, input.resumeSessionId)
        } else {
          throw new Error(
            `Exactly one of issueId, issueIds, actionId or resumeSessionId is required`
          )
        }

        await caller(user, request).steer.startSession({
          ...input,
          issueId,
          issueIds,
          // EXP-679: a run started from inside a run is that run's child.
          ...(sessionId ? { parentSessionId: sessionId } : {}),
        })

        // The relay accepted the frame; the desktop registers the run via
        // codingSessions.start moments later. Wait for it so the caller can
        // track the run by id instead of guessing from a list.
        const deadline = Date.now() + SESSION_START_POLL_MS
        let session: Record<string, unknown> | null = null
        for (;;) {
          const [row] = await db
            .select(sessionColumns)
            .from(codingSessions)
            .leftJoin(issues, eq(issues.id, codingSessions.issueId))
            .where(
              and(
                eq(codingSessions.userId, user.id),
                eq(codingSessions.deviceId, input.deviceId),
                eq(codingSessions.status, `running`),
                gte(codingSessions.createdAt, startedAfter),
                match
              )
            )
            .orderBy(desc(codingSessions.createdAt))
            .limit(1)
          if (row) {
            // EXP-679: the device creates the row, so the parent link is
            // stamped here — history only, never worth failing the start.
            if (sessionId && !row.parentSessionId) {
              try {
                await db
                  .update(codingSessions)
                  .set({ parentSessionId: sessionId })
                  .where(eq(codingSessions.id, row.id))
                row.parentSessionId = sessionId
              } catch {
                // ignored
              }
            }
            session = row
            break
          }
          if (Date.now() >= deadline) break
          await sleep(SESSION_START_POLL_STEP_MS)
        }
        return ok({
          ok: true,
          deviceId: input.deviceId,
          sessionId: (session?.id as string | undefined) ?? null,
          session,
          // EXP-700: a child started from inside a run reports back
          // event-based (every supported device brands it agent-started —
          // steer.startSession refuses a host that does not).
          ...(sessionId && session
            ? {
                note: `The child reports into this session as a bracketed [Exponential child run ...] user message when it finishes or asks a question; answer questions with exponential_sessions_message. Poll exponential_sessions_get only as a fallback.`,
              }
            : {}),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Devices (EXP-660: the picker for exponential_sessions_start)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_devices_list`,
    {
      description: `List your registered machines (desktop app / CLI daemon), plus servers teammates shared with teamId. Pick an online device whose agents includes the agent you want; caps must include resume-run to resume an ended run.`,
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
            sharedTeamId: device.sharedTeamId,
            isDefault: device.isDefault,
            // EXP-484: per-agent sign-in status and usage windows as the
            // machine last probed them (absent on builds without the
            // collector).
            agentAccounts: device.agentAccounts ?? null,
            agentUsage: device.agentUsage ?? null,
            agentUsageAt: device.agentUsageAt ?? null,
            ...(device.owner ? { owner: device.owner } : {}),
          }))
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
      description: `List the MCP user's own notifications, newest first. Set unreadOnly to show only those not yet read.`,
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
      description: `List the members of a team. id is the USER id (use it for assigneeId); memberId is the team_members row id (what teamMembers.updateRole/remove take).`,
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

  // -----------------------------------------------------------------------
  // Repositories
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_repositories_list`,
    {
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

  server.registerTool(
    `exponential_repositories_branch_diff`,
    {
      description: `Get the diff of an issue's exp/<IDENTIFIER> branch against the repo's default branch (UUID or identifier). Returns null when the branch was never pushed. Team members only.`,
      inputSchema: strictInput({ issueId: z.string().min(1) }),
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
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        // FULL team grant even for the read: action bodies are locally
        // executed operational prompts, not board-workflow aux data — a
        // board-confined OAuth token has no business reading them (the
        // run_configs precedent confined list the same way).
        if (!access.full) assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).actions.list({ teamId })
        // EXP-539: actions.list stopped appending the virtual builtins
        // (native clients construct them locally); agents still need them
        // listed, so this tool appends both.
        return ok(
          page(
            [
              ...result.actions,
              builtinCreateAction(teamId),
              builtinFixConflictsAction(teamId),
            ],
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
    `exponential_actions_create`,
    {
      description: `Create a team action (owner only). body = the markdown prompt an agent runs locally; repositoryId targets that repo's trunk clone; icon = a curated icon name; inputs = run-dialog fields injected into the prompt.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        teamId: uuidString,
        name: z.string().min(1).max(255),
        description: z.string().nullable().optional(),
        icon: boardIconEnumSchema.nullable().optional(),
        repositoryId: uuidString.nullable().optional(),
        body: z.string().min(1),
        inputs: actionInputsSchema.optional(),
      }),
    },
    async (input) => {
      try {
        if (!access.full) assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).actions.create(input)
        return ok(result.action)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_update`,
    {
      description: `Update an action by UUID (owner only); pass only fields to change. icon: null clears; inputs: whole-array replace.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        description: z.string().nullable().optional(),
        icon: boardIconEnumSchema.nullable().optional(),
        repositoryId: uuidString.nullable().optional(),
        body: z.string().min(1).optional(),
        inputs: actionInputsSchema.optional(),
        sortOrder: z.number().finite().optional(),
      }),
    },
    async (input) => {
      try {
        if (!access.full) {
          const action = await getActionContext(input.id)
          assertTeamFullyGranted(access, action.teamId)
        }
        const result = await caller(user, request).actions.update(input)
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
      inputSchema: strictInput({ id: uuidString }),
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
  // Automations (EXP-583): schedule/event trigger → action on a device
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_automations_create`,
    {
      description: `Create an automation (owner only) running actionId on deviceId; pass provided values verbatim. trigger = {kind:schedule,interval:daily|weekly|monthly,minuteOfDay,weekday?,dayOfMonth?} or {kind:event,event:created|status_changed|assignee_changed|label_added|priority_changed|pr_opened|pr_merged,filters?}.`,
      inputSchema: strictInput({
        teamId: uuidString,
        actionId: uuidString,
        deviceId: z.string().min(1).max(128),
        trigger: z.record(z.string(), z.unknown()),
        // Null and absent both mean the device's launch defaults — the same
        // nullability contract as automations_update (EXP-707 theme F).
        agent: z.enum(codingAgentValues).nullable().optional(),
        model: z.string().max(64).nullable().optional(),
        effort: z.string().max(32).nullable().optional(),
      }),
    },
    async (input) => {
      try {
        if (!access.full) assertTeamFullyGranted(access, input.teamId)
        // Declared loose to stay inside the MCP context budget; the strict
        // union validates here (and again in the router — single source).
        const trigger = automationTriggerSchema.parse(input.trigger)
        const result = await caller(user, request).automations.create({
          ...input,
          trigger,
        })
        return ok(result.automation)
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-660: the rest of the automations surface. Owner checks, the
  // enabled⇒no-required-inputs rule, device/agent validation and the
  // trigger union all live in the router; these add the grant check only.
  server.registerTool(
    `exponential_automations_list`,
    {
      description: `List a team's automations: which action runs on which device, its trigger (schedule or issue event), launch agent/model/effort and whether it is enabled. Team members only.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        // Rows name devices and locally executed actions — team-level
        // operational data, gated like actions_list.
        if (!access.full) assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).automations.list({ teamId })
        return ok(page(result.automations, limit, offset))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_automations_update`,
    {
      description: `Update an automation (owner only); pass only the fields to change. trigger takes the same shape as exponential_automations_create; null agent/model/effort clears the pin. An enabled automation needs every action input optional.`,
      inputSchema: strictInput({
        id: uuidString,
        actionId: uuidString.optional(),
        deviceId: z.string().min(1).max(128).optional(),
        trigger: z.record(z.string(), z.unknown()).optional(),
        enabled: z.boolean().optional(),
        sortOrder: z.number().finite().optional(),
        agent: z.enum(codingAgentValues).nullable().optional(),
        model: z.string().max(64).nullable().optional(),
        effort: z.string().max(32).nullable().optional(),
      }),
    },
    async (input) => {
      try {
        if (!access.full) {
          const automation = await getAutomationContext(input.id)
          assertTeamFullyGranted(access, automation.teamId)
        }
        // Loose in the schema for the context budget; the strict union
        // validates here (and again in the router — single source).
        const trigger =
          input.trigger === undefined
            ? undefined
            : automationTriggerSchema.parse(input.trigger)
        const result = await caller(user, request).automations.update({
          ...input,
          trigger,
        })
        return ok(result.automation)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_automations_toggle`,
    {
      description: `Enable or disable an automation (owner only) without touching its trigger, device or action. Enabling needs every input of the action optional.`,
      inputSchema: strictInput({ id: uuidString, enabled: z.boolean() }),
    },
    async ({ id, enabled }) => {
      try {
        if (!access.full) {
          const automation = await getAutomationContext(id)
          assertTeamFullyGranted(access, automation.teamId)
        }
        const result = await caller(user, request).automations.update({
          id,
          enabled,
        })
        return ok(result.automation)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_automations_delete`,
    {
      description: `Delete an automation (owner only). Past runs keep their history; nothing else is touched.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const automation = await getAutomationContext(id)
          assertTeamFullyGranted(access, automation.teamId)
        }
        await caller(user, request).automations.delete({ id })
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
      inputSchema: strictInput({ issueId: z.string().min(1) }),
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
          // team registry (pr_open / pr_files / branch_diff then reach
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
      description: `Update a team's name or icon (by its UUID). Team owner only. Teams are always private.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        iconUrl: z.string().url().max(2048).nullable().optional(),
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
      description: `Create an invite link for a team, returning the token to share. Owner only. Pass email to have the server mail the link (emailDelivered reports the attempt).`,
      inputSchema: strictInput({
        teamId: uuidString,
        role: z.enum([`owner`, `member`]).default(`member`),
        email: z.string().email().max(255).optional(),
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
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_invites_list`,
    {
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
  // Attachments upload (base64 payload → S3 → attachments row)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_attachments_upload`,
    {
      description: `Upload a base64-encoded file and attach it to an issue (UUID or identifier). Images (png/jpeg/webp/gif/avif, max 10 MB) also return a "markdown" field. Embed that string to show the image. Other types (max 50 MB) land in the issue's Files list, return no markdown, and must not be embedded. Storage limits apply; base64 inflates ~33%.`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        filename: z.string().min(1).max(255),
        contentType: z.string().min(1).max(255),
        dataBase64: z.string().min(1),
        alt: z.string().max(500).optional(),
      }),
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

  // -----------------------------------------------------------------------
  // Helpdesk (EXP-660): support tickets filed through the widget
  // -----------------------------------------------------------------------
  // Registered only when at least one team this caller could use them in
  // has helpdesk switched on (gates.helpdesk, resolved per request by the
  // route) — a family of tools the agent can never call is context noise.
  // That is hygiene, not the boundary: every tool re-checks the SPECIFIC
  // team's flag (the router never does, REV2-23), membership lives in the
  // router, and reads need a FULL team grant like writes — threads carry
  // reporter email/name, not board-workflow aux data.
  if (gates.helpdesk) {
    server.registerTool(
      `exponential_helpdesk_threads_list`,
      {
        description: `List a team's support tickets (newest activity first) with their last message and an unread flag. Page with cursor = the oldest loaded row's updatedAt. Team members only; needs helpdesk enabled.`,
        inputSchema: strictInput({
          teamId: uuidString,
          filter: z.enum([`open`, `resolved`]).default(`open`),
          limit: z.number().int().min(1).max(200).default(50),
          cursor: isoDateTime.optional(),
        }),
      },
      async (input) => {
        try {
          assertTeamFullyGranted(access, input.teamId)
          const [team] = await db
            .select({ helpdeskEnabled: teams.helpdeskEnabled })
            .from(teams)
            .where(eq(teams.id, input.teamId))
            .limit(1)
          if (!team) throw new Error(`Team not found`)
          assertHelpdeskEnabled(team.helpdeskEnabled)
          const result = await caller(user, request).helpdesk.listThreads({
            ...input,
            cursor: input.cursor ? new Date(input.cursor) : undefined,
          })
          return ok(result)
        } catch (e) {
          return err(e)
        }
      }
    )

    server.registerTool(
      `exponential_helpdesk_threads_get`,
      {
        description: `Get a support ticket with its full conversation (public replies and internal notes, each with its email delivery status) and the escalated issue if any.`,
        inputSchema: strictInput({ id: uuidString }),
      },
      async ({ id }) => {
        try {
          const thread = await getSupportThreadContext(id)
          assertTeamFullyGranted(access, thread.teamId)
          assertHelpdeskEnabled(thread.helpdeskEnabled)
          const result = await caller(user, request).helpdesk.getThread({
            threadId: id,
          })
          return ok(result)
        } catch (e) {
          return err(e)
        }
      }
    )

    server.registerTool(
      `exponential_helpdesk_reply`,
      {
        description: `Post a public reply on a support ticket: the reporter sees it on their magic-link page and gets it emailed (once they have opened the link and are not viewing right now). Replying to a resolved ticket reopens it.`,
        inputSchema: strictInput({
          id: uuidString,
          body: z.string().trim().min(1).max(10_000),
        }),
      },
      async ({ id, body }) => {
        try {
          const thread = await getSupportThreadContext(id)
          assertTeamFullyGranted(access, thread.teamId)
          assertHelpdeskEnabled(thread.helpdeskEnabled)
          const result = await caller(user, request).helpdesk.reply({
            threadId: id,
            body,
          })
          return ok(result)
        } catch (e) {
          return err(e)
        }
      }
    )

    server.registerTool(
      `exponential_helpdesk_note`,
      {
        description: `Add an internal note to a support ticket: visible to team members only, never emailed, never shown to the reporter.`,
        inputSchema: strictInput({
          id: uuidString,
          body: z.string().trim().min(1).max(10_000),
        }),
      },
      async ({ id, body }) => {
        try {
          const thread = await getSupportThreadContext(id)
          assertTeamFullyGranted(access, thread.teamId)
          assertHelpdeskEnabled(thread.helpdeskEnabled)
          const result = await caller(user, request).helpdesk.note({
            threadId: id,
            body,
          })
          return ok(result.message)
        } catch (e) {
          return err(e)
        }
      }
    )

    server.registerTool(
      `exponential_helpdesk_close`,
      {
        description: `Resolve a support ticket: the transcript stays readable but the reporter's magic link stops accepting replies. An escalated issue is untouched.`,
        inputSchema: strictInput({ id: uuidString }),
      },
      async ({ id }) => {
        try {
          const thread = await getSupportThreadContext(id)
          assertTeamFullyGranted(access, thread.teamId)
          assertHelpdeskEnabled(thread.helpdeskEnabled)
          await caller(user, request).helpdesk.close({ threadId: id })
          return ok({ ok: true, id })
        } catch (e) {
          return err(e)
        }
      }
    )

    server.registerTool(
      `exponential_helpdesk_reopen`,
      {
        description: `Reopen a resolved support ticket; the reporter's existing magic link works again.`,
        inputSchema: strictInput({ id: uuidString }),
      },
      async ({ id }) => {
        try {
          const thread = await getSupportThreadContext(id)
          assertTeamFullyGranted(access, thread.teamId)
          assertHelpdeskEnabled(thread.helpdeskEnabled)
          await caller(user, request).helpdesk.reopen({ threadId: id })
          return ok({ ok: true, id })
        } catch (e) {
          return err(e)
        }
      }
    )

    server.registerTool(
      `exponential_helpdesk_escalate`,
      {
        description: `File an issue from a support ticket on a board of the ticket's team and link them (one escalation per ticket). The issue opens with the reporter's message as its description; title defaults to the ticket's.`,
        inputSchema: strictInput({
          id: uuidString,
          boardId: uuidString,
          title: z.string().trim().min(1).max(500).optional(),
        }),
      },
      async ({ id, ...rest }) => {
        try {
          const thread = await getSupportThreadContext(id)
          assertTeamFullyGranted(access, thread.teamId)
          assertHelpdeskEnabled(thread.helpdeskEnabled)
          const result = await caller(user, request).helpdesk.escalate({
            threadId: id,
            ...rest,
          })
          return ok(result.issue)
        } catch (e) {
          return err(e)
        }
      }
    )
  }

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
