import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { count, eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import {
  projects,
  supportThreads,
  users,
  widgetConfigs,
  widgetSubmissions,
  workspaces,
} from "@/db/schema"
import {
  assertWorkspaceMember,
  assertWorkspaceOwner,
  getIssueWorkspaceContext,
  getProjectWorkspaceId,
} from "@/lib/workspace-membership"
import { generateWidgetKey } from "@/lib/widget/key"
import { createWidgetUser, widgetUserName } from "@/lib/widget/widget-user"
import { assertCanCreateWidget, assertCanUseHelpdesk } from "@/lib/billing"

const widgetNameSchema = z.string().trim().min(1).max(255)
// Hostname[:port] patterns, optionally `*.`-prefixed. Kept permissive on
// purpose (the matcher in lib/widget/origin.ts is the source of truth) —
// this only rejects obvious junk like full URLs or whitespace.
const domainPatternSchema = z
  .string()
  .trim()
  .min(1)
  .max(253)
  .regex(
    /^(\*\.)?[a-zA-Z0-9.-]+(:\d{1,5})?$/,
    `Enter a hostname like app.example.com`
  )
const allowedDomainsSchema = z.array(domainPatternSchema).max(20)

const formConfigSchema = z
  .object({
    buttonLabel: z.string().trim().max(40).optional(),
    accentColor: z
      .string()
      .regex(/^#[0-9a-fA-F]{6}$/)
      .optional(),
    position: z.enum([`bottom-right`, `bottom-left`]).optional(),
    emailRequired: z.boolean().optional(),
    // Which entry points the panel offers (EXP-130); absent = feedback-only.
    modes: z
      .array(z.enum([`feedback`, `support`]))
      .min(1)
      .max(2)
      .optional(),
  })
  .optional()

// Absent modes = feedback-only (every pre-modes config).
function modesOf(formConfig: { modes?: string[] } | null | undefined): string[] {
  const raw = formConfig?.modes
  return Array.isArray(raw) && raw.length > 0 ? raw : [`feedback`]
}

// Support mode files helpdesk tickets into the workspace support inbox, so it
// needs both the plan gate and the workspace helpdesk switch — otherwise
// tickets would land invisibly (the Support inbox nav keys off
// workspaces.helpdesk_enabled).
async function assertSupportModeUsable(workspaceId: string) {
  await assertCanUseHelpdesk(workspaceId)
  const [workspace] = await db
    .select({ helpdeskEnabled: workspaces.helpdeskEnabled })
    .from(workspaces)
    .where(eq(workspaces.id, workspaceId))
    .limit(1)
  if (workspace?.helpdeskEnabled !== true) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Enable the helpdesk in the widget settings first`,
    })
  }
}

async function loadConfigForWorkspaceAdmin(
  userId: string,
  widgetConfigId: string
) {
  const [config] = await db
    .select()
    .from(widgetConfigs)
    .where(eq(widgetConfigs.id, widgetConfigId))
    .limit(1)
  if (!config) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Widget not found` })
  }
  // Owner-only: tightens both update and delete (their only callers) in one
  // place. The widget-settings surface is owner-gated on every client.
  await assertWorkspaceOwner(userId, config.workspaceId)
  return config
}

export const widgetsRouter = router({
  // EXP-42b: the reporter/page/env metadata stripped from widget-issue
  // descriptions lives only in widget_submissions — this read powers the
  // members-only "Reported via widget" card on the issue detail view.
  // MEMBER-gated on purpose (every member triages widget issues), unlike the
  // rest of this router, which stays owner-only.
  submissionForIssue: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const issueContext = await getIssueWorkspaceContext(input.issueId)
      await assertWorkspaceMember(ctx.session.user.id, issueContext.workspaceId)
      const [submission] = await ctx.db
        .select()
        .from(widgetSubmissions)
        .where(eq(widgetSubmissions.issueId, input.issueId))
        .limit(1)
      return submission ?? null
    }),

  // Same card for the support inbox details rail: the page/env context of a
  // widget-filed ticket. MEMBER-gated like submissionForIssue (every member
  // handles support).
  submissionForThread: authedProcedure
    .input(z.object({ threadId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const [thread] = await ctx.db
        .select({ workspaceId: supportThreads.workspaceId })
        .from(supportThreads)
        .where(eq(supportThreads.id, input.threadId))
        .limit(1)
      if (!thread) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Thread not found` })
      }
      await assertWorkspaceMember(ctx.session.user.id, thread.workspaceId)
      const [submission] = await ctx.db
        .select()
        .from(widgetSubmissions)
        .where(eq(widgetSubmissions.supportThreadId, input.threadId))
        .limit(1)
      return submission ?? null
    }),

  list: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      // Owner-only: exposes publicKey + submission counts, consumed only by the
      // owner-gated widget settings section.
      await assertWorkspaceOwner(ctx.session.user.id, input.workspaceId)
      return await ctx.db
        .select({
          id: widgetConfigs.id,
          name: widgetConfigs.name,
          publicKey: widgetConfigs.publicKey,
          projectId: widgetConfigs.projectId,
          projectName: projects.name,
          allowedDomains: widgetConfigs.allowedDomains,
          enabled: widgetConfigs.enabled,
          formConfig: widgetConfigs.formConfig,
          createdAt: widgetConfigs.createdAt,
          submissionCount: count(widgetSubmissions.id),
        })
        .from(widgetConfigs)
        // Left join: a support-only widget has no feedback board.
        .leftJoin(projects, eq(widgetConfigs.projectId, projects.id))
        .leftJoin(
          widgetSubmissions,
          eq(widgetSubmissions.widgetConfigId, widgetConfigs.id)
        )
        .where(eq(widgetConfigs.workspaceId, input.workspaceId))
        .groupBy(widgetConfigs.id, projects.name)
        .orderBy(widgetConfigs.createdAt)
    }),

  create: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        // The feedback target board. Required iff the widget offers feedback
        // mode; a support-only widget has none (tickets go to the workspace
        // support inbox).
        projectId: z.string().uuid().nullable().optional(),
        name: widgetNameSchema,
        allowedDomains: allowedDomainsSchema.default([]),
        formConfig: formConfigSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      // Owner-only: creating a public write path is privacy-significant.
      await assertWorkspaceOwner(ctx.session.user.id, input.workspaceId)
      // Widget count is capped per tier (1 on Free). The bootstrap dogfood
      // config is inserted directly and is exempt.
      await assertCanCreateWidget(input.workspaceId)

      const modes = modesOf(input.formConfig)
      const projectId = input.projectId ?? null
      if (modes.includes(`feedback`) && projectId == null) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Pick a board for feedback submissions`,
        })
      }
      if (projectId != null) {
        const project = await getProjectWorkspaceId(projectId)
        if (project.workspaceId !== input.workspaceId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Board must belong to the workspace`,
          })
        }
      }
      if (modes.includes(`support`)) {
        await assertSupportModeUsable(input.workspaceId)
      }

      return await ctx.db.transaction(async (tx) => {
        const widgetUserId = await createWidgetUser(tx, {
          workspaceId: input.workspaceId,
          configName: input.name,
        })
        const [config] = await tx
          .insert(widgetConfigs)
          .values({
            workspaceId: input.workspaceId,
            projectId,
            name: input.name,
            publicKey: generateWidgetKey(),
            allowedDomains: input.allowedDomains,
            formConfig: input.formConfig ?? null,
            widgetUserId,
            createdByUserId: ctx.session.user.id,
          })
          .returning()
        return config
      })
    }),

  update: authedProcedure
    .input(
      z.object({
        widgetConfigId: z.string().uuid(),
        name: widgetNameSchema.optional(),
        // Tri-state: undefined = unchanged, null = clear (support-only
        // widget), uuid = feedback lands on that board.
        projectId: z.string().uuid().nullable().optional(),
        allowedDomains: allowedDomainsSchema.optional(),
        enabled: z.boolean().optional(),
        formConfig: formConfigSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      const config = await loadConfigForWorkspaceAdmin(
        ctx.session.user.id,
        input.widgetConfigId
      )

      if (input.projectId != null && input.projectId !== config.projectId) {
        const project = await getProjectWorkspaceId(input.projectId)
        if (project.workspaceId !== config.workspaceId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Board must belong to the workspace`,
          })
        }
      }

      // Validate the FINAL state, but only when this update actually touches
      // it — a lapsed plan must not block unrelated edits like renaming or
      // disabling, and stale stored support degrades gracefully at serve
      // time via effectiveWidgetModes.
      const touchesModeState =
        input.formConfig !== undefined || input.projectId !== undefined
      const finalModes =
        input.formConfig !== undefined
          ? modesOf(input.formConfig)
          : modesOf(config.formConfig as { modes?: string[] } | null)
      const finalProjectId =
        input.projectId !== undefined ? input.projectId : config.projectId
      if (touchesModeState) {
        if (finalModes.includes(`feedback`) && finalProjectId == null) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Pick a board for feedback submissions`,
          })
        }
        if (finalModes.includes(`support`)) {
          await assertSupportModeUsable(config.workspaceId)
        }
      }

      await ctx.db.transaction(async (tx) => {
        await tx
          .update(widgetConfigs)
          .set({
            ...(input.name !== undefined ? { name: input.name } : {}),
            ...(input.projectId !== undefined
              ? { projectId: input.projectId }
              : {}),
            ...(input.allowedDomains !== undefined
              ? { allowedDomains: input.allowedDomains }
              : {}),
            ...(input.enabled !== undefined ? { enabled: input.enabled } : {}),
            ...(input.formConfig !== undefined
              ? { formConfig: input.formConfig ?? null }
              : {}),
          })
          .where(eq(widgetConfigs.id, config.id))

        // Keep the synthetic creator's display name in sync so issue
        // timelines keep saying "Widget: <current name>".
        if (input.name !== undefined && input.name !== config.name) {
          await tx
            .update(users)
            .set({ name: widgetUserName(input.name) })
            .where(eq(users.id, config.widgetUserId))
        }
      })
      return { ok: true }
    }),

  // Deletes the config row ONLY. The synthetic widget user (and its agent
  // membership) is intentionally retained: issues.creator_id cascades on
  // user delete, so removing the user would delete every issue this widget
  // ever created. widget_configs.widget_user_id is `restrict` for the same
  // reason. widget_submissions rows survive via their `set null` FK.
  delete: authedProcedure
    .input(z.object({ widgetConfigId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const config = await loadConfigForWorkspaceAdmin(
        ctx.session.user.id,
        input.widgetConfigId
      )
      await ctx.db.delete(widgetConfigs).where(eq(widgetConfigs.id, config.id))
      return { ok: true }
    }),
})
