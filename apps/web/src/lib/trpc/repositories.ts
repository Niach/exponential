import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, isNotNull, isNull } from "drizzle-orm"
import type { db } from "@/db/connection"
import { router, authedProcedure } from "@/lib/trpc"
import { issues, projects, releases, repositories } from "@/db/schema"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"
import {
  fetchBranchDiff,
  githubAppConfigured,
  peekBranchDiff,
  resolveRepoDefaultBranch,
  resolveRepoDefaultBranchCached,
  resolveRepoInstallationToken,
  resolveRepoInstallationTokenInfo,
} from "@/lib/integrations/github-app"
import {
  GitHubMergeError,
  listOpenPulls,
  mergePullRequest,
  type OpenPull,
} from "@/lib/integrations/github-pr"
import {
  assertCanManageRepos,
  assertRepoInstallationAccess,
  isInstallationLinkedToWorkspace,
} from "@/lib/trpc/integrations"

// assertCanManageRepos moved to integrations.ts (both routers need it and the
// import direction only works that way) — re-exported for existing callers.
export { assertCanManageRepos }

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
// by a project still pointing at the repo. A trashed project keeps its repo FK
// (restrict) but is hidden from the synced "in use by" chips, so name that case.
export function repoInUseMessage(count: number): string {
  return `Cannot remove — this repository backs ${count} project${count === 1 ? `` : `s`}. Retarget or delete those projects first (a project in the trash may still use it).`
}

const fullNameSchema = z
  .string()
  .min(1)
  .max(255)
  .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`)

// Repo-backed capabilities (list/forIssue/branchDiff reads, JIT token minting)
// reach into the backing GitHub repo. Member-gated: since v7 every membership
// is an explicit invite (the self-service public join is gone), so the old
// moderator-only clamp for self-joined public-workspace members is obsolete —
// anonymous public-board viewers never reach these procedures at all.
async function assertRepoCapability(userId: string, workspaceId: string) {
  await assertWorkspaceMember(userId, workspaceId)
}

type Db = typeof db
type Tx = Parameters<Parameters<Db[`transaction`]>[0]>[0]

// The exact `repositories.add` validation + upsert, reusable inside another
// transaction (projects.create's inline connect path). Verifies the repo
// resolves to a GitHub App installation LINKED to the target workspace — the
// App JWT can reach every installation of the App, so this check (not mere
// installed-ness) is what stops an owner binding an unrelated account's
// private repo to their workspace. Upserts + un-archives, returns the
// repository id. Owner/admin + plan-cap checks are the caller's responsibility
// (done before opening the tx). The persisted installation id is the
// authoritative one resolved from GitHub, never the client-supplied claim.
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
    input.workspaceId,
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
      inaccessibleAt: null,
    })
    .onConflictDoNothing({
      target: [repositories.workspaceId, repositories.fullName],
    })
    .returning({ id: repositories.id })
  if (inserted) return inserted.id

  // Already registered — un-archive, refresh the installation binding, clear
  // any stale no-access flag (a re-connect just proved access), and return the
  // existing row.
  const [existing] = await tx
    .update(repositories)
    .set({ archivedAt: null, installationId, inaccessibleAt: null })
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
// persist the fix best-effort (`persist` failures never fail the read). A
// resolved live branch also proves the App can still reach the repo, so it
// clears a stale `inaccessibleAt` flag; a null result never SETS the flag —
// the lookup is too flaky to be evidence of lost access (only webhooks and the
// verified token mint set it). The `resolve` lookup defaults to the
// short-cached resolver so a fan-out read can't hammer GitHub; both `resolve`
// and `persist` are injectable for tests.
export async function healRepoDefaultBranches<
  R extends {
    id: string
    fullName: string
    defaultBranch: string
    inaccessibleAt?: Date | null
  }
>(
  repos: R[],
  persist: (
    id: string,
    patch: { defaultBranch?: string; clearInaccessible?: boolean }
  ) => Promise<void>,
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
      if (!live) return repo
      const patch: { defaultBranch?: string; clearInaccessible?: boolean } = {}
      if (live !== repo.defaultBranch) patch.defaultBranch = live
      if (repo.inaccessibleAt != null) patch.clearInaccessible = true
      if (!patch.defaultBranch && !patch.clearInaccessible) return repo
      try {
        await persist(repo.id, patch)
      } catch (err) {
        console.warn(
          `[repositories] default-branch heal write failed for ${repo.fullName}`,
          err
        )
      }
      return {
        ...repo,
        defaultBranch: live,
        ...(patch.clearInaccessible ? { inaccessibleAt: null } : {}),
      }
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
      inaccessibleAt: repositories.inaccessibleAt,
    })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  if (!repo) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Repository not found` })
  }
  return repo
}

// The Reviews queue's "everything else" source: open PRs listed live from
// GitHub for every workspace repo. Short in-process cache so tab switches and
// re-mounts don't hammer the GitHub API; busted by mergePull.
const OPEN_PULLS_TTL_MS = 60_000
interface CachedOpenPulls {
  expiresAt: number
  repos: Array<{ repositoryId: string; fullName: string; pulls: OpenPull[] }>
}
const openPullsCache = new Map<string, CachedOpenPulls>()

// Resolve the App installation token for a repo row, honoring the same
// link-gate as installationToken: a token is only used when the installation
// serving the repo is still claimed by the repo's workspace. Returns null when
// no gated token is available (callers may still read public repos
// unauthenticated / via GITHUB_TOKEN).
async function resolveGatedRepoToken(repo: {
  workspaceId: string
  fullName: string
  installationId: number | null
}): Promise<string | null> {
  if (!githubAppConfigured()) return null
  const resolved = await resolveRepoInstallationTokenInfo(repo.fullName, {
    fallbackInstallationId: repo.installationId,
  })
  if (!resolved) return null
  if (
    !(await isInstallationLinkedToWorkspace(
      repo.workspaceId,
      resolved.installationId
    ))
  ) {
    return null
  }
  return resolved.token
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
      // the returned rows always carry the live value when known. A successful
      // lookup doubles as an accessibility proof and clears a stale no-access
      // flag.
      const repos = await healRepoDefaultBranches(rawRepos, (id, patch) =>
        ctx.db
          .update(repositories)
          .set({
            ...(patch.defaultBranch ? { defaultBranch: patch.defaultBranch } : {}),
            ...(patch.clearInaccessible ? { inaccessibleAt: null } : {}),
          })
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

  // Member-readable: every open pull request across the workspace's repos
  // that is NOT already linked to an issue (those rows render from the synced
  // issues shape). Listed live from GitHub so PRs opened outside the issue
  // flow — release PRs on other branches, manual PRs, external contributors —
  // still land in the Reviews queue.
  openPulls: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertRepoCapability(ctx.session.user.id, input.workspaceId)

      const cached = openPullsCache.get(input.workspaceId)
      if (cached && cached.expiresAt > Date.now()) {
        return { repos: cached.repos }
      }

      const repos = await ctx.db
        .select({
          id: repositories.id,
          fullName: repositories.fullName,
          workspaceId: repositories.workspaceId,
          installationId: repositories.installationId,
        })
        .from(repositories)
        .where(
          and(
            eq(repositories.workspaceId, input.workspaceId),
            isNull(repositories.archivedAt)
          )
        )
        .orderBy(asc(repositories.sortOrder), asc(repositories.fullName))

      // PRs already linked to an issue are excluded by URL — the issue rows
      // carry them (regardless of the row's possibly-drifted prState).
      const linkedRows = await ctx.db
        .select({ prUrl: issues.prUrl })
        .from(issues)
        .innerJoin(projects, eq(projects.id, issues.projectId))
        .where(
          and(
            eq(projects.workspaceId, input.workspaceId),
            isNotNull(issues.prUrl)
          )
        )
      // Release PRs are excluded the same way (EXP-73) — the Reviews surfaces
      // render them as first-class release rows from the synced releases
      // shape, so the anonymous external bucket must not double-list them.
      const releaseRows = await ctx.db
        .select({ prUrl: releases.prUrl })
        .from(releases)
        .where(
          and(
            eq(releases.workspaceId, input.workspaceId),
            isNotNull(releases.prUrl)
          )
        )
      const linkedUrls = new Set(
        [...linkedRows, ...releaseRows].map((row) => row.prUrl).filter(Boolean)
      )

      const results = await Promise.all(
        repos.map(async (repo) => {
          try {
            const token = await resolveGatedRepoToken(repo)
            const pulls = await listOpenPulls(repo.fullName, token)
            return {
              repositoryId: repo.id,
              fullName: repo.fullName,
              pulls: pulls.filter((pull) => !linkedUrls.has(pull.url)),
            }
          } catch {
            // Unreachable repo (App access revoked, private repo without a
            // token, GitHub hiccup) — the queue shows what it can.
            return { repositoryId: repo.id, fullName: repo.fullName, pulls: [] }
          }
        })
      )

      openPullsCache.set(input.workspaceId, {
        expiresAt: Date.now() + OPEN_PULLS_TTL_MS,
        repos: results,
      })
      return { repos: results }
    }),

  // Member-gated squash-merge for a pull request WITHOUT an issue link (the
  // issue-linked path is issues.mergePr, which also syncs the issue row).
  // Same trust model as installationToken: the workspace's ownership of the
  // repo row plus the installation link-gate authorizes the merge.
  mergePull: authedProcedure
    .input(
      z.object({
        repositoryId: z.string().uuid(),
        prNumber: z.number().int().positive(),
      })
    )
    .mutation(async ({ ctx, input }): Promise<{ merged: true }> => {
      const repo = await loadRepository(input.repositoryId)
      await assertRepoCapability(ctx.session.user.id, repo.workspaceId)
      if (!githubAppConfigured()) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub App is not configured on this instance`,
        })
      }
      const token = await resolveGatedRepoToken(repo)
      if (!token) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The GitHub App no longer has access to ${repo.fullName}. Re-grant it on GitHub (workspace settings → Repositories), then retry.`,
        })
      }

      try {
        await mergePullRequest({
          repo: repo.fullName,
          prNumber: input.prNumber,
          token,
        })
      } catch (err) {
        if (err instanceof GitHubMergeError) {
          if (err.status === 405) {
            throw new TRPCError({
              code: `PRECONDITION_FAILED`,
              message: err.message,
            })
          }
          if (err.status === 409) {
            throw new TRPCError({
              code: `CONFLICT`,
              message: `Head branch changed on GitHub — refresh and try again`,
            })
          }
          if (err.status === 404) {
            throw new TRPCError({
              code: `NOT_FOUND`,
              message: `Pull request not found on GitHub`,
            })
          }
          throw new TRPCError({
            code: `INTERNAL_SERVER_ERROR`,
            message: `GitHub merge failed: ${err.message}`,
          })
        }
        throw err
      }

      openPullsCache.delete(repo.workspaceId)
      return { merged: true }
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

  // Session-gated JIT push token for the native launcher's ambient git
  // credentials (repo-local credential helper — EXP-73; the token no longer
  // rides the remote URL). Never persisted server-side — minted per session
  // and expires. Replaces the deleted companion.repoToken.
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
      // per-repo lookup misses — see resolveRepoInstallationTokenInfo (fixes
      // the spurious 412 when the repo IS covered by a known installation). The
      // fallback token is VERIFIED against the repo, so a null here means the
      // App genuinely lost access (repo removed from the installation's
      // selection, or uninstalled) — stamp the row so the settings UI shows the
      // no-access badge, and tell the caller how to fix it.
      const resolved = await resolveRepoInstallationTokenInfo(repo.fullName, {
        fallbackInstallationId: repo.installationId,
      })
      if (!resolved) {
        await ctx.db
          .update(repositories)
          .set({ inaccessibleAt: new Date() })
          .where(
            and(eq(repositories.id, repo.id), isNull(repositories.inaccessibleAt))
          )
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The GitHub App no longer has access to ${repo.fullName}. Re-grant it on GitHub (workspace settings → Repositories), then retry.`,
        })
      }
      // Link-gate: the installation serving this repo must still be claimed by
      // the repo's workspace — a workspace must not keep minting through a
      // GitHub account it never connected (or disconnected).
      if (
        !(await isInstallationLinkedToWorkspace(
          repo.workspaceId,
          resolved.installationId
        ))
      ) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${repo.fullName} resolves to a GitHub account that isn't connected to this workspace. Reconnect it in workspace settings → Repositories.`,
        })
      }
      const token = resolved.token
      // The mint just proved access — heal a drifted stored installation id and
      // clear a stale no-access flag (best-effort bookkeeping).
      if (
        repo.installationId !== resolved.installationId ||
        repo.inaccessibleAt != null
      ) {
        await ctx.db
          .update(repositories)
          .set({ installationId: resolved.installationId, inaccessibleAt: null })
          .where(eq(repositories.id, repo.id))
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
        // GitHub's REAL expiry for the (possibly cache-served) token — EXP-73:
        // a synthetic now+55min here once labeled a nearly-dead cached token
        // "fresh", and every desktop freshness check trusted the fiction. The
        // desktop schedules its credential refresh from this value.
        expiresAt: new Date(resolved.expiresAt),
      }
    }),
})
