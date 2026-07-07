import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, isNull } from "drizzle-orm"
import type { db } from "@/db/connection"
import { router, authedProcedure } from "@/lib/trpc"
import { issues, projects, repositories } from "@/db/schema"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
  isWorkspaceModerator,
} from "@/lib/workspace-membership"
import { isUserAdmin } from "@/lib/admin"
import {
  fetchBranchDiff,
  githubAppConfigured,
  peekBranchDiff,
  resolveRepoDefaultBranch,
  resolveRepoDefaultBranchCached,
  resolveRepoInstallationToken,
} from "@/lib/integrations/github-app"
import { assertRepoInstallationAccess } from "@/lib/trpc/integrations"

// GitHub installation tokens last ~1h; we hand back a conservative 55-minute
// horizon so the desktop launcher refreshes before the real expiry. (The
// storage-free App path doesn't expose the precise API expiry to callers.)
const INSTALLATION_TOKEN_TTL_MS = 55 * 60 * 1000

// The default branch-name prefix for issue worktrees: `exp/<IDENTIFIER>`.
export const BRANCH_PREFIX_DEFAULT = `exp/`

// The worktree branch a coding session pushes for an issue identifier.
export function issueBranchName(identifier: string): string {
  return `${BRANCH_PREFIX_DEFAULT}${identifier}`
}

// Postgres FK `restrict` violation (SQLSTATE 23503). node-postgres surfaces the
// code on the thrown error; drizzle rethrows it unwrapped.
export function isForeignKeyViolation(err: unknown): boolean {
  const code = (err as { code?: string; cause?: { code?: string } })?.code
  const causeCode = (err as { cause?: { code?: string } })?.cause?.code
  return code === `23503` || causeCode === `23503`
}

// "repository backs N projects" — the CONFLICT message when a delete is blocked
// by a project still pointing at the repo.
export function repoInUseMessage(count: number): string {
  return `Cannot remove — this repository backs ${count} project${count === 1 ? `` : `s`}. Retarget or delete those projects first.`
}

const fullNameSchema = z
  .string()
  .min(1)
  .max(255)
  .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`)

// Repo management (add/remove/retarget) is owner-or-admin, mirroring member
// management. Reads (list/forIssue/branchDiff) and token minting are
// member-gated — and moderator-only on public workspaces (below).
export async function assertCanManageRepos(userId: string, workspaceId: string) {
  if (await isUserAdmin(userId)) return
  await assertWorkspaceMember(userId, workspaceId, [`owner`])
}

// Repo-backed capabilities (list/forIssue/branchDiff reads, JIT token minting)
// reach into the backing GitHub repo, so on a PUBLIC workspace — where
// membership is an open self-service join — they are moderator-only: a plain
// self-joined member must never see private-repo contents or hold an
// installation token. In a private workspace every member passes
// (isWorkspaceModerator semantics).
async function assertRepoCapability(userId: string, workspaceId: string) {
  await assertWorkspaceMember(userId, workspaceId)
  if (!(await isWorkspaceModerator(userId, workspaceId))) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Repository access on a public workspace is restricted to moderators`,
    })
  }
}

type Db = typeof db
type Tx = Parameters<Parameters<Db[`transaction`]>[0]>[0]

// The exact `repositories.add` validation + upsert, reusable inside another
// transaction (projects.create's inline connect path). Verifies the repo
// resolves to a GitHub App installation the CALLER is attributed to — the App
// JWT can reach every installation of the App, so this check (not mere
// installed-ness) is what stops one user binding another user's private repo
// to their own workspace. Upserts + un-archives, returns the repository id.
// Owner/admin + plan-cap checks are the caller's responsibility (done before
// opening the tx). The persisted installation id is the authoritative one
// resolved from GitHub, never the client-supplied claim.
export async function connectRepositoryInTx(
  tx: Tx,
  input: {
    userId: string
    workspaceId: string
    fullName: string
    defaultBranch?: string
    private?: boolean
  }
): Promise<string> {
  const installationId = await assertRepoInstallationAccess(
    input.userId,
    input.fullName
  )

  // Never blind-seed `main` (L30): when the caller didn't supply a branch, ask
  // GitHub for the authoritative default. Only fall back to `main` when the live
  // lookup yields nothing (App unconfigured / repo gone / transient failure), and
  // log so a wrong-fallback row is traceable.
  let defaultBranch = input.defaultBranch
  if (!defaultBranch) {
    try {
      defaultBranch = (await resolveRepoDefaultBranch(input.fullName)) ?? undefined
    } catch (err) {
      console.warn(
        `[repositories] default-branch lookup threw for ${input.fullName}; falling back to main`,
        err
      )
    }
    if (!defaultBranch) {
      console.warn(
        `[repositories] could not resolve default branch for ${input.fullName}; falling back to main`
      )
      defaultBranch = `main`
    }
  }

  const [inserted] = await tx
    .insert(repositories)
    .values({
      workspaceId: input.workspaceId,
      fullName: input.fullName,
      defaultBranch,
      private: input.private ?? false,
      installationId,
    })
    .onConflictDoNothing({
      target: [repositories.workspaceId, repositories.fullName],
    })
    .returning({ id: repositories.id })
  if (inserted) return inserted.id

  // Already registered — un-archive and return the existing row.
  const [existing] = await tx
    .update(repositories)
    .set({ archivedAt: null })
    .where(
      and(
        eq(repositories.workspaceId, input.workspaceId),
        eq(repositories.fullName, input.fullName)
      )
    )
    .returning({ id: repositories.id })
  // A concurrent delete between the onConflictDoNothing INSERT and this UPDATE
  // leaves nothing to un-archive — surface a retryable CONFLICT rather than
  // dereferencing undefined.
  if (!existing) {
    throw new TRPCError({
      code: `CONFLICT`,
      message: `Repository was removed concurrently — retry.`,
    })
  }
  return existing.id
}

// Heal a batch of repo rows against GitHub's authoritative default branch (L30):
// return each row carrying the live value when it's known and disagrees, and
// persist the fix best-effort (`persist` failures never fail the read). The
// `resolve` lookup defaults to the short-cached resolver so a fan-out read can't
// hammer GitHub; both `resolve` and `persist` are injectable for tests.
export async function healRepoDefaultBranches<
  R extends { id: string; fullName: string; defaultBranch: string }
>(
  repos: R[],
  persist: (id: string, defaultBranch: string) => Promise<void>,
  resolve: (fullName: string) => Promise<string | null> = resolveRepoDefaultBranchCached
): Promise<R[]> {
  return Promise.all(
    repos.map(async (repo) => {
      let live: string | null = null
      try {
        live = await resolve(repo.fullName)
      } catch (err) {
        console.warn(
          `[repositories] default-branch heal lookup failed for ${repo.fullName}`,
          err
        )
      }
      if (!live || live === repo.defaultBranch) return repo
      try {
        await persist(repo.id, live)
      } catch (err) {
        console.warn(
          `[repositories] default-branch heal write failed for ${repo.fullName}`,
          err
        )
      }
      return { ...repo, defaultBranch: live }
    })
  )
}

// Project → repo resolution (v4): a project is backed by exactly one repo via
// `projects.repositoryId`. Returns null only for dangling data (archived repo).
// Shared by repositories.forIssue and steer.startSession's precondition.
export async function resolveProjectRepository(projectId: string) {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select({
      repositoryId: repositories.id,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
      installationId: repositories.installationId,
    })
    .from(projects)
    .innerJoin(repositories, eq(repositories.id, projects.repositoryId))
    .where(and(eq(projects.id, projectId), isNull(repositories.archivedAt)))
    .limit(1)
  return row ?? null
}

async function loadRepository(repositoryId: string) {
  const { db } = await import(`@/db/connection`)
  const [repo] = await db
    .select({
      id: repositories.id,
      workspaceId: repositories.workspaceId,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
      installationId: repositories.installationId,
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
  // Member-readable (moderator-only on public workspaces): the workspace's
  // repos + the projects each one backs (for the settings "in use by" chips
  // and mobile pickers).
  list: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertRepoCapability(ctx.session.user.id, input.workspaceId)

      const rawRepos = await ctx.db
        .select()
        .from(repositories)
        .where(
          and(
            eq(repositories.workspaceId, input.workspaceId),
            isNull(repositories.archivedAt)
          )
        )
        .orderBy(asc(repositories.sortOrder), asc(repositories.fullName))

      // Heal a stale/misseeded `defaultBranch` the same way `installationToken`
      // does — GitHub is authoritative (L30). Bounded by a short in-process
      // cache so a fan-out read can't hammer GitHub; the write is best-effort but
      // the returned rows always carry the live value when known.
      const repos = await healRepoDefaultBranches(rawRepos, (id, defaultBranch) =>
        ctx.db
          .update(repositories)
          .set({ defaultBranch })
          .where(eq(repositories.id, id))
          .then(() => {})
      )

      // Projects that point at these repos, computed from projects.repositoryId.
      const projectRows = await ctx.db
        .select({
          id: projects.id,
          name: projects.name,
          slug: projects.slug,
          repositoryId: projects.repositoryId,
        })
        .from(projects)
        .where(
          and(
            eq(projects.workspaceId, input.workspaceId),
            isNull(projects.archivedAt)
          )
        )

      return repos.map((repo) => ({
        ...repo,
        projects: projectRows
          .filter((p) => p.repositoryId === repo.id)
          .map((p) => ({ id: p.id, name: p.name, slug: p.slug })),
      }))
    }),

  // Owner/admin: register a repo reachable through one of the CALLER's GitHub
  // App installations. The installation id is resolved server-side from
  // GitHub — clients never supply it.
  add: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        fullName: fullNameSchema,
        defaultBranch: z.string().min(1).max(255).optional(),
        private: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertCanManageRepos(ctx.session.user.id, input.workspaceId)

      // The install-check + upsert + un-archive sequence is connectRepositoryInTx
      // (shared with projects.create's inline connect) — call it, then load the
      // full row to hand back.
      const repository = await ctx.db.transaction(async (tx) => {
        const repositoryId = await connectRepositoryInTx(tx, {
          userId: ctx.session.user.id,
          workspaceId: input.workspaceId,
          fullName: input.fullName,
          defaultBranch: input.defaultBranch,
          private: input.private,
        })
        const [row] = await tx
          .select()
          .from(repositories)
          .where(eq(repositories.id, repositoryId))
          .limit(1)
        return row
      })
      return { repository }
    }),

  // Owner/admin: hard-delete. Blocked (CONFLICT) while any project still points
  // at the repo — the `projects.repository_id` FK is `restrict`.
  remove: authedProcedure
    .input(z.object({ repositoryId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      await assertCanManageRepos(ctx.session.user.id, repo.workspaceId)
      try {
        await ctx.db
          .delete(repositories)
          .where(eq(repositories.id, input.repositoryId))
      } catch (err) {
        if (isForeignKeyViolation(err)) {
          const backing = await ctx.db
            .select({ id: projects.id })
            .from(projects)
            .where(eq(projects.repositoryId, input.repositoryId))
          throw new TRPCError({
            code: `CONFLICT`,
            message: repoInUseMessage(backing.length),
          })
        }
        throw err
      }
      return { ok: true as const }
    }),

  // The launcher's clone-target resolution: issue → project → repositoryId.
  // Member-readable (moderator-only on public workspaces).
  forIssue: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const issueCtx = await getIssueWorkspaceContext(input.issueId)
      await assertRepoCapability(ctx.session.user.id, issueCtx.workspaceId)
      return resolveProjectRepository(issueCtx.projectId)
    }),

  // Member-gated middle tier of remote Changes visibility (§4.8, L18): the
  // issue's `exp/<IDENTIFIER>` branch compared against the repo default branch,
  // returned in the shared `prFiles` shape. Null when the branch was never
  // pushed (GitHub 404). ~60s per-branch cache lives in github-app.fetchBranchDiff.
  branchDiff: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const issueCtx = await getIssueWorkspaceContext(input.issueId)
      await assertRepoCapability(ctx.session.user.id, issueCtx.workspaceId)

      const [issue] = await ctx.db
        .select({ identifier: issues.identifier })
        .from(issues)
        .where(eq(issues.id, input.issueId))
        .limit(1)
      if (!issue?.identifier) return null

      const repo = await resolveProjectRepository(issueCtx.projectId)
      if (!repo) return null

      const branch = issueBranchName(issue.identifier)
      // Warm-cache hit short-circuits BEFORE the token/installation lookups
      // (both uncached GitHub round-trips) — fetchBranchDiff would peek the same
      // cache, but only after resolveRepoInstallationToken already paid for a
      // /repos/{repo}/installation call.
      const cached = peekBranchDiff(repo.fullName, repo.defaultBranch, branch)
      if (cached) return cached

      const token = await resolveRepoInstallationToken(repo.fullName, {
        fallbackInstallationId: repo.installationId,
      })
      return fetchBranchDiff({
        repo: repo.fullName,
        base: repo.defaultBranch,
        branch,
        token,
      })
    }),

  // Session-gated JIT push token for the native launcher's token-embedded git
  // remote. Never persisted — minted per session and expires. Replaces the
  // deleted companion.repoToken.
  installationToken: authedProcedure
    .input(z.object({ repositoryId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      // Team coding: any member of the repo's workspace may mint a JIT token
      // (assertRepoCapability = membership, moderator-clamped on PUBLIC
      // workspaces only — so on the public feedback board it's owner/moderator
      // exclusively, while in a normal private workspace every teammate passes).
      // Per-installer attribution is intentionally NOT required here: the repo
      // is only present in this workspace because a member legitimately
      // connected it, and connectRepositoryInTx already enforced
      // assertRepoInstallationAccess at connect time. The workspace's ownership
      // of the repo row is the authorization; requiring the caller to also be
      // the original installer would break coding for every other teammate.
      await assertRepoCapability(ctx.session.user.id, repo.workspaceId)
      if (!githubAppConfigured()) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not configured on this instance`,
        })
      }

      // Fall back to the installation persisted at connect time when GitHub's
      // per-repo lookup misses — see resolveRepoInstallationToken (fixes the
      // spurious 412 when the repo IS covered by a known installation).
      const token = await resolveRepoInstallationToken(repo.fullName, {
        fallbackInstallationId: repo.installationId,
      })
      if (!token) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The Exponential GitHub App is not installed on ${repo.fullName}. Reconnect it in workspace settings.`,
        })
      }
      // GitHub is authoritative on the default branch — a stale/misseeded row
      // (e.g. `main` for a `master` repo) would break the launcher's
      // `git worktree add … origin/<default>`. Prefer the live value; heal the
      // row when it drifted; fall back to the stored value if the lookup fails.
      const liveDefaultBranch = await resolveRepoDefaultBranch(repo.fullName)
      if (liveDefaultBranch && liveDefaultBranch !== repo.defaultBranch) {
        await ctx.db
          .update(repositories)
          .set({ defaultBranch: liveDefaultBranch })
          .where(eq(repositories.id, repo.id))
      }
      return {
        token,
        fullName: repo.fullName,
        defaultBranch: liveDefaultBranch ?? repo.defaultBranch,
        expiresAt: new Date(Date.now() + INSTALLATION_TOKEN_TTL_MS),
      }
    }),
})
