import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { projects, repositories } from "@/db/schema"
import { and, eq, isNotNull, isNull } from "drizzle-orm"
import {
  projectIconSchema,
  projectTypeSchema,
  PROJECT_TRASH_RETENTION_MS,
  type ProjectType,
} from "@exp/db-schema/domain"
import {
  assertProjectMember,
  resolveWorkspaceAccess,
  assertWorkspaceOwner,
  invalidatePublicProjectCache,
} from "@/lib/workspace-membership"
import { invalidatePublicMetaCache } from "@/lib/seo/public-meta"
import type { db } from "@/db/connection"
import {
  assertCanManageRepos,
  connectRepositoryInTx,
} from "@/lib/trpc/repositories"
import { assertCanUseHelpdesk } from "@/lib/billing"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

// A repository is OPTIONAL on every project (the type collapse): coding
// features gate purely on repo presence. Create either points at an existing
// registry repo (`repositoryId`) or connects one inline (`fullName`, same
// validation as repositories.add).
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

// Dual-write of the legacy `type` column while shipped native clients still
// read it (dropped in the min-version-gated finale): public → feedback, else
// repo-backed → dev, else tasks.
function deriveLegacyType(
  isPublic: boolean,
  repositoryId: string | null
): ProjectType {
  return isPublic ? `feedback` : repositoryId ? `dev` : `tasks`
}

// The deprecated `type` input survives as an alias for external MCP clients:
// only 'feedback' ever mapped to publicness.
function isPublicFromInput(
  isPublic: boolean | undefined,
  type: ProjectType | undefined
): boolean | undefined {
  return isPublic ?? (type === undefined ? undefined : type === `feedback`)
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
        // Public-board switch (replaces type='feedback'). The deprecated
        // `type` alias below still maps for external MCP clients.
        isPublic: z.boolean().optional(),
        icon: projectIconSchema.nullish(),
        type: projectTypeSchema.optional(),
        // Anonymous-visitor visibility toggles (public boards only; inert on
        // private projects).
        publicShowComments: z.boolean().optional(),
        publicShowActivity: z.boolean().optional(),
        // Always optional (coding features gate on repo presence). Either
        // target an existing registry repo or connect one inline in the same
        // transaction (onboarding/create dialogs stay a single call).
        repository: repositoryInputSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`
      )
      const isPublic = isPublicFromInput(input.isPublic, input.type) ?? false
      const repositoryInput = input.repository
      const inlineConnect =
        repositoryInput != null && `fullName` in repositoryInput
      if (inlineConnect) {
        // The inline connect path needs owner/admin (repo management), beyond
        // the member-level project create. Projects & repos are unlimited on
        // every tier now (v5 per-seat model), so there is no plan cap here.
        await assertCanManageRepos(ctx.session.user.id, input.workspaceId)
      }
      if (isPublic) {
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
              isPublic,
              icon: input.icon ?? null,
              type: deriveLegacyType(isPublic, repositoryId),
              publicShowComments: input.publicShowComments ?? true,
              publicShowActivity: input.publicShowActivity ?? false,
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

      if (isPublic) invalidatePublicProjectCache()
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
        // Repos attach/detach freely on any project; only the dual-written
        // legacy `type` needs the current publicness to stay consistent.
        const [row] = await tx
          .select({ isPublic: projects.isPublic })
          .from(projects)
          .where(eq(projects.id, input.projectId))
          .limit(1)
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
          .set({
            repositoryId,
            type: deriveLegacyType(row?.isPublic ?? false, repositoryId),
          })
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
        // Public-board switch; the deprecated `type` alias still maps for
        // external MCP clients (only 'feedback' ever meant public).
        isPublic: z.boolean().optional(),
        icon: projectIconSchema.nullable().optional(),
        type: projectTypeSchema.optional(),
        publicShowComments: z.boolean().optional(),
        publicShowActivity: z.boolean().optional(),
        helpdeskEnabled: z.boolean().optional(),
        archivedAt: z
          .string()
          .datetime()
          .transform((value) => new Date(value))
          .nullable()
          .optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, type: legacyType, ...updates } = input
      const isPublicUpdate = isPublicFromInput(updates.isPublic, legacyType)
      updates.isPublic = isPublicUpdate

      const projectRecord = await assertProjectMember(ctx.session.user.id, id)

      // Archiving, publicness flips, public-visibility toggles and the
      // helpdesk switch are privacy/structure-significant —
      // workspace-owner-only. Name/color/icon stay member-editable.
      const ownerGated =
        Object.hasOwn(updates, `archivedAt`) ||
        isPublicUpdate !== undefined ||
        updates.publicShowComments !== undefined ||
        updates.publicShowActivity !== undefined ||
        updates.helpdeskEnabled !== undefined
      if (ownerGated) {
        await assertWorkspaceOwner(
          ctx.session.user.id,
          projectRecord.workspaceId
        )
      }
      // The helpdesk is Pro+ on cloud; disabling is always allowed (a
      // downgraded workspace must be able to turn it off).
      if (updates.helpdeskEnabled === true) {
        await assertCanUseHelpdesk(projectRecord.workspaceId)
      }

      const [current] = await ctx.db
        .select({
          isProtected: projects.isProtected,
          isPublic: projects.isPublic,
          repositoryId: projects.repositoryId,
        })
        .from(projects)
        .where(eq(projects.id, id))
        .limit(1)

      // Protected projects (the dogfood board) can't be archived or have
      // their publicness flipped; name/color/icon stay editable.
      const attemptsArchiveOrFlip =
        (Object.hasOwn(updates, `archivedAt`) && updates.archivedAt != null) ||
        (isPublicUpdate !== undefined && isPublicUpdate !== current?.isPublic)
      if (attemptsArchiveOrFlip && current?.isProtected) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This project is protected and cannot be archived or made private`,
        })
      }

      // Dual-write the legacy type while natives still read it.
      const nextIsPublic = isPublicUpdate ?? current?.isPublic ?? false
      const [project] = await ctx.db
        .update(projects)
        .set({
          ...updates,
          type: deriveLegacyType(nextIsPublic, current?.repositoryId ?? null),
        })
        .where(eq(projects.id, id))
        .returning()

      // Type/toggle/archive changes can alter the instance's public surface.
      if (ownerGated) {
        invalidatePublicProjectCache()
        invalidatePublicMetaCache()
      }

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
      invalidatePublicMetaCache()
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
          icon: projects.icon,
          isPublic: projects.isPublic,
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
