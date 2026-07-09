import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { projects, repositories } from "@/db/schema"
import { and, eq, isNotNull, isNull } from "drizzle-orm"
import {
  projectTypeSchema,
  publicCodingVisibilitySchema,
  PROJECT_TRASH_RETENTION_MS,
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

// Postgres unique_violation (23505), as surfaced by postgres-js directly or
// wrapped in an error cause by drizzle.
function isUniqueViolation(err: unknown): boolean {
  if (!err || typeof err !== `object`) return false
  const candidate = err as { code?: unknown; cause?: unknown }
  if (candidate.code === `23505`) return true
  return isUniqueViolation(candidate.cause)
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
        // Identifiers are minted as `{PREFIX}-{number}` and the cross-client
        // issue-ref token contract (lib/issue-refs.ts) only matches
        // letter-led alphanumeric prefixes — reject whitespace/symbol
        // prefixes at the door so a project can never mint unreferenceable
        // identifiers (EXP-46 hardening; stored uppercased below).
        prefix: z
          .string()
          .trim()
          .regex(
            /^[A-Za-z][A-Za-z0-9]{0,9}$/,
            `Prefix must be 1-10 letters or digits, starting with a letter`
          ),
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

      // Symbol/emoji-only names slugify to `` — fall back to the (alphanumeric)
      // prefix, then a generic root, mirroring workspaces' uniqueSlug fallback,
      // so a project can never insert the unroutable slug '' (EXP-46).
      const slug =
        slugify(input.name) || slugify(input.prefix) || `project`
      let result
      try {
        result = await ctx.db.transaction(async (tx) => {
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
              slug,
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
      } catch (error) {
        // A trashed project keeps its (workspace_id, slug) reservation for the
        // whole retention window; distinguish that from a live-name clash.
        if (isUniqueViolation(error)) {
          const [trashed] = await ctx.db
            .select({ id: projects.id })
            .from(projects)
            .where(
              and(
                eq(projects.workspaceId, input.workspaceId),
                eq(projects.slug, slug),
                isNotNull(projects.deletedAt)
              )
            )
            .limit(1)
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: trashed
              ? `A project with this name is in the trash — restore it or wait for the purge`
              : `A project with this name already exists`,
          })
        }
        throw error
      }

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

      // Protected projects (the dogfood board) keep their repo — mirrors the
      // delete/archive/retype guards.
      const [current] = await ctx.db
        .select({ isProtected: projects.isProtected })
        .from(projects)
        .where(eq(projects.id, input.projectId))
        .limit(1)
      if (current?.isProtected) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This project is protected — its repository cannot be changed`,
        })
      }

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

      // Protected projects (the dogfood board) can't be archived or retyped;
      // name/color stay editable.
      const attemptsArchiveOrRetype =
        (Object.hasOwn(updates, `archivedAt`) && updates.archivedAt != null) ||
        updates.type !== undefined
      if (attemptsArchiveOrRetype) {
        const [current] = await ctx.db
          .select({ isProtected: projects.isProtected })
          .from(projects)
          .where(eq(projects.id, id))
          .limit(1)
        if (current?.isProtected) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `This project is protected and cannot be archived or retyped`,
          })
        }
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

  // Soft delete: move the project to the trash. The purge sweep hard-deletes it
  // (cascade) after PROJECT_TRASH_RETENTION_HOURS; owners can restore it before
  // then. Direct select (not the trash-filtered helper) so an already-trashed
  // project stays resolvable for the idempotent no-op.
  delete: authedProcedure
    .input(z.object({ projectId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [project] = await ctx.db
        .select({
          workspaceId: projects.workspaceId,
          deletedAt: projects.deletedAt,
          isProtected: projects.isProtected,
        })
        .from(projects)
        .where(eq(projects.id, input.projectId))
        .limit(1)

      if (!project) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Project not found` })
      }

      await assertWorkspaceOwner(ctx.session.user.id, project.workspaceId)

      if (project.isProtected) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This project is protected and cannot be deleted`,
        })
      }

      // Already trashed → nothing changed, so no sync barrier needed.
      if (project.deletedAt) {
        return { ok: true as const }
      }

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(projects)
          .set({ deletedAt: new Date() })
          .where(
            and(eq(projects.id, input.projectId), isNull(projects.deletedAt))
          )
        return { ok: true as const, txId }
      })
      invalidatePublicProjectCache()
      return result
    }),

  // Owner-only: pull a project back out of the trash. Direct select bypasses the
  // trash-filtered helper (which would 404 a trashed project). The slug was
  // reserved the whole time, so there is no restore-time conflict.
  restore: authedProcedure
    .input(z.object({ projectId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
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
        const restored = await tx
          .update(projects)
          .set({ deletedAt: null })
          .where(
            and(eq(projects.id, input.projectId), isNotNull(projects.deletedAt))
          )
          .returning({ id: projects.id })
        if (restored.length === 0) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Project is not in the trash`,
          })
        }
        return { ok: true as const, txId }
      })
      invalidatePublicProjectCache()
      return result
    }),

  // Owner-only: the trashed projects for the web trash UI. Restore is owner-only
  // and the trash card lives in the owner-gated Projects section, so member
  // visibility buys nothing.
  listDeleted: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertWorkspaceOwner(ctx.session.user.id, input.workspaceId)
      const rows = await ctx.db
        .select({
          id: projects.id,
          name: projects.name,
          slug: projects.slug,
          prefix: projects.prefix,
          color: projects.color,
          type: projects.type,
          deletedAt: projects.deletedAt,
        })
        .from(projects)
        .where(
          and(
            eq(projects.workspaceId, input.workspaceId),
            isNotNull(projects.deletedAt)
          )
        )
      return rows.map((row) => ({
        ...row,
        purgeAt: row.deletedAt
          ? new Date(row.deletedAt.getTime() + PROJECT_TRASH_RETENTION_MS)
          : null,
      }))
    }),
})
