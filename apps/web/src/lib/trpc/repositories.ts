import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { projectRepositories, repositories } from "@/db/schema"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
  getProjectWorkspaceId,
} from "@/lib/workspace-membership"
import { isUserAdmin } from "@/lib/admin"
import { assertWithinPlanLimits } from "@/lib/billing"
import { resolveRepoInstallationToken } from "@/lib/integrations/github-app"

// GitHub installation tokens last ~1h; we hand back a conservative 55-minute
// horizon so the desktop launcher refreshes before the real expiry. (The
// storage-free App path doesn't expose the precise API expiry to callers.)
const INSTALLATION_TOKEN_TTL_MS = 55 * 60 * 1000

const fullNameSchema = z
  .string()
  .min(1)
  .max(255)
  .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`)

// Repo management (add/remove/link) is owner-or-admin, mirroring member
// management. Reads (list/forIssue/installationToken) are member-gated.
async function assertCanManageRepos(userId: string, workspaceId: string) {
  if (await isUserAdmin(userId)) return
  await assertWorkspaceMember(userId, workspaceId, [`owner`])
}

// Project → repo resolution: the primary link, else the sole link, else null
// (multiple links with no primary is ambiguous). Shared by repositories.forIssue
// and steer.startSession's repo-linked precondition.
export async function resolveProjectRepository(projectId: string) {
  const { db } = await import(`@/db/connection`)
  const links = await db
    .select({
      repositoryId: projectRepositories.repositoryId,
      isPrimary: projectRepositories.isPrimary,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
    })
    .from(projectRepositories)
    .innerJoin(
      repositories,
      eq(repositories.id, projectRepositories.repositoryId)
    )
    .where(
      and(
        eq(projectRepositories.projectId, projectId),
        isNull(repositories.archivedAt)
      )
    )

  if (links.length === 0) return null
  const chosen =
    links.find((l) => l.isPrimary) ?? (links.length === 1 ? links[0] : null)
  if (!chosen) return null // multiple links, no primary → ambiguous
  return {
    repositoryId: chosen.repositoryId,
    fullName: chosen.fullName,
    defaultBranch: chosen.defaultBranch,
  }
}

async function loadRepository(repositoryId: string) {
  const { db } = await import(`@/db/connection`)
  const [repo] = await db
    .select({
      id: repositories.id,
      workspaceId: repositories.workspaceId,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
    })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  if (!repo) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Repository not found` })
  }
  return repo
}

export const repositoriesRouter = router({
  // Member-readable: the workspace's repos + their project links (for the
  // settings link editor and native launcher).
  list: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId)

      const repos = await ctx.db
        .select()
        .from(repositories)
        .where(
          and(
            eq(repositories.workspaceId, input.workspaceId),
            isNull(repositories.archivedAt)
          )
        )
        .orderBy(asc(repositories.sortOrder), asc(repositories.fullName))

      const repoIds = repos.map((r) => r.id)
      const links =
        repoIds.length > 0
          ? await ctx.db
              .select({
                repositoryId: projectRepositories.repositoryId,
                projectId: projectRepositories.projectId,
                isPrimary: projectRepositories.isPrimary,
              })
              .from(projectRepositories)
              .where(inArray(projectRepositories.repositoryId, repoIds))
          : []

      return repos.map((repo) => ({
        ...repo,
        projectLinks: links
          .filter((l) => l.repositoryId === repo.id)
          .map((l) => ({ projectId: l.projectId, isPrimary: l.isPrimary })),
      }))
    }),

  // Owner/admin: register a repo the App is installed on. The install check is
  // the gate — a repo the App can't reach can never mint a push token.
  add: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        fullName: fullNameSchema,
        defaultBranch: z.string().min(1).max(255).optional(),
        private: z.boolean().optional(),
        installationId: z.number().int().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertCanManageRepos(ctx.session.user.id, input.workspaceId)
      // Plan cap on connected (non-archived) repos — throws PRECONDITION_FAILED
      // with an upgrade-nudge message; self-hosted is unlimited.
      await assertWithinPlanLimits(input.workspaceId, `repositories`)

      const token = await resolveRepoInstallationToken(input.fullName)
      if (!token) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The Exponential GitHub App is not installed on ${input.fullName}. Install it, then try again.`,
        })
      }

      const [inserted] = await ctx.db
        .insert(repositories)
        .values({
          workspaceId: input.workspaceId,
          fullName: input.fullName,
          defaultBranch: input.defaultBranch ?? `main`,
          private: input.private ?? false,
          installationId: input.installationId ?? null,
        })
        .onConflictDoNothing({
          target: [repositories.workspaceId, repositories.fullName],
        })
        .returning()

      if (inserted) return { repository: inserted }

      // Already registered — un-archive if needed and return the existing row.
      const [existing] = await ctx.db
        .update(repositories)
        .set({ archivedAt: null })
        .where(
          and(
            eq(repositories.workspaceId, input.workspaceId),
            eq(repositories.fullName, input.fullName)
          )
        )
        .returning()
      return { repository: existing }
    }),

  // Owner/admin: hard-delete; project_repositories links cascade via FK.
  remove: authedProcedure
    .input(z.object({ repositoryId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      await assertCanManageRepos(ctx.session.user.id, repo.workspaceId)
      await ctx.db
        .delete(repositories)
        .where(eq(repositories.id, input.repositoryId))
      return { ok: true as const }
    }),

  linkProject: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        repositoryId: z.string().uuid(),
        isPrimary: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      const project = await getProjectWorkspaceId(input.projectId)
      if (project.workspaceId !== repo.workspaceId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Project and repository must belong to the same workspace`,
        })
      }
      await assertCanManageRepos(ctx.session.user.id, repo.workspaceId)

      const isPrimary = input.isPrimary ?? false
      await ctx.db.transaction(async (tx) => {
        // The partial unique index allows one primary per project — clear the
        // current one before promoting this link.
        if (isPrimary) {
          await tx
            .update(projectRepositories)
            .set({ isPrimary: false })
            .where(
              and(
                eq(projectRepositories.projectId, input.projectId),
                eq(projectRepositories.isPrimary, true)
              )
            )
        }
        await tx
          .insert(projectRepositories)
          .values({
            projectId: input.projectId,
            repositoryId: input.repositoryId,
            isPrimary,
          })
          .onConflictDoUpdate({
            target: [
              projectRepositories.projectId,
              projectRepositories.repositoryId,
            ],
            set: { isPrimary },
          })
      })
      return { ok: true as const }
    }),

  unlinkProject: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        repositoryId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      await assertCanManageRepos(ctx.session.user.id, repo.workspaceId)
      await ctx.db
        .delete(projectRepositories)
        .where(
          and(
            eq(projectRepositories.projectId, input.projectId),
            eq(projectRepositories.repositoryId, input.repositoryId)
          )
        )
      return { ok: true as const }
    }),

  setPrimary: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        repositoryId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      await assertCanManageRepos(ctx.session.user.id, repo.workspaceId)

      const [link] = await ctx.db
        .select({ repositoryId: projectRepositories.repositoryId })
        .from(projectRepositories)
        .where(
          and(
            eq(projectRepositories.projectId, input.projectId),
            eq(projectRepositories.repositoryId, input.repositoryId)
          )
        )
        .limit(1)
      if (!link) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Link the repository to the project before making it primary`,
        })
      }

      await ctx.db.transaction(async (tx) => {
        await tx
          .update(projectRepositories)
          .set({ isPrimary: false })
          .where(
            and(
              eq(projectRepositories.projectId, input.projectId),
              eq(projectRepositories.isPrimary, true)
            )
          )
        await tx
          .update(projectRepositories)
          .set({ isPrimary: true })
          .where(
            and(
              eq(projectRepositories.projectId, input.projectId),
              eq(projectRepositories.repositoryId, input.repositoryId)
            )
          )
      })
      return { ok: true as const }
    }),

  // The launcher's clone-target resolution: issue → project → primary link
  // (else the sole link, else null). Member-readable.
  forIssue: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const issueCtx = await getIssueWorkspaceContext(input.issueId)
      await assertWorkspaceMember(ctx.session.user.id, issueCtx.workspaceId)
      return resolveProjectRepository(issueCtx.projectId)
    }),

  // Session-gated JIT push token for the native launcher's token-embedded git
  // remote. Never persisted — minted per session and expires. Replaces the
  // deleted companion.repoToken.
  installationToken: authedProcedure
    .input(z.object({ repositoryId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      await assertWorkspaceMember(ctx.session.user.id, repo.workspaceId)

      const token = await resolveRepoInstallationToken(repo.fullName)
      if (!token) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The Exponential GitHub App is not installed on ${repo.fullName}. Reconnect it in workspace settings.`,
        })
      }
      return {
        token,
        fullName: repo.fullName,
        defaultBranch: repo.defaultBranch,
        expiresAt: new Date(Date.now() + INSTALLATION_TOKEN_TTL_MS),
      }
    }),
})
