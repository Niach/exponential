import { z } from "zod"
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { and, asc, desc, eq, gte, ilike, inArray, isNull, lte } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  comments,
  issueAgentState,
  issueLabels,
  issues,
  labels,
  projects,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import {
  issuePriorityValues,
  issueStatusValues,
  recurrenceUnitValues,
} from "@/lib/domain"
import {
  getIssueWorkspaceContext,
  getProjectWorkspaceId,
  getPublicWorkspaceIds,
  getUserWorkspaceIds,
  resolveWorkspaceAccess,
} from "@/lib/workspace-membership"
import { appRouter } from "@/routes/api/trpc/$"
import type { Context } from "@/lib/trpc"
import { err, ok } from "./helpers"
import type { McpUser } from "./server"

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

const issueStatusEnumSchema = z.enum(issueStatusValues)
const issuePriorityEnumSchema = z.enum(issuePriorityValues)
const recurrenceUnitEnumSchema = z.enum(recurrenceUnitValues)
const dateOnly = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/, `Expected YYYY-MM-DD`)

export function registerExponentialTools(
  server: McpServer,
  user: McpUser,
  request: Request
) {
  // -----------------------------------------------------------------------
  // Workspaces
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_workspaces_list`,
    {
      title: `List workspaces`,
      description: `List workspaces the MCP user is a member of.`,
      inputSchema: {},
    },
    async () => {
      try {
        const memberRows = await db
          .select({
            id: workspaces.id,
            name: workspaces.name,
            slug: workspaces.slug,
            iconUrl: workspaces.iconUrl,
            role: workspaceMembers.role,
            createdAt: workspaces.createdAt,
            updatedAt: workspaces.updatedAt,
          })
          .from(workspaces)
          .innerJoin(
            workspaceMembers,
            eq(workspaceMembers.workspaceId, workspaces.id)
          )
          .where(eq(workspaceMembers.userId, user.id))
          .orderBy(asc(workspaces.name))

        const memberIds = new Set(memberRows.map((row) => row.id))
        const publicIds = await getPublicWorkspaceIds()
        const extraIds = publicIds.filter((id) => !memberIds.has(id))
        const publicOnly: typeof memberRows = []
        if (extraIds.length > 0) {
          const publicRows = await db
            .select({
              id: workspaces.id,
              name: workspaces.name,
              slug: workspaces.slug,
              iconUrl: workspaces.iconUrl,
              createdAt: workspaces.createdAt,
              updatedAt: workspaces.updatedAt,
            })
            .from(workspaces)
            .where(inArray(workspaces.id, extraIds))
            .orderBy(asc(workspaces.name))
          for (const row of publicRows) {
            publicOnly.push({ ...row, role: `public` as never })
          }
        }
        return ok([...memberRows, ...publicOnly])
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_workspaces_get`,
    {
      title: `Get workspace`,
      description: `Get a single workspace by id.`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        await resolveWorkspaceAccess(user.id, id)
        const [row] = await db
          .select()
          .from(workspaces)
          .where(eq(workspaces.id, id))
          .limit(1)
        return ok(row)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Projects
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_projects_list`,
    {
      title: `List projects`,
      description: `List projects in a workspace, or across all workspaces the user belongs to.`,
      inputSchema: {
        workspaceId: z.string().uuid().optional(),
        includeArchived: z.boolean().default(false),
      },
    },
    async ({ workspaceId, includeArchived }) => {
      try {
        let allowedWorkspaceIds: Array<string>
        if (workspaceId) {
          await resolveWorkspaceAccess(user.id, workspaceId)
          allowedWorkspaceIds = [workspaceId]
        } else {
          allowedWorkspaceIds = await getUserWorkspaceIds(user.id)
          if (allowedWorkspaceIds.length === 0) return ok([])
        }

        const rows = await db
          .select()
          .from(projects)
          .where(inArray(projects.workspaceId, allowedWorkspaceIds))
          .orderBy(asc(projects.sortOrder), asc(projects.name))

        const filtered = includeArchived
          ? rows
          : rows.filter((row) => row.archivedAt == null)
        return ok(filtered)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_projects_get`,
    {
      title: `Get project`,
      description: `Get a single project by id.`,
      inputSchema: { id: z.string().uuid() },
    },
    async ({ id }) => {
      try {
        const project = await getProjectWorkspaceId(id)
        await resolveWorkspaceAccess(user.id, project.workspaceId)
        const [row] = await db
          .select()
          .from(projects)
          .where(eq(projects.id, id))
          .limit(1)
        return ok(row)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_projects_create`,
    {
      title: `Create project`,
      description: `Create a project in a workspace. The MCP user must be a member of the workspace.`,
      inputSchema: {
        workspaceId: z.string().uuid(),
        name: z.string().min(1).max(255),
        prefix: z.string().min(1).max(10),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
      },
    },
    async (input) => {
      try {
        const result = await caller(user, request).projects.create(input)
        return ok(result.project)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_projects_update`,
    {
      title: `Update project`,
      description: `Update a project's name, color, or archive state.`,
      inputSchema: {
        id: z.string().uuid(),
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
        const result = await caller(user, request).projects.update(input)
        return ok(result.project)
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
      description: `List issues in projects the MCP user can access. Supports filtering by project, status, priority, assignee, due-date range, and a free-text title search. Defaults to non-archived issues, newest first.`,
      inputSchema: {
        projectId: z.string().uuid().optional(),
        workspaceId: z.string().uuid().optional(),
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
      projectId,
      workspaceId,
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
        let allowedProjectIds: Array<string>

        if (projectId) {
          const project = await getProjectWorkspaceId(projectId)
          await resolveWorkspaceAccess(user.id, project.workspaceId)
          allowedProjectIds = [projectId]
        } else {
          let workspaceIds: Array<string>
          if (workspaceId) {
            await resolveWorkspaceAccess(user.id, workspaceId)
            workspaceIds = [workspaceId]
          } else {
            workspaceIds = await getUserWorkspaceIds(user.id)
          }
          if (workspaceIds.length === 0) return ok([])
          const projectRows = await db
            .select({ id: projects.id })
            .from(projects)
            .where(inArray(projects.workspaceId, workspaceIds))
          allowedProjectIds = projectRows.map((r) => r.id)
        }

        if (allowedProjectIds.length === 0) return ok([])

        const conditions = [inArray(issues.projectId, allowedProjectIds)]
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
        if (search) conditions.push(ilike(issues.title, `%${search}%`))

        const rows = await db
          .select()
          .from(issues)
          .where(and(...conditions))
          .orderBy(desc(issues.createdAt))
          .limit(limit)
          .offset(offset)

        const filtered = includeArchived
          ? rows
          : rows.filter((r) => r.archivedAt == null)
        return ok(filtered)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_get`,
    {
      title: `Get issue`,
      description: `Get a single issue by id, including its label ids, agent-plan fields, and the latest comments on the thread (newest first). The recentComments array is capped at ${`50`}; pass commentsLimit to override.`,
      inputSchema: {
        id: z.string().uuid(),
        commentsLimit: z.number().int().min(0).max(200).optional(),
      },
    },
    async ({ id, commentsLimit }) => {
      try {
        const ctxIssue = await getIssueWorkspaceContext(id)
        await resolveWorkspaceAccess(user.id, ctxIssue.workspaceId)
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
            kind: comments.kind,
            answeredAt: comments.answeredAt,
            createdAt: comments.createdAt,
            editedAt: comments.editedAt,
          })
          .from(comments)
          .where(eq(comments.issueId, id))
          .orderBy(desc(comments.createdAt))
          .limit(commentsLimit ?? 50)
        // Structured agent plan/question text (server-only; not on the issue
        // row). The daemon reads these instead of parsing plan/question comments.
        const [agentState] = await db
          .select({
            agentPlanText: issueAgentState.planText,
            agentQuestion: issueAgentState.question,
          })
          .from(issueAgentState)
          .where(eq(issueAgentState.issueId, id))
          .limit(1)
        return ok({
          ...issue,
          labelIds: labelRows.map((r) => r.labelId),
          recentComments,
          agentPlanText: agentState?.agentPlanText ?? null,
          agentQuestion: agentState?.agentQuestion ?? null,
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
      description: `Create a new issue in a project the MCP user has access to. Description must be plain text (no embedded images on creation).`,
      inputSchema: {
        projectId: z.string().uuid(),
        title: z.string().min(1).max(500),
        status: issueStatusEnumSchema.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        descriptionText: z.string().optional(),
        dueDate: dateOnly.nullable().optional(),
        labelIds: z.array(z.string().uuid()).optional(),
        recurrenceInterval: z.number().int().min(1).nullable().optional(),
        recurrenceUnit: recurrenceUnitEnumSchema.nullable().optional(),
      },
    },
    async ({ descriptionText, ...rest }) => {
      try {
        const result = await caller(user, request).issues.create({
          ...rest,
          description: descriptionText
            ? { text: descriptionText }
            : undefined,
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
        recurrenceInterval: z.number().int().min(1).nullable().optional(),
        recurrenceUnit: recurrenceUnitEnumSchema.nullable().optional(),
      },
    },
    async ({ descriptionText, ...rest }) => {
      try {
        const description =
          descriptionText === undefined
            ? undefined
            : descriptionText === null
              ? null
              : { text: descriptionText }
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
        await caller(user, request).issues.delete(input)
        return ok({ ok: true, id: input.id })
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
      description: `List labels for a workspace.`,
      inputSchema: { workspaceId: z.string().uuid() },
    },
    async ({ workspaceId }) => {
      try {
        await resolveWorkspaceAccess(user.id, workspaceId)
        const rows = await db
          .select()
          .from(labels)
          .where(eq(labels.workspaceId, workspaceId))
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
      description: `Get a label by id (must be in a workspace the user belongs to).`,
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
        await resolveWorkspaceAccess(user.id, label.workspaceId)
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
      description: `Create a label in a workspace.`,
      inputSchema: {
        workspaceId: z.string().uuid(),
        name: z.string().min(1).max(255),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .default(`#6366f1`),
      },
    },
    async (input) => {
      try {
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
        workspaceId: z.string().uuid(),
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
      description: `Delete a label from a workspace.`,
      inputSchema: {
        workspaceId: z.string().uuid(),
        labelId: z.string().uuid(),
      },
    },
    async (input) => {
      try {
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
      description: `Attach a label to an issue (workspaces must match).`,
      inputSchema: {
        issueId: z.string().uuid(),
        labelId: z.string().uuid(),
      },
    },
    async (input) => {
      try {
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
      description: `List comments on an issue (oldest first). The MCP user must have access to the issue's workspace.`,
      inputSchema: {
        issueId: z.string().uuid(),
        limit: z.number().int().min(1).max(200).default(100),
        offset: z.number().int().min(0).default(0),
      },
    },
    async ({ issueId, limit, offset }) => {
      try {
        const ctxIssue = await getIssueWorkspaceContext(issueId)
        await resolveWorkspaceAccess(user.id, ctxIssue.workspaceId)
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
      description: `Post a regular comment on an issue authored by the MCP user. Body is plain text. To ask the owner a question, call exponential_agent_plan_submit with state='awaiting_answer' instead — do NOT post questions as comments.`,
      inputSchema: {
        issueId: z.string().uuid(),
        bodyText: z.string().min(1).max(10_000),
      },
    },
    async ({ issueId, bodyText }) => {
      try {
        const result = await caller(user, request).comments.create({
          issueId,
          body: { text: bodyText },
        })
        return ok(result.comment)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_agent_plan_submit`,
    {
      title: `Submit an agent plan`,
      description: `Write the latest agent plan or open question onto an issue. Use state='awaiting_approval' with the full plan when it's ready for the owner to approve. Use state='awaiting_answer' and pass your questions in the 'question' field (markdown) when you need the owner to answer before continuing — do NOT post questions as comments. The revision counter bumps automatically; any prior approval is cleared. Caller must be an agent member of the issue's workspace.`,
      inputSchema: {
        issueId: z.string().uuid(),
        plan: z.string().max(50_000),
        state: z.enum([`awaiting_approval`, `awaiting_answer`]),
        question: z.string().max(50_000).optional(),
      },
    },
    async ({ issueId, plan, state, question }) => {
      try {
        const result = await caller(user, request).agentPlan.submitPlan({
          issueId,
          plan,
          state,
          question,
        })
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_agent_plan_mark_started`,
    {
      title: `Mark agent plan as started`,
      description: `Flip the issue's agent_plan_state to 'drafting' so the UI shows an "Agent has started" spinner the moment the pipeline engages. Only takes effect when the current state is NULL — never overrides awaiting_approval, approved, etc. Caller must be an agent member of the issue's workspace.`,
      inputSchema: {
        issueId: z.string().uuid(),
      },
    },
    async ({ issueId }) => {
      try {
        const result = await caller(user, request).agentPlan.markStarted({
          issueId,
        })
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_agent_plan_reset`,
    {
      title: `Reset agent plan state`,
      description: `Clear all agent-plan fields on an issue (state, revision, approval). Called by the daemon when an issue is hard-reset (typically after re-assignment). Caller must be an agent member of the issue's workspace.`,
      inputSchema: {
        issueId: z.string().uuid(),
      },
    },
    async ({ issueId }) => {
      try {
        const result = await caller(user, request).agentPlan.resetPlan({
          issueId,
        })
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_agent_open_pr`,
    {
      title: `Open the pull request for an issue`,
      description: `Open a GitHub PR for the branch the agent just pushed. The SERVER creates the PR using the agent owner's connected GitHub token (the agent no longer holds a GitHub API credential), then records pr_url/pr_number/pr_state + emits pr_opened. Pass the pushed head branch and the base branch (the repo's default). Caller must be an agent member of the issue's workspace.`,
      inputSchema: {
        issueId: z.string().uuid(),
        branch: z.string().max(255),
        base: z.string().max(255),
        title: z.string().max(255).optional(),
        body: z.string().max(4000).optional(),
      },
    },
    async ({ issueId, branch, base, title, body }) => {
      try {
        const result = await caller(user, request).agentPlan.openPr({
          issueId,
          branch,
          base,
          title,
          body,
        })
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_agent_report_pr`,
    {
      title: `Report the pull request for an issue`,
      description: `Record the agent's PR onto an issue (one issue = one PR). Sets the synced pr_url/pr_number/pr_state/branch columns and emits a pr_opened / pr_merged activity event on transition. Use state='open' when the PR is created, 'merged'/'closed' when it resolves. Caller must be an agent member of the issue's workspace.`,
      inputSchema: {
        issueId: z.string().uuid(),
        prUrl: z.string().url(),
        prNumber: z.number().int().positive(),
        prState: z.enum([`open`, `closed`, `merged`, `draft`]),
        branch: z.string().max(255).optional(),
        mergedAt: z.string().datetime().optional(),
      },
    },
    async ({ issueId, prUrl, prNumber, prState, branch, mergedAt }) => {
      try {
        const result = await caller(user, request).agentPlan.reportPr({
          issueId,
          prUrl,
          prNumber,
          prState,
          branch,
          mergedAt,
        })
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_agent_report_error`,
    {
      title: `Report an agent error`,
      description: `Record a terminal agent error on an issue. Emits an agent_error activity event so the UI can offer a Retry. Post the full error text as a comment separately. Caller must be an agent member of the issue's workspace.`,
      inputSchema: {
        issueId: z.string().uuid(),
        message: z.string().max(2000),
      },
    },
    async ({ issueId, message }) => {
      try {
        await caller(user, request).agentPlan.reportError({ issueId, message })
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )
}
