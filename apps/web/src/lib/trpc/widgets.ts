import { z } from "zod"
import { hexColorSchema } from "@exp/db-schema/domain"
import { TRPCError } from "@trpc/server"
import { and, count, eq, inArray } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import {
  boards,
  labels,
  supportThreads,
  widgetConfigs,
  widgetSubmissions,
  teams,
} from "@/db/schema"
import {
  assertTeamMember,
  assertTeamOwner,
  getIssueTeamContext,
  getBoardTeamId,
} from "@/lib/team-membership"
import { PICKABLE_ICONS } from "@exp/icons"
import { generateWidgetKey } from "@/lib/widget/key"
import {
  maxWidgetCustomFields,
  maxWidgetLabels,
  widgetCustomFieldKeyPattern,
  widgetLauncherModes,
  widgetLauncherPositions,
} from "@/lib/widget/service"
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
// Non-empty: an unconfigured allowlist blocks the key at serve time
// (EXP-209 removed allow-all), so every config must name its domains.
const allowedDomainsSchema = z.array(domainPatternSchema).min(1).max(20)

// One device's launcher placement (EXP-569).
const launcherPlacementSchema = z.object({
  mode: z.enum(widgetLauncherModes),
  position: z.enum(widgetLauncherPositions),
})

const formConfigSchema = z
  .object({
    buttonLabel: z.string().trim().max(40).optional(),
    accentColor: hexColorSchema.optional(),
    // Per-device launcher appearance (EXP-569). Absent devices fall back to
    // the serve-time defaults.
    launcher: z
      .object({
        desktop: launcherPlacementSchema.optional(),
        mobile: launcherPlacementSchema.optional(),
        icon: z.enum(PICKABLE_ICONS).optional(),
      })
      .optional(),
    emailRequired: z.boolean().optional(),
    // EXP-244 field toggles: collectEmail defaults true (absent = legacy
    // behavior), collectName defaults false. The config route normalizes the
    // required-implies-collect contradictions on read.
    collectEmail: z.boolean().optional(),
    collectName: z.boolean().optional(),
    nameRequired: z.boolean().optional(),
    // Owner-defined feedback-form inputs (EXP-244); values land in the
    // submission's customData blob under `key`.
    customFields: z
      .array(
        z.object({
          key: z.string().regex(widgetCustomFieldKeyPattern),
          label: z.string().trim().min(1).max(40),
          required: z.boolean().optional(),
        })
      )
      .max(maxWidgetCustomFields)
      .refine(
        (fields) => new Set(fields.map((f) => f.key)).size === fields.length,
        { message: `Custom field keys must be unique` }
      )
      .optional(),
    // Which entry points the panel offers (EXP-130); absent = feedback-only.
    modes: z
      .array(z.enum([`feedback`, `support`]))
      .min(1)
      .max(2)
      .optional(),
    // Team labels visitors can tag their report with (EXP-435). Stored as
    // ids; the config route resolves them to {id,name,color} and silently
    // drops labels deleted since this write.
    labelIds: z
      .array(z.string().uuid())
      .max(maxWidgetLabels)
      .refine((ids) => new Set(ids).size === ids.length, {
        message: `Label ids must be unique`,
      })
      .optional(),
    // Panel theme (EXP-435); absent = dark (every pre-theme config).
    // The EXP-435 backgroundColor/textColor overrides were removed by
    // EXP-569 (one accent color + the theme presets are the whole palette
    // surface); a data migration stripped them from stored rows.
    theme: z.enum([`dark`, `light`, `auto`]).optional(),
  })
  .optional()

// Absent modes = feedback-only (every pre-modes config).
function modesOf(formConfig: { modes?: string[] } | null | undefined): string[] {
  const raw = formConfig?.modes
  return Array.isArray(raw) && raw.length > 0 ? raw : [`feedback`]
}

// Support mode files helpdesk tickets into the team support inbox, so it
// needs both the plan gate and the team helpdesk switch — otherwise
// tickets would land invisibly (the Support inbox nav keys off
// teams.helpdesk_enabled).
async function assertSupportModeUsable(teamId: string) {
  await assertCanUseHelpdesk(teamId)
  const [team] = await db
    .select({ helpdeskEnabled: teams.helpdeskEnabled })
    .from(teams)
    .where(eq(teams.id, teamId))
    .limit(1)
  if (team?.helpdeskEnabled !== true) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Enable the helpdesk in the widget settings first`,
    })
  }
}

// Write-time gate for form_config.labelIds: every id must be a live label of
// this team (mirrors issues.create). Read paths stay tolerant of ids that go
// stale later.
async function assertLabelsBelongToTeam(
  teamId: string,
  formConfig: { labelIds?: string[] } | null | undefined
) {
  const labelIds = formConfig?.labelIds
  if (!labelIds || labelIds.length === 0) return
  const rows = await db
    .select({ id: labels.id })
    .from(labels)
    .where(and(inArray(labels.id, labelIds), eq(labels.teamId, teamId)))
  if (rows.length !== labelIds.length) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Labels must belong to the team`,
    })
  }
}

async function loadConfigForTeamAdmin(
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
  await assertTeamOwner(userId, config.teamId)
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
      const issueContext = await getIssueTeamContext(input.issueId)
      await assertTeamMember(ctx.session.user.id, issueContext.teamId)
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
        .select({ teamId: supportThreads.teamId })
        .from(supportThreads)
        .where(eq(supportThreads.id, input.threadId))
        .limit(1)
      if (!thread) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Thread not found` })
      }
      await assertTeamMember(ctx.session.user.id, thread.teamId)
      const [submission] = await ctx.db
        .select()
        .from(widgetSubmissions)
        .where(eq(widgetSubmissions.supportThreadId, input.threadId))
        .limit(1)
      return submission ?? null
    }),

  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      // Owner-only: exposes publicKey + submission counts, consumed only by the
      // owner-gated widget settings section.
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      return await ctx.db
        .select({
          id: widgetConfigs.id,
          name: widgetConfigs.name,
          publicKey: widgetConfigs.publicKey,
          boardId: widgetConfigs.boardId,
          boardName: boards.name,
          allowedDomains: widgetConfigs.allowedDomains,
          enabled: widgetConfigs.enabled,
          formConfig: widgetConfigs.formConfig,
          createdAt: widgetConfigs.createdAt,
          submissionCount: count(widgetSubmissions.id),
        })
        .from(widgetConfigs)
        // Left join: a support-only widget has no feedback board.
        .leftJoin(boards, eq(widgetConfigs.boardId, boards.id))
        .leftJoin(
          widgetSubmissions,
          eq(widgetSubmissions.widgetConfigId, widgetConfigs.id)
        )
        .where(eq(widgetConfigs.teamId, input.teamId))
        .groupBy(widgetConfigs.id, boards.name)
        .orderBy(widgetConfigs.createdAt)
    }),

  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        // The feedback target board. Required iff the widget offers feedback
        // mode; a support-only widget has none (tickets go to the team
        // support inbox).
        boardId: z.string().uuid().nullable().optional(),
        name: widgetNameSchema,
        allowedDomains: allowedDomainsSchema,
        formConfig: formConfigSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      // Owner-only: creating a public write path is privacy-significant.
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      // Widget count is capped per tier (1 on Free).
      await assertCanCreateWidget(input.teamId)

      const modes = modesOf(input.formConfig)
      const boardId = input.boardId ?? null
      if (modes.includes(`feedback`) && boardId == null) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Pick a board for feedback submissions`,
        })
      }
      if (boardId != null) {
        const board = await getBoardTeamId(boardId)
        if (board.teamId !== input.teamId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Board must belong to the team`,
          })
        }
      }
      if (modes.includes(`support`)) {
        await assertSupportModeUsable(input.teamId)
      }
      await assertLabelsBelongToTeam(input.teamId, input.formConfig)

      const [config] = await ctx.db
        .insert(widgetConfigs)
        .values({
          teamId: input.teamId,
          boardId,
          name: input.name,
          publicKey: generateWidgetKey(),
          allowedDomains: input.allowedDomains,
          formConfig: input.formConfig ?? null,
          createdByUserId: ctx.session.user.id,
        })
        .returning()
      return config
    }),

  update: authedProcedure
    .input(
      z.object({
        widgetConfigId: z.string().uuid(),
        name: widgetNameSchema.optional(),
        // Tri-state: undefined = unchanged, null = clear (support-only
        // widget), uuid = feedback lands on that board.
        boardId: z.string().uuid().nullable().optional(),
        allowedDomains: allowedDomainsSchema.optional(),
        enabled: z.boolean().optional(),
        formConfig: formConfigSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      const config = await loadConfigForTeamAdmin(
        ctx.session.user.id,
        input.widgetConfigId
      )

      if (input.boardId != null && input.boardId !== config.boardId) {
        const board = await getBoardTeamId(input.boardId)
        if (board.teamId !== config.teamId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Board must belong to the team`,
          })
        }
      }

      // Validate the FINAL state, but only when this update actually touches
      // it — a lapsed plan must not block unrelated edits like renaming or
      // disabling, and stale stored support degrades gracefully at serve
      // time via effectiveWidgetModes.
      const touchesModeState =
        input.formConfig !== undefined || input.boardId !== undefined
      const finalModes =
        input.formConfig !== undefined
          ? modesOf(input.formConfig)
          : modesOf(config.formConfig as { modes?: string[] } | null)
      const finalBoardId =
        input.boardId !== undefined ? input.boardId : config.boardId
      if (touchesModeState) {
        if (finalModes.includes(`feedback`) && finalBoardId == null) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Pick a board for feedback submissions`,
          })
        }
        if (finalModes.includes(`support`)) {
          await assertSupportModeUsable(config.teamId)
        }
      }
      if (input.formConfig !== undefined) {
        await assertLabelsBelongToTeam(config.teamId, input.formConfig)
      }

      await ctx.db
        .update(widgetConfigs)
        .set({
          ...(input.name !== undefined ? { name: input.name } : {}),
          ...(input.boardId !== undefined ? { boardId: input.boardId } : {}),
          ...(input.allowedDomains !== undefined
            ? { allowedDomains: input.allowedDomains }
            : {}),
          ...(input.enabled !== undefined ? { enabled: input.enabled } : {}),
          ...(input.formConfig !== undefined
            ? { formConfig: input.formConfig ?? null }
            : {}),
        })
        .where(eq(widgetConfigs.id, config.id))
      return { ok: true }
    }),

  // Deletes the config row ONLY. Widget-filed issues carry a null creator +
  // source `widget` (no synthetic user to clean up). widget_submissions rows
  // survive via their `set null` FK.
  delete: authedProcedure
    .input(z.object({ widgetConfigId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const config = await loadConfigForTeamAdmin(
        ctx.session.user.id,
        input.widgetConfigId
      )
      await ctx.db.delete(widgetConfigs).where(eq(widgetConfigs.id, config.id))
      return { ok: true }
    }),
})
