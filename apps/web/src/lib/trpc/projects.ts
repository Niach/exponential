import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { projects, repositories } from "@/db/schema"
import { and, eq, isNull } from "drizzle-orm"
import {
  assertProjectMember,
  resolveWorkspaceAccess,
  assertWorkspaceOwner,
} from "@/lib/workspace-membership"
import type { db } from "@/db/connection"
import {
  assertCanManageRepos,
  connectRepositoryInTx,
} from "@/lib/trpc/repositories"
import type { ProjectPreviewMirror } from "@exp/db-schema/domain"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

// Every project is backed by exactly one repo (v4). Create either points at an
// existing registry repo (`repositoryId`) or connects one inline (`fullName`,
// same validation as repositories.add).
const fullNameSchema = z
  .string()
  .min(1)
  .max(255)
  .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`)

const repositoryInputSchema = z.union([
  z.object({ repositoryId: z.string().uuid() }),
  z.object({
    fullName: fullNameSchema,
    defaultBranch: z.string().min(1).max(255).optional(),
    private: z.boolean().optional(),
    installationId: z.number().int().optional(),
  }),
])

// Validate that a repo id belongs to the workspace and isn't archived.
async function assertRepositoryInWorkspace(
  tx: Tx,
  repositoryId: string,
  workspaceId: string
): Promise<string> {
  const [repo] = await tx
    .select({ id: repositories.id })
    .from(repositories)
    .where(
      and(
        eq(repositories.id, repositoryId),
        eq(repositories.workspaceId, workspaceId),
        isNull(repositories.archivedAt)
      )
    )
    .limit(1)
  if (!repo) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Repository not found in this workspace`,
    })
  }
  return repo.id
}

// The DB mirror is display-only metadata — it is never executed. It now holds a
// single field: the feedback routing target. Run configs (build/run commands)
// live in the `run_configs` table and are edited IDE-side only (L23).
const previewMirrorInputSchema = z
  .object({
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
        // v4: a project is a repo. Either target an existing registry repo or
        // connect one inline in the same transaction (onboarding/create dialogs
        // stay a single call).
        repository: repositoryInputSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`
      )
      const inlineConnect = `fullName` in input.repository
      if (inlineConnect) {
        // The inline connect path needs owner/admin (repo management), beyond
        // the member-level project create. Projects & repos are unlimited on
        // every tier now (v5 per-seat model), so there is no plan cap here.
        await assertCanManageRepos(ctx.session.user.id, input.workspaceId)
      }

      const repositoryInput = input.repository
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        const repositoryId =
          `fullName` in repositoryInput
            ? await connectRepositoryInTx(tx, {
                workspaceId: input.workspaceId,
                fullName: repositoryInput.fullName,
                defaultBranch: repositoryInput.defaultBranch,
                private: repositoryInput.private,
                installationId: repositoryInput.installationId,
              })
            : await assertRepositoryInWorkspace(
                tx,
                repositoryInput.repositoryId,
                input.workspaceId
              )

        const [project] = await tx
          .insert(projects)
          .values({
            workspaceId: input.workspaceId,
            name: input.name,
            slug: slugify(input.name),
            prefix: input.prefix.toUpperCase(),
            color: input.color ?? `#6366f1`,
            repositoryId,
          })
          .returning()

        return { project, txId }
      })
    }),

  // Owner/admin: retarget a project's backing repo. Existing worktrees for
  // old-repo issues keep working locally (they're just git); new launches use
  // the new repo.
  setRepository: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        repositoryId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const projectRecord = await assertProjectMember(
        ctx.session.user.id,
        input.projectId
      )
      await assertCanManageRepos(
        ctx.session.user.id,
        projectRecord.workspaceId
      )

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const repositoryId = await assertRepositoryInWorkspace(
          tx,
          input.repositoryId,
          projectRecord.workspaceId
        )
        const [project] = await tx
          .update(projects)
          .set({ repositoryId })
          .where(eq(projects.id, input.projectId))
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

  // Writes ONLY the display mirror (`projects.preview_config`) — now just the
  // feedback-routing target. Build/run commands live in the `run_configs` table
  // and are edited IDE-side only (L23); nothing here is ever executed.
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
        mirror = mirror.feedbackProjectId
          ? { feedbackProjectId: mirror.feedbackProjectId }
          : {}
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
