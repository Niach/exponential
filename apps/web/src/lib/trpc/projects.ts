import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { projects } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import {
  assertProjectMember,
  resolveWorkspaceAccess,
  assertWorkspaceOwner,
} from "@/lib/workspace-membership"
import { assertWithinPlanLimits } from "@/lib/billing"
import {
  platformValues,
  type ProjectPreviewMirror,
} from "@exp/db-schema/domain"

// The DB mirror is display-only metadata — it is never executed. Even so we
// clamp the strings the desktop syncs up (defence in depth) so a hostile or
// buggy client can't bloat the row or smuggle a giant payload through Electric.
const MAX_TARGET_ID = 128
const MAX_TARGET_NAME = 256
const MAX_TARGETS = 64

const previewMirrorInputSchema = z
  .object({
    targets: z
      .array(
        z.object({
          id: z.string().min(1).max(MAX_TARGET_ID),
          name: z.string().min(1).max(MAX_TARGET_NAME),
          platform: z.enum(platformValues),
        })
      )
      .max(MAX_TARGETS),
    feedbackProjectId: z.string().uuid().optional(),
  })
  .nullable()

function slugify(name: string): string {
  return name
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, ``)
    .replace(/[\s_]+/g, `-`)
    .replace(/-+/g, `-`)
    .replace(/^-|-$/g, ``)
}

export const projectsRouter = router({
  create: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        name: z.string().min(1).max(255),
        prefix: z.string().min(1).max(10),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`
      )
      await assertWithinPlanLimits(input.workspaceId, `projects`)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [project] = await tx
          .insert(projects)
          .values({
            workspaceId: input.workspaceId,
            name: input.name,
            slug: slugify(input.name),
            prefix: input.prefix.toUpperCase(),
            color: input.color ?? `#6366f1`,
          })
          .returning()

        return { project, txId }
      })
    }),

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
        archivedAt: z
          .string()
          .datetime()
          .transform((value) => new Date(value))
          .nullable()
          .optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input

      const projectRecord = await assertProjectMember(ctx.session.user.id, id)

      if (Object.hasOwn(updates, `archivedAt`)) {
        await assertWorkspaceOwner(
          ctx.session.user.id,
          projectRecord.workspaceId
        )
      }

      const [project] = await ctx.db
        .update(projects)
        .set(updates)
        .where(eq(projects.id, id))
        .returning()

      return { project }
    }),

  delete: authedProcedure
    .input(z.object({ projectId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      // Look up the project to get its workspaceId
      const [project] = await ctx.db
        .select({ workspaceId: projects.workspaceId })
        .from(projects)
        .where(eq(projects.id, input.projectId))
        .limit(1)

      if (!project) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Project not found` })
      }

      await assertWorkspaceOwner(ctx.session.user.id, project.workspaceId)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(projects).where(eq(projects.id, input.projectId))
        return { ok: true, txId }
      })
    }),

  // Writes ONLY the display mirror (`projects.preview_config`). The canonical
  // build/run shell commands live in the committed `.exponential/config.json`
  // and are read solely from the cloned working tree by the desktop apps — they
  // are never stored here and never executed from this synced value. We accept
  // the run-target metadata the desktop discovers (so the web settings UI shows
  // the targets the repo actually declares) plus the feedback-routing target.
  updatePreviewConfig: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        previewConfig: previewMirrorInputSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      const projectRecord = await assertProjectMember(
        ctx.session.user.id,
        input.projectId
      )
      await assertWorkspaceOwner(
        ctx.session.user.id,
        projectRecord.workspaceId
      )

      let mirror: ProjectPreviewMirror | null = input.previewConfig
      if (mirror) {
        // feedbackProjectId routes filed issues; it must point at a project in
        // the SAME workspace (server-checked — never trust the synced value).
        if (mirror.feedbackProjectId) {
          const [target] = await ctx.db
            .select({ id: projects.id })
            .from(projects)
            .where(
              and(
                eq(projects.id, mirror.feedbackProjectId),
                eq(projects.workspaceId, projectRecord.workspaceId)
              )
            )
            .limit(1)
          if (!target) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `feedbackProjectId must be a project in this workspace`,
            })
          }
        }
        mirror = {
          targets: mirror.targets,
          ...(mirror.feedbackProjectId
            ? { feedbackProjectId: mirror.feedbackProjectId }
            : {}),
        }
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [project] = await tx
          .update(projects)
          .set({ previewConfig: mirror })
          .where(eq(projects.id, input.projectId))
          .returning()
        return { project, txId }
      })
    }),
})
