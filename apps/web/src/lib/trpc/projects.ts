import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { projects } from "@/db/schema"
import { eq } from "drizzle-orm"
import {
  assertProjectMember,
  assertCanMutateWorkspaceResources,
  assertWorkspaceOwner,
} from "@/lib/workspace-membership"
import { assertWithinPlanLimits } from "@/lib/billing"

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
        // Optional "owner/repo" to link at creation. The agent-first path picks
        // a repo up front; trusting the string here (no GitHub round-trip)
        // keeps create fast — the agent run degrades gracefully if unresolvable.
        repo: z
          .string()
          .regex(/^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/, `Expected "owner/repo"`)
          .optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertCanMutateWorkspaceResources(
        ctx.session.user.id,
        input.workspaceId
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
            githubRepo: input.repo ?? null,
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

  linkGithubRepo: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        repo: z
          .string()
          .regex(
            /^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/,
            `Expected "owner/repo"`
          ),
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
      const [project] = await ctx.db
        .update(projects)
        .set({ githubRepo: input.repo })
        .where(eq(projects.id, input.projectId))
        .returning()
      return { project }
    }),

  unlinkGithubRepo: authedProcedure
    .input(z.object({ projectId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const projectRecord = await assertProjectMember(
        ctx.session.user.id,
        input.projectId
      )
      await assertWorkspaceOwner(
        ctx.session.user.id,
        projectRecord.workspaceId
      )
      const [project] = await ctx.db
        .update(projects)
        .set({ githubRepo: null })
        .where(eq(projects.id, input.projectId))
        .returning()
      return { project }
    }),
})
