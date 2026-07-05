import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { count, eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import { projects, users, widgetConfigs, widgetSubmissions } from "@/db/schema"
import {
  getProjectWorkspaceId,
  resolveWorkspaceAccess,
} from "@/lib/workspace-membership"
import { generateWidgetKey } from "@/lib/widget/key"
import { createWidgetUser, widgetUserName } from "@/lib/widget/widget-user"
import { assertCanCreateWidget } from "@/lib/billing"

const widgetNameSchema = z.string().trim().min(1).max(255)
// Hostname[:port] patterns, optionally `*.`-prefixed. Kept permissive on
// purpose (the matcher in lib/widget/origin.ts is the source of truth) —
// this only rejects obvious junk like full URLs or whitespace.
const domainPatternSchema = z
  .string()
  .trim()
  .min(1)
  .max(253)
  .regex(/^(\*\.)?[a-zA-Z0-9.-]+(:\d{1,5})?$/, `Enter a hostname like app.example.com`)
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
  })
  .optional()

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
  await resolveWorkspaceAccess(userId, config.workspaceId, `mutate_resources`)
  return config
}

export const widgetsRouter = router({
  list: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`
      )
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
        .innerJoin(projects, eq(widgetConfigs.projectId, projects.id))
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
        projectId: z.string().uuid(),
        name: widgetNameSchema,
        allowedDomains: allowedDomainsSchema.default([]),
        formConfig: formConfigSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`
      )
      // Feedback widget is a Pro+ feature, capped per tier (§3.3(4)). The
      // bootstrap dogfood config is inserted directly and is exempt.
      await assertCanCreateWidget(input.workspaceId)
      const project = await getProjectWorkspaceId(input.projectId)
      if (project.workspaceId !== input.workspaceId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Project must belong to the workspace`,
        })
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
            projectId: input.projectId,
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
        projectId: z.string().uuid().optional(),
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

      if (input.projectId && input.projectId !== config.projectId) {
        const project = await getProjectWorkspaceId(input.projectId)
        if (project.workspaceId !== config.workspaceId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Project must belong to the workspace`,
          })
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
