import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNotNull, isNull, sql } from "drizzle-orm"
import type { db } from "@/db/connection"
import { router, authedProcedure } from "@/lib/trpc"
import { issues, boards, repositories, users } from "@/db/schema"
import { assertTeamMember, getIssueTeamContext } from "@/lib/team-membership"
import {
  claimPrMerge,
  releasePrMergeClaim,
} from "@/lib/integrations/pr-actor-claims"
import {
  fetchBranchDiff,
  githubAppConfigured,
  listRepoBranches,
  peekBranchDiff,
  resolveRepoDefaultBranch,
  resolveRepoDefaultBranchCached,
  resolveRepoInstallationTokenInfo,
} from "@/lib/integrations/github-app"
import {
  branchExists,
  diagnoseUnmergeablePr,
  GitHubMergeError,
  listOpenPulls,
  mergePullRequest,
  type OpenPull,
} from "@/lib/integrations/github-pr"
import { isNotMergeable, prMergeFailureError } from "@/lib/trpc/pr-merge-error"
import {
  assertRepoInstallationAccess,
  isInstallationLinkedToTeam,
} from "@/lib/trpc/integrations"

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

// "repository backs N boards" — the CONFLICT message when a delete is blocked
// by a board still pointing at the repo. A trashed board keeps its repo FK
// (restrict) but is hidden from the synced "in use by" chips, so name that case.
export function repoInUseMessage(count: number): string {
  return `Can't remove this repository. It backs ${count} board${count === 1 ? `` : `s`}. Retarget or delete those boards first (a board in the trash may still use it).`
}

const fullNameSchema = z
  .string()
  .min(1)
  .max(255)
  .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`)

// Repo-backed capabilities (list/forIssue/branchDiff reads, JIT token minting)
// reach into the backing GitHub repo. Member-gated: since v7 every membership
// is an explicit invite (the self-service public join is gone), so the old
// moderator-only clamp for self-joined public-team members is obsolete —
// anonymous callers never reach these procedures at all.
async function assertRepoCapability(userId: string, teamId: string) {
  await assertTeamMember(userId, teamId)
}

// EXP-557 per-user sharing: a connected repo is MANAGED (remove, branch pin)
// by the member who shared it or a team owner. Pre-EXP-557 rows carry a NULL
// sharer and fall back to owners only. Exported for tests.
export async function assertRepoManager(
  userId: string,
  repo: { teamId: string; sharedByUserId: string | null }
) {
  const member = await assertTeamMember(userId, repo.teamId)
  if (repo.sharedByUserId === userId || member?.role === `owner`) return
  throw new TRPCError({
    code: `FORBIDDEN`,
    message: `Only the member who shared this repository or a team owner can manage it.`,
  })
}

type Db = typeof db
type Tx = Parameters<Parameters<Db[`transaction`]>[0]>[0]

// The exact `repositories.add` validation + upsert, reusable inside another
// transaction (boards.create's inline connect path). Verifies the repo
// resolves to a GitHub App installation LINKED to the target team — the
// App JWT can reach every installation of the App, so this check (not mere
// installed-ness) is what stops an owner binding an unrelated account's
// private repo to their team. Upserts + un-archives, returns the
// repository id. Owner/admin + plan-cap checks are the caller's responsibility
// (done before opening the tx). The persisted installation id is the
// authoritative one resolved from GitHub, never the client-supplied claim.
// The gate runs on THIS transaction (EXP-371) and leaves the resolved
// installation's link row locked FOR UPDATE, so a concurrent unlink can't drop
// the token path out from under the repository row written below.
export async function connectRepositoryInTx(
  tx: Tx,
  input: {
    userId: string
    teamId: string
    fullName: string
    defaultBranch?: string
    private?: boolean
  }
): Promise<string> {
  // Never blind-seed `main` (L30): when the caller didn't supply a branch, ask
  // GitHub for the authoritative default. Only fall back to `main` when the live
  // lookup yields nothing (App unconfigured / repo gone / transient failure), and
  // log so a wrong-fallback row is traceable.
  // Resolved BEFORE the access gate below on purpose: the gate returns with the
  // installation's link row locked FOR UPDATE, and this lookup is up to two
  // unbounded GitHub round-trips (token mint + repo GET) — a GitHub hang under
  // that lock would stall the team's unlink/claim/connect writers. Pre-lock
  // matches the gate's own GitHub calls, and the lookup doesn't depend on the
  // gate's result (it mints its own repo-scoped token).
  let defaultBranch = input.defaultBranch
  if (!defaultBranch) {
    try {
      defaultBranch =
        (await resolveRepoDefaultBranch(input.fullName)) ?? undefined
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

  const installationId = await assertRepoInstallationAccess(
    tx,
    input.teamId,
    input.userId,
    input.fullName
  )

  const [inserted] = await tx
    .insert(repositories)
    .values({
      teamId: input.teamId,
      fullName: input.fullName,
      defaultBranch,
      private: input.private ?? false,
      installationId,
      inaccessibleAt: null,
      // EXP-557: the connector becomes the repo's sharer (manages it
      // alongside owners).
      sharedByUserId: input.userId,
    })
    .onConflictDoNothing({
      target: [repositories.teamId, repositories.fullName],
    })
    .returning({ id: repositories.id })
  if (inserted) return inserted.id

  // Already registered — un-archive, refresh the installation binding, clear
  // any stale no-access flag (a re-connect just proved access), and return the
  // existing row. The sharer only changes when the row was archived or never
  // had one (EXP-557): re-adding a live repo must not quietly transfer its
  // manage rights away from the original sharer. SET expressions read the OLD
  // row, so referencing archived_at next to its own reset is sound.
  const [existing] = await tx
    .update(repositories)
    .set({
      archivedAt: null,
      installationId,
      inaccessibleAt: null,
      sharedByUserId: sql`case when ${repositories.archivedAt} is not null or ${repositories.sharedByUserId} is null then ${input.userId} else ${repositories.sharedByUserId} end`,
    })
    .where(
      and(
        eq(repositories.teamId, input.teamId),
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
      message: `Repository was removed concurrently. Retry.`,
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
  },
>(
  repos: R[],
  persist: (
    id: string,
    patch: { defaultBranch?: string; clearInaccessible?: boolean }
  ) => Promise<void>,
  resolve: (
    fullName: string
  ) => Promise<string | null> = resolveRepoDefaultBranchCached
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

// The branch the product treats as "the default branch" (EXP-462): the
// team-chosen override when set, GitHub's healed default otherwise. Every
// payload named `defaultBranch` that a consumer acts on (worktree base, PR
// base, trunk reset, diffs) carries THIS value — the raw GitHub column only
// surfaces as `githubDefaultBranch` in the settings payloads.
export function effectiveDefaultBranch(repo: {
  defaultBranch: string
  defaultBranchOverride: string | null
}): string {
  return repo.defaultBranchOverride ?? repo.defaultBranch
}

// The override for a (team, "owner/name") pair, or null when the team follows
// GitHub (or never connected the repo — a PR-URL-derived repo may not have a
// row). For the paths that resolve the default branch live from GitHub
// (retargetPr, prepareConflictFix, stacked-PR healing) instead of loading the
// repo row.
export async function repoBranchOverride(
  teamId: string,
  fullName: string
): Promise<string | null> {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select({ defaultBranchOverride: repositories.defaultBranchOverride })
    .from(repositories)
    .where(
      and(
        eq(repositories.teamId, teamId),
        eq(repositories.fullName, fullName),
        isNull(repositories.archivedAt)
      )
    )
    .limit(1)
  return row?.defaultBranchOverride ?? null
}

// Board → repo resolution (v4): a board is backed by exactly one repo via
// `boards.repositoryId`. Returns null only for dangling data (archived repo).
// Shared by repositories.forIssue and steer.startSession's precondition.
// `defaultBranch` is the EFFECTIVE branch (override-aware).
export async function resolveBoardRepository(boardId: string) {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select({
      repositoryId: repositories.id,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
      installationId: repositories.installationId,
    })
    .from(boards)
    .innerJoin(repositories, eq(repositories.id, boards.repositoryId))
    .where(and(eq(boards.id, boardId), isNull(repositories.archivedAt)))
    .limit(1)
  if (!row) return null
  const { defaultBranchOverride, ...rest } = row
  return { ...rest, defaultBranch: effectiveDefaultBranch(row) }
}

async function loadRepository(repositoryId: string) {
  const { db } = await import(`@/db/connection`)
  const [repo] = await db
    .select({
      id: repositories.id,
      teamId: repositories.teamId,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
      installationId: repositories.installationId,
      inaccessibleAt: repositories.inaccessibleAt,
      sharedByUserId: repositories.sharedByUserId,
      archivedAt: repositories.archivedAt,
    })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  if (!repo) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Repository not found` })
  }
  return repo
}

// EXP-626: the repo-by-id lookup the issue-LESS `exponential_pr_open` path
// needs — the same row `mergePull` loads, minus archived repos (a chore PR
// must never be opened against a repo the team retired), with the EFFECTIVE
// default branch already resolved so the caller can use it as the PR base.
// Authorization stays with the caller (membership + scope + link-gate); this
// only resolves the row.
export async function loadRepositoryForTeam(repositoryId: string): Promise<{
  id: string
  teamId: string
  fullName: string
  defaultBranch: string
}> {
  const repo = await loadRepository(repositoryId)
  if (repo.archivedAt) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `${repo.fullName} is archived for this team. Restore it in team settings → Repositories.`,
    })
  }
  return {
    id: repo.id,
    teamId: repo.teamId,
    fullName: repo.fullName,
    defaultBranch: effectiveDefaultBranch(repo),
  }
}

// The Reviews queue's "everything else" source: open PRs listed live from
// GitHub for every team repo. Short in-process cache so tab switches and
// re-mounts don't hammer the GitHub API; busted by mergePull.
const OPEN_PULLS_TTL_MS = 60_000
interface CachedOpenPulls {
  expiresAt: number
  repos: Array<{ repositoryId: string; fullName: string; pulls: OpenPull[] }>
}
const openPullsCache = new Map<string, CachedOpenPulls>()

// Resolve the App installation token for a repo row, honoring the same
// link-gate as installationToken: a token is only used when the installation
// serving the repo is still claimed by the repo's team. Returns null when
// no gated token is available (callers may still read public repos
// unauthenticated / via GITHUB_TOKEN).
async function resolveGatedRepoToken(repo: {
  teamId: string
  fullName: string
  installationId: number | null
}): Promise<string | null> {
  if (!githubAppConfigured()) return null
  const resolved = await resolveRepoInstallationTokenInfo(repo.fullName, {
    fallbackInstallationId: repo.installationId,
  })
  if (!resolved) return null
  if (
    !(await isInstallationLinkedToTeam(repo.teamId, resolved.installationId))
  ) {
    return null
  }
  return resolved.token
}

export const repositoriesRouter = router({
  // Member-readable: the team's repos + the boards each one backs (for the
  // settings "in use by" chips and mobile pickers).
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertRepoCapability(ctx.session.user.id, input.teamId)

      const rawRepos = await ctx.db
        .select()
        .from(repositories)
        .where(
          and(
            eq(repositories.teamId, input.teamId),
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
            ...(patch.defaultBranch
              ? { defaultBranch: patch.defaultBranch }
              : {}),
            ...(patch.clearInaccessible ? { inaccessibleAt: null } : {}),
          })
          .where(eq(repositories.id, id))
          .then(() => {})
      )

      // Boards that point at these repos, computed from boards.repositoryId.
      const boardRows = await ctx.db
        .select({
          id: boards.id,
          name: boards.name,
          slug: boards.slug,
          repositoryId: boards.repositoryId,
        })
        .from(boards)
        .where(eq(boards.teamId, input.teamId))

      // The sharers' display identity for the ×4 "Shared by" rows (EXP-557).
      const sharerIds = [
        ...new Set(
          repos
            .map((repo) => repo.sharedByUserId)
            .filter((id): id is string => Boolean(id))
        ),
      ]
      const sharerRows =
        sharerIds.length > 0
          ? await ctx.db
              .select({
                id: users.id,
                name: users.name,
                email: users.email,
              })
              .from(users)
              .where(inArray(users.id, sharerIds))
          : []
      const sharerById = new Map(sharerRows.map((row) => [row.id, row]))

      // `defaultBranch` = the effective branch every consumer should act on
      // (old desktop builds included); the raw healed GitHub value rides along
      // as `githubDefaultBranch` for the settings picker.
      return repos.map((repo) => ({
        ...repo,
        defaultBranch: effectiveDefaultBranch(repo),
        githubDefaultBranch: repo.defaultBranch,
        sharedBy: repo.sharedByUserId
          ? (sharerById.get(repo.sharedByUserId) ?? null)
          : null,
        boards: boardRows
          .filter((p) => p.repositoryId === repo.id)
          .map((p) => ({ id: p.id, name: p.name, slug: p.slug })),
      }))
    }),

  // Member-readable: every open pull request across the team's repos
  // that is NOT already linked to an issue (those rows render from the synced
  // issues shape). Listed live from GitHub so PRs opened outside the issue
  // flow — PRs on other branches, manual PRs, external contributors —
  // still land in the Reviews queue.
  openPulls: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertRepoCapability(ctx.session.user.id, input.teamId)

      const cached = openPullsCache.get(input.teamId)
      if (cached && cached.expiresAt > Date.now()) {
        return { repos: cached.repos }
      }

      const repos = await ctx.db
        .select({
          id: repositories.id,
          fullName: repositories.fullName,
          teamId: repositories.teamId,
          installationId: repositories.installationId,
        })
        .from(repositories)
        .where(
          and(
            eq(repositories.teamId, input.teamId),
            isNull(repositories.archivedAt)
          )
        )
        .orderBy(asc(repositories.sortOrder), asc(repositories.fullName))

      // PRs already linked to an issue are excluded by URL — the issue rows
      // carry them (regardless of the row's possibly-drifted prState).
      const linkedRows = await ctx.db
        .select({ prUrl: issues.prUrl })
        .from(issues)
        .innerJoin(boards, eq(boards.id, issues.boardId))
        .where(and(eq(boards.teamId, input.teamId), isNotNull(issues.prUrl)))
      const linkedUrls = new Set(
        linkedRows.map((row) => row.prUrl).filter(Boolean)
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

      openPullsCache.set(input.teamId, {
        expiresAt: Date.now() + OPEN_PULLS_TTL_MS,
        repos: results,
      })
      return { repos: results }
    }),

  // Member-gated squash-merge for a pull request WITHOUT an issue link (the
  // issue-linked path is issues.mergePr, which also syncs the issue row).
  // Same trust model as installationToken: the team's ownership of the
  // repo row plus the installation link-gate authorizes the merge.
  mergePull: authedProcedure
    .input(
      z.object({
        repositoryId: z.string().uuid(),
        prNumber: z.number().int().positive(),
        // EXP-711: see issues.mergePr — rides the merge claim to the webhook's
        // branch-keyed sweep, the only merge-driven end a chore PR has.
        endSessions: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }): Promise<{ merged: true }> => {
      const repo = await loadRepository(input.repositoryId)
      await assertRepoCapability(ctx.session.user.id, repo.teamId)
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
          message: `The GitHub App no longer has access to ${repo.fullName}. Re-grant it on GitHub (team settings → Repositories), then retry.`,
        })
      }

      // EXP-494: even an "unlinked" PR's `closed` webhook can resolve an
      // issue via the branch parse — claim the initiator so that fan-out is
      // attributed to (and excludes) the merging member instead of going out
      // anonymously.
      claimPrMerge(repo.fullName, input.prNumber, {
        userId: ctx.session.user.id,
        // EXP-626: reachable from the MCP `exponential_pr_merge` tool now
        // (a chore PR with no issue), so the claim records agent-mediation
        // the same way issues.mergePr does.
        viaAgent: ctx.viaMcp === true,
        endSessions: input.endSessions,
      })
      try {
        await mergePullRequest({
          repo: repo.fullName,
          prNumber: input.prNumber,
          token,
        })
      } catch (err) {
        releasePrMergeClaim(repo.fullName, input.prNumber)
        if (err instanceof GitHubMergeError) {
          // "Not mergeable" is misleading on a stacked PR whose base is stale
          // (EXP-324) — diagnose the base's real state; degrade to GitHub's
          // message when the diagnosis can't run (same treatment as
          // issues.mergePr, same conflict signal, EXP-533).
          let diagnosis = null
          if (isNotMergeable(err)) {
            const defaultBranch =
              effectiveDefaultBranch(repo) ??
              (await resolveRepoDefaultBranchCached(repo.fullName))
            diagnosis = defaultBranch
              ? await diagnoseUnmergeablePr({
                  repo: repo.fullName,
                  prNumber: input.prNumber,
                  token,
                  defaultBranch,
                })
              : null
          }
          throw prMergeFailureError(err, diagnosis)
        }
        throw err
      }

      openPullsCache.delete(repo.teamId)
      return { merged: true }
    }),

  // Any member: register a repo THEIR OWN GitHub connection grants (EXP-557 —
  // connecting shares it with the team; the actor becomes its sharer). The
  // installation id is resolved server-side from GitHub — clients never
  // supply it.
  add: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        fullName: fullNameSchema,
        defaultBranch: z.string().min(1).max(255).optional(),
        private: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)

      // The install-check + upsert + un-archive sequence is connectRepositoryInTx
      // (shared with boards.create's inline connect) — call it, then load the
      // full row to hand back.
      const repository = await ctx.db.transaction(async (tx) => {
        const repositoryId = await connectRepositoryInTx(tx, {
          userId: ctx.session.user.id,
          teamId: input.teamId,
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

  // Sharer or owner: hard-delete. Blocked (CONFLICT) while any board still
  // points at the repo — the `boards.repository_id` FK is `restrict`.
  remove: authedProcedure
    .input(z.object({ repositoryId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      await assertRepoManager(ctx.session.user.id, repo)
      try {
        await ctx.db
          .delete(repositories)
          .where(eq(repositories.id, input.repositoryId))
      } catch (err) {
        if (isForeignKeyViolation(err)) {
          const backing = await ctx.db
            .select({ id: boards.id })
            .from(boards)
            .where(eq(boards.repositoryId, input.repositoryId))
          throw new TRPCError({
            code: `CONFLICT`,
            message: repoInUseMessage(backing.length),
          })
        }
        throw err
      }
      return { ok: true as const }
    }),

  // Any member: the repo's branch names for the default-branch picker
  // (EXP-462). Listed live from GitHub — never persisted. A read on a shared
  // repo, so plain membership (the PIN itself stays sharer/owner).
  listBranches: authedProcedure
    .input(z.object({ repositoryId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      await assertRepoCapability(ctx.session.user.id, repo.teamId)
      const token = await resolveGatedRepoToken(repo)
      let branches: string[]
      try {
        branches = await listRepoBranches(repo.fullName, token)
      } catch {
        throw new TRPCError({
          code: `BAD_GATEWAY`,
          message: `Could not list branches of ${repo.fullName} from GitHub`,
        })
      }
      return { branches }
    }),

  // Sharer or owner: pin the branch the product treats as the repo's default
  // (EXP-462) — PR base, worktree base, trunk sync, diffs. `branch: null`
  // clears the pin (follow GitHub again), and picking GitHub's current
  // default normalizes to null so such a repo keeps tracking GitHub renames.
  // The GitHub-truth `defaultBranch` column keeps being healed underneath.
  setDefaultBranch: authedProcedure
    .input(
      z.object({
        repositoryId: z.string().uuid(),
        branch: z.string().min(1).max(255).nullable(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const repo = await loadRepository(input.repositoryId)
      await assertRepoManager(ctx.session.user.id, repo)

      let override = input.branch
      let live: string | null = null
      if (override != null) {
        // Validate against GitHub so a typo can't brick every worktree/PR
        // base. Verified with the same gated token the reads use; an
        // unreachable repo surfaces as a real error, not a silent save.
        const token = await resolveGatedRepoToken(repo)
        let exists: boolean
        try {
          exists = await branchExists(repo.fullName, override, token)
        } catch {
          throw new TRPCError({
            code: `BAD_GATEWAY`,
            message: `Could not verify '${override}' on ${repo.fullName} — GitHub is unreachable`,
          })
        }
        if (!exists) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `'${override}' is not a branch of ${repo.fullName}`,
          })
        }
        // Normalize against GitHub's LIVE default, not the possibly-stale
        // stored column (EXP-466): right after a GitHub default-branch
        // rename, pinning the stale-stored name must store the pin, not
        // silently clear it. Unresolvable (App can't reach the repo right
        // now) degrades to the stored value.
        live = await resolveRepoDefaultBranchCached(repo.fullName)
        if (override === (live ?? repo.defaultBranch)) override = null
      }

      // Heal the GitHub-truth column in the same write when the live lookup
      // disagreed, so the returned `githubDefaultBranch` is fresh (same
      // spirit as healRepoDefaultBranches).
      const [updated] = await ctx.db
        .update(repositories)
        .set({
          defaultBranchOverride: override,
          ...(live && live !== repo.defaultBranch
            ? { defaultBranch: live }
            : {}),
        })
        .where(eq(repositories.id, repo.id))
        .returning()
      if (!updated) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Repository was removed concurrently`,
        })
      }
      return {
        repository: {
          ...updated,
          defaultBranch: effectiveDefaultBranch(updated),
          githubDefaultBranch: updated.defaultBranch,
        },
      }
    }),

  // The launcher's clone-target resolution: issue → board → repositoryId.
  // Member-readable.
  forIssue: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const issueCtx = await getIssueTeamContext(input.issueId)
      await assertRepoCapability(ctx.session.user.id, issueCtx.teamId)
      return resolveBoardRepository(issueCtx.boardId)
    }),

  // Member-gated middle tier of remote Changes visibility (§4.8, L18): the
  // issue's `exp/<IDENTIFIER>` branch compared against the repo default branch,
  // returned in the shared `prFiles` shape. Null when the branch was never
  // pushed (GitHub 404). ~60s per-branch cache lives in github-app.fetchBranchDiff.
  branchDiff: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const issueCtx = await getIssueTeamContext(input.issueId)
      await assertRepoCapability(ctx.session.user.id, issueCtx.teamId)

      const [issue] = await ctx.db
        .select({ identifier: issues.identifier })
        .from(issues)
        .where(eq(issues.id, input.issueId))
        .limit(1)
      if (!issue?.identifier) return null

      const repo = await resolveBoardRepository(issueCtx.boardId)
      if (!repo) return null

      const branch = issueBranchName(issue.identifier)
      // Warm-cache hit short-circuits BEFORE the token/installation lookups
      // (both uncached GitHub round-trips) — fetchBranchDiff would peek the same
      // cache, but only after resolveRepoInstallationTokenInfo already paid for a
      // /repos/{repo}/installation call.
      const cached = peekBranchDiff(repo.fullName, repo.defaultBranch, branch)
      if (cached) return cached

      const resolved = await resolveRepoInstallationTokenInfo(repo.fullName, {
        fallbackInstallationId: repo.installationId,
      })
      // Link-gate (mirrors issues.prFiles): the installation serving this repo
      // must still be claimed by the issue's team — a deliberately severed
      // GitHub connection must not keep exposing private-repo branch diffs. An
      // unresolved installation degrades to an unauthenticated public-repo read.
      if (
        resolved &&
        !(await isInstallationLinkedToTeam(
          issueCtx.teamId,
          resolved.installationId
        ))
      ) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${repo.fullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`,
        })
      }
      return fetchBranchDiff({
        repo: repo.fullName,
        base: repo.defaultBranch,
        branch,
        token: resolved?.token ?? null,
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
      // Team coding: any member of the repo's team may mint a JIT token
      // (assertRepoCapability = plain team membership).
      // Per-installer attribution is intentionally NOT required here: the repo
      // is only present in this team because a member legitimately
      // connected it, and connectRepositoryInTx already enforced
      // assertRepoInstallationAccess at connect time. The team's ownership
      // of the repo row is the authorization; requiring the caller to also be
      // the original installer would break coding for every other teammate.
      await assertRepoCapability(ctx.session.user.id, repo.teamId)
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
            and(
              eq(repositories.id, repo.id),
              isNull(repositories.inaccessibleAt)
            )
          )
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The GitHub App no longer has access to ${repo.fullName}. Re-grant it on GitHub (team settings → Repositories), then retry.`,
        })
      }
      // Link-gate: the installation serving this repo must still be claimed by
      // the repo's team — a team must not keep minting through a
      // GitHub account it never connected (or disconnected).
      if (
        !(await isInstallationLinkedToTeam(
          repo.teamId,
          resolved.installationId
        ))
      ) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${repo.fullName} resolves to a GitHub account that isn't connected to this team. Reconnect it in team settings → Repositories.`,
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
          .set({
            installationId: resolved.installationId,
            inaccessibleAt: null,
          })
          .where(eq(repositories.id, repo.id))
      }
      // GitHub is authoritative on the default branch — a stale/misseeded row
      // (e.g. `main` for a `master` repo) would break the launcher's
      // `git worktree add … origin/<default>`. Prefer the live value; heal the
      // row when it drifted; fall back to the stored value if the lookup fails.
      // A team-chosen override (EXP-462) wins over both — the launcher must
      // base worktrees on the branch the team actually develops on.
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
        defaultBranch:
          repo.defaultBranchOverride ?? liveDefaultBranch ?? repo.defaultBranch,
        // GitHub's REAL expiry for the (possibly cache-served) token — EXP-73:
        // a synthetic now+55min here once labeled a nearly-dead cached token
        // "fresh", and every desktop freshness check trusted the fiction. The
        // desktop schedules its credential refresh from this value.
        expiresAt: new Date(resolved.expiresAt),
      }
    }),
})
