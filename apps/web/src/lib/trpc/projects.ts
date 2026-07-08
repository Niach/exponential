import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { projects, repositories } from "@/db/schema"
import { and, eq, isNull } from "drizzle-orm"
import {
  projectTypeSchema,
  publicCodingVisibilitySchema,
} from "@exp/db-schema/domain"
import {
  assertProjectMember,
  resolveWorkspaceAccess,
  assertWorkspaceOwner,
  invalidatePublicProjectCache,
} from "@/lib/workspace-membership"
import type { db } from "@/db/connection"
import {
  assertCanManageRepos,
  connectRepositoryInTx,
} from "@/lib/trpc/repositories"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

// A `dev` project is backed by exactly one repo; `tasks`/`feedback` projects
// may have one (the dogfood feedback board does) but don't need one (v7 —
// this is what unlocks project creation on instances without a GitHub App).
// Create either points at an existing registry repo (`repositoryId`) or
// connects one inline (`fullName`, same validation as repositories.add).
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
        type: projectTypeSchema.default(`dev`),
        // Anonymous-visitor visibility toggles (feedback boards only; inert on
        // other types).
        publicShowComments: z.boolean().optional(),
        publicShowActivity: z.boolean().optional(),
        publicShowCoding: publicCodingVisibilitySchema.optional(),
        // Required for `dev` projects, optional otherwise. Either target an
        // existing registry repo or connect one inline in the same transaction
        // (onboarding/create dialogs stay a single call).
        repository: repositoryInputSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`
      )
      if (input.type === `dev` && !input.repository) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Dev projects require a repository`,
        })
      }
      const repositoryInput = input.repository
      const inlineConnect =
        repositoryInput != null && `fullName` in repositoryInput
      if (inlineConnect) {
        // The inline connect path needs owner/admin (repo management), beyond
        // the member-level project create. Projects & repos are unlimited on
        // every tier now (v5 per-seat model), so there is no plan cap here.
        await assertCanManageRepos(ctx.session.user.id, input.workspaceId)
      }
      if (input.type === `feedback`) {
        // Flipping content public is a privacy-significant act — owner-only,
        // mirroring the update path. (Free on every tier — no plan gate.)
        await assertWorkspaceOwner(ctx.session.user.id, input.workspaceId)
      }

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        const repositoryId = !repositoryInput
          ? null
          : `fullName` in repositoryInput
            ? await connectRepositoryInTx(tx, {
                userId: ctx.session.user.id,
                workspaceId: input.workspaceId,
                fullName: repositoryInput.fullName,
                defaultBranch: repositoryInput.defaultBranch,
                private: repositoryInput.private,
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
            type: input.type,
            publicShowComments: input.publicShowComments ?? true,
            publicShowActivity: input.publicShowActivity ?? false,
            publicShowCoding: input.publicShowCoding ?? `off`,
            repositoryId,
          })
          .returning()

        return { project, txId }
      })

      if (input.type === `feedback`) invalidatePublicProjectCache()
      return result
    }),

  // Owner/admin: retarget a project's backing repo — or detach it entirely
  // (repositoryId: null) from a non-dev project. Existing worktrees for
  // old-repo issues keep working locally (they're just git); new launches use
  // the new repo.
  setRepository: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        repositoryId: z.string().uuid().nullable(),
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
        if (input.repositoryId === null) {
          const [current] = await tx
            .select({ type: projects.type })
            .from(projects)
            .where(eq(projects.id, input.projectId))
            .limit(1)
          if (current?.type === `dev`) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `Dev projects require a repository — switch the project type first`,
            })
          }
        }
        const repositoryId =
          input.repositoryId === null
            ? null
            : await assertRepositoryInWorkspace(
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
        type: projectTypeSchema.optional(),
        publicShowComments: z.boolean().optional(),
        publicShowActivity: z.boolean().optional(),
        publicShowCoding: publicCodingVisibilitySchema.optional(),
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

      // Archiving, type changes and public-visibility toggles are
      // privacy/structure-significant — workspace-owner-only. Name/color stay
      // member-editable.
      const ownerGated =
        Object.hasOwn(updates, `archivedAt`) ||
        updates.type !== undefined ||
        updates.publicShowComments !== undefined ||
        updates.publicShowActivity !== undefined ||
        updates.publicShowCoding !== undefined
      if (ownerGated) {
        await assertWorkspaceOwner(
          ctx.session.user.id,
          projectRecord.workspaceId
        )
      }

      if (updates.type === `dev`) {
        const [current] = await ctx.db
          .select({ repositoryId: projects.repositoryId })
          .from(projects)
          .where(eq(projects.id, id))
          .limit(1)
        if (!current?.repositoryId) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Connect a repository before switching to a dev project`,
          })
        }
      }

      const [project] = await ctx.db
        .update(projects)
        .set(updates)
        .where(eq(projects.id, id))
        .returning()

      // Type/toggle/archive changes can alter the instance's public surface.
      if (ownerGated) invalidatePublicProjectCache()

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

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(projects).where(eq(projects.id, input.projectId))
        return { ok: true, txId }
      })
      invalidatePublicProjectCache()
      return result
    }),
})
