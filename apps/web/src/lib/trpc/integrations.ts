import { and, eq, inArray, isNull } from "drizzle-orm"
import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import {
  githubInstallationLinks,
  githubInstallationRepoGrants,
  githubInstallations,
  repositories,
} from "@/db/schema"
import { isUserAdmin } from "@/lib/admin"
import { assertWorkspaceMember } from "@/lib/workspace-membership"
import { TRPCError } from "@trpc/server"
import {
  githubAppConfigured,
  githubAppInstallUrl,
  githubOAuthAuthorizeUrl,
  githubOAuthConfigured,
  installationIdForRepo,
  installationManageUrl,
  listAllInstallationRepos,
  type InstallationRepo,
} from "@/lib/integrations/github-app"
import {
  mintGithubSetupState,
  readGithubClaimTicket,
} from "@/lib/integrations/github-setup-state"
import { getFeedbackWorkspaceId } from "@/lib/bootstrap-cloud"

// Repo management (connect/remove/claim/unlink) is owner-or-instance-admin,
// mirroring member management. Lives here (not repositories.ts) because both
// routers need it and repositories.ts already imports from this module —
// repositories.ts re-exports it for its existing callers.
export async function assertCanManageRepos(userId: string, workspaceId: string) {
  if (await isUserAdmin(userId)) return
  await assertWorkspaceMember(userId, workspaceId, [`owner`])
}

// The GitHub App INSTALL page URL (new install / grant more repos). Also the
// claim fallback when the App has no OAuth client secret configured: the
// signed state carries the target workspace, and the setup redirect links the
// installation to it after the round-trip. `dialog: true` lands the redirect
// on the self-closing /integrations/github/installed page; `mobile: true`
// serves the exponential://github-connected deep-link page instead.
function installUrlFor(
  userId: string,
  workspaceId: string,
  opts?: { mobile?: boolean }
): string | null {
  return githubAppInstallUrl(
    mintGithubSetupState(userId, {
      dialog: true,
      mobile: opts?.mobile,
      workspaceId,
    })
  )
}

// The OAuth claim URL — the mobile-friendly primary connect path: a single
// authorize screen (instant auto-redirect on re-auth), then the callback
// enumerates the user's installations and links them to the workspace without
// ever visiting GitHub's configure page. Null when the App's OAuth client
// secret isn't configured (self-hosted fallback = installUrl).
function connectUrlFor(
  userId: string,
  workspaceId: string,
  opts?: { mobile?: boolean }
): string | null {
  return githubOAuthAuthorizeUrl(
    mintGithubSetupState(userId, {
      dialog: true,
      mobile: opts?.mobile,
      workspaceId,
      oauth: true,
    })
  )
}

interface ResolvedInstallation {
  installationId: number
  accountLogin: string | null
  accountType: string | null
}

// The installations a workspace may browse/connect: exactly its claimed links.
// No admin bypass, no unattributed fallback — an unlinked installation is
// invisible to every picker (the old "admins see all ownerless installs" rule
// leaked one account's repos into unrelated contexts).
async function resolveWorkspaceInstallations(
  workspaceId: string
): Promise<ResolvedInstallation[]> {
  return db
    .select({
      installationId: githubInstallations.installationId,
      accountLogin: githubInstallations.accountLogin,
      accountType: githubInstallations.accountType,
    })
    .from(githubInstallationLinks)
    .innerJoin(
      githubInstallations,
      eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
    )
    .where(eq(githubInstallationLinks.workspaceId, workspaceId))
}

// --- Repo grants (the user-scoped access boundary) --------------------------
// A workspace ↔ installation LINK is installation-granular, but GitHub
// attributes an installation to a user who can access even ONE of its repos.
// github_installation_repo_grants (captured at OAuth-callback time from
// `GET /user/installations/{id}/repositories`) records what the connecting
// user could actually access; when the App's OAuth secret is configured, repo
// DISCOVERY (the `repos` query) and CONNECT (assertRepoInstallationAccess)
// are confined to granted repos. Without the OAuth secret there is no
// user-scoped capture path at all (single-tenant/trusted self-host), so the
// installation-wide behavior stays — exactly mirroring setup.ts's
// githubOAuthConfigured() split. Token minting is deliberately NOT grant-gated
// (already-connected repos keep working; the gate is for discovery/connect).

// Every granted (installationId, fullName, …) for a workspace, restricted to
// the given linked installation ids. Any member's grant counts — entitlement
// is the union across members.
async function workspaceGrantRows(
  workspaceId: string,
  installationIds: number[]
): Promise<
  Array<{
    installationId: number
    fullName: string
    private: boolean
    defaultBranch: string | null
  }>
> {
  if (installationIds.length === 0) return []
  return db
    .select({
      installationId: githubInstallationRepoGrants.installationId,
      fullName: githubInstallationRepoGrants.fullName,
      private: githubInstallationRepoGrants.private,
      defaultBranch: githubInstallationRepoGrants.defaultBranch,
    })
    .from(githubInstallationRepoGrants)
    .where(
      and(
        eq(githubInstallationRepoGrants.workspaceId, workspaceId),
        inArray(githubInstallationRepoGrants.installationId, installationIds)
      )
    )
}

// The connect-time grant gate. No-op when the OAuth secret isn't configured
// (no capture path exists — trusted single-tenant fallback).
async function assertRepoGrant(
  workspaceId: string,
  installationId: number,
  fullName: string
): Promise<void> {
  if (!githubOAuthConfigured()) return
  const [row] = await db
    .select({ id: githubInstallationRepoGrants.id })
    .from(githubInstallationRepoGrants)
    .where(
      and(
        eq(githubInstallationRepoGrants.workspaceId, workspaceId),
        eq(githubInstallationRepoGrants.installationId, installationId),
        eq(githubInstallationRepoGrants.fullName, fullName)
      )
    )
    .limit(1)
  if (row) return
  throw new TRPCError({
    code: `FORBIDDEN`,
    message: `You don't have access to ${fullName} on GitHub, or your connection is stale — reconnect GitHub in workspace settings → Repositories to refresh which repositories you can access.`,
  })
}

// Short-lived in-process cache of a workspace's installable repos so
// re-opening the project dialog doesn't hammer GitHub (and its secondary rate
// limits). Keyed per workspace.
const REPOS_TTL_MS = 60_000
interface CachedRepos {
  repos: InstallationRepo[]
  hasMore: boolean
  installations: Array<
    ResolvedInstallation & { hasMore: boolean; needsReauth: boolean }
  >
  expiresAt: number
}
const repoCache = new Map<string, CachedRepos>()

// Drop a workspace's cached repo list so the next `repos` query re-hits
// GitHub. Called after a claim/link lands or when the UI asks for a forced
// refresh, both of which mean the installable set just changed.
export function invalidateRepoCache(workspaceId: string): void {
  repoCache.delete(workspaceId)
}

// Installation-wide invalidation (webhooks: repos granted/removed, install
// suspended): drop every linked workspace's entry.
export async function invalidateRepoCacheForInstallation(
  installationId: number
): Promise<void> {
  const linked = await db
    .select({ workspaceId: githubInstallationLinks.workspaceId })
    .from(githubInstallationLinks)
    .innerJoin(
      githubInstallations,
      eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
    )
    .where(eq(githubInstallations.installationId, installationId))
  for (const row of linked) invalidateRepoCache(row.workspaceId)
}

// Connect-path authorization: connecting a repo (repositories.add /
// projects.create inline) must resolve to an installation LINKED to the target
// workspace — the App JWT itself can reach every installation of the App, so
// without this check any owner who knows a repo's full name could bind an
// unrelated account's private repo to their workspace. Returns the
// authoritative installation id so callers persist that instead of trusting
// the client-supplied one. When GitHub's per-repo lookup 404s (it's flaky when
// the App spans several accounts), fall back to scanning the workspace's
// linked installations' repo lists — bounded, connect-time only.
// On OAuth-configured instances the resolved installation must ALSO carry a
// user-scoped grant for this exact repo (assertRepoGrant) — the link alone is
// installation-granular, and a single-repo collaborator must not connect the
// rest of the account's repos.
export async function assertRepoInstallationAccess(
  workspaceId: string,
  fullName: string
): Promise<number> {
  const installs = await resolveWorkspaceInstallations(workspaceId)
  if (installs.length === 0) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `No GitHub account is connected to this workspace. Connect one in workspace settings → Repositories, then try again.`,
    })
  }
  const repoInstallationId = await installationIdForRepo(fullName)
  if (repoInstallationId != null) {
    if (!installs.some((i) => i.installationId === repoInstallationId)) {
      throw new TRPCError({
        code: `FORBIDDEN`,
        message: `${fullName} belongs to a GitHub App installation that isn't connected to this workspace. Connect that GitHub account in workspace settings → Repositories first.`,
      })
    }
    // The link alone is installation-granular; the grant (captured user-scoped
    // at OAuth time) proves a member can actually access THIS repo.
    await assertRepoGrant(workspaceId, repoInstallationId, fullName)
    return repoInstallationId
  }
  for (const inst of installs) {
    // On GitHub a full_name maps to exactly one repo (and so one installation
    // of this App) — a scan hit is authoritative; gate it and stop. The grant
    // check runs OUTSIDE the try so its FORBIDDEN is never swallowed.
    let found = false
    try {
      const { repos } = await listAllInstallationRepos(inst.installationId)
      found = repos.some((r) => r.fullName === fullName)
    } catch {
      // A revoked/suspended installation must not fail the whole scan.
    }
    if (found) {
      await assertRepoGrant(workspaceId, inst.installationId, fullName)
      return inst.installationId
    }
  }
  throw new TRPCError({
    code: `PRECONDITION_FAILED`,
    message: `The Exponential GitHub App has no access to ${fullName}. Grant it on GitHub (workspace settings → Repositories → Manage), then try again.`,
  })
}

// Token-mint gate: is this installation claimed by the repo's workspace?
// (repositories.installationToken re-checks the link at mint time so a repo
// row can't keep minting through an installation the workspace disconnected.)
export async function isInstallationLinkedToWorkspace(
  workspaceId: string,
  installationId: number
): Promise<boolean> {
  const [row] = await db
    .select({ id: githubInstallationLinks.id })
    .from(githubInstallationLinks)
    .innerJoin(
      githubInstallations,
      eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
    )
    .where(
      and(
        eq(githubInstallationLinks.workspaceId, workspaceId),
        eq(githubInstallations.installationId, installationId)
      )
    )
    .limit(1)
  return Boolean(row)
}

function installationSummary(inst: ResolvedInstallation) {
  return {
    installationId: inst.installationId,
    accountLogin: inst.accountLogin,
    accountType: inst.accountType,
    manageUrl: installationManageUrl(inst),
  }
}

export const integrationsRouter = router({
  github: router({
    // GitHub connection state for a workspace (drives the settings section and
    // the pickers' empty state). Member-gated. Token resolution is
    // storage-free (the App JWT looks up a repo's installation on demand);
    // this only reflects what's linked.
    status: authedProcedure
      .input(z.object({ workspaceId: z.string().uuid() }))
      .query(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const { workspaceId } = input
        await assertWorkspaceMember(userId, workspaceId)
        if (!githubAppConfigured()) {
          return {
            configured: false as const,
            installed: false,
            installUrl: null as string | null,
            connectUrl: null as string | null,
            accounts: [] as string[],
            installations: [] as Array<
              ReturnType<typeof installationSummary> & { needsReauth: boolean }
            >,
          }
        }
        const installs = await resolveWorkspaceInstallations(workspaceId)
        // Additive UX signal: a linked installation with ZERO grants for this
        // workspace (e.g. linked before grants existed) yields no repos and
        // refuses connects until a member re-runs the OAuth connect flow —
        // surface that as `needsReauth` so the settings UI can prompt.
        const grantedIds = githubOAuthConfigured()
          ? new Set(
              (
                await workspaceGrantRows(
                  workspaceId,
                  installs.map((i) => i.installationId)
                )
              ).map((g) => g.installationId)
            )
          : null
        return {
          configured: true as const,
          installed: installs.length > 0,
          installUrl: installUrlFor(userId, workspaceId),
          connectUrl: connectUrlFor(userId, workspaceId),
          // Login-only convenience mirror of `installations`.
          accounts: installs
            .map((r) => r.accountLogin)
            .filter((a): a is string => Boolean(a)),
          installations: installs.map((inst) => ({
            ...installationSummary(inst),
            needsReauth: grantedIds
              ? !grantedIds.has(inst.installationId)
              : false,
          })),
        }
      }),

    // Repos the workspace can connect, aggregated across its linked
    // installations and deduped. Backs the repo pickers. `hasMore` signals the
    // per-installation page cap truncated the set so the UI can point at
    // "manage repos on GitHub". `refresh` bypasses the cache so returning from
    // a GitHub hop (new repos granted) reflects immediately. `platform:
    // "mobile"` (native clients only) marks the minted URLs' state so the
    // callbacks deep-link back into the app; web callers omit it.
    repos: authedProcedure
      .input(
        z.object({
          workspaceId: z.string().uuid(),
          refresh: z.boolean().optional(),
          platform: z.enum([`web`, `mobile`]).optional(),
        })
      )
      .query(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const { workspaceId } = input
        const mobile = input.platform === `mobile`
        await assertWorkspaceMember(userId, workspaceId)
        if (input.refresh) invalidateRepoCache(workspaceId)
        if (!githubAppConfigured()) {
          return {
            configured: false as const,
            installed: false,
            installUrl: null as string | null,
            connectUrl: null as string | null,
            repos: [] as InstallationRepo[],
            hasMore: false,
            installations: [] as Array<
              ReturnType<typeof installationSummary> & {
                hasMore: boolean
                needsReauth: boolean
              }
            >,
          }
        }

        const installs = await resolveWorkspaceInstallations(workspaceId)
        const urls = {
          installUrl: installUrlFor(userId, workspaceId, { mobile }),
          connectUrl: connectUrlFor(userId, workspaceId, { mobile }),
        }

        if (installs.length === 0) {
          return {
            configured: true as const,
            installed: false,
            ...urls,
            repos: [] as InstallationRepo[],
            hasMore: false,
            installations: [] as Array<
              ReturnType<typeof installationSummary> & {
                hasMore: boolean
                needsReauth: boolean
              }
            >,
          }
        }

        const cached = repoCache.get(workspaceId)
        if (cached && cached.expiresAt > Date.now()) {
          return {
            configured: true as const,
            installed: true,
            ...urls,
            repos: cached.repos,
            hasMore: cached.hasMore,
            installations: cached.installations.map((inst) => ({
              ...installationSummary(inst),
              hasMore: inst.hasMore,
              needsReauth: inst.needsReauth,
            })),
          }
        }

        const seen = new Set<string>()
        const merged: InstallationRepo[] = []
        let hasMore = false
        const withMeta: Array<
          ResolvedInstallation & { hasMore: boolean; needsReauth: boolean }
        > = []
        if (githubOAuthConfigured()) {
          // Grant path (OAuth configured): the pickers list exactly the repos
          // some member proved USER-SCOPED access to at OAuth time — never the
          // installation-wide selection (which leaks every repo of an account
          // to a single-repo collaborator), and with zero GitHub round-trips.
          // The grant snapshot is bounded (capture pages are capped), so
          // hasMore is always false here; re-running the connect flow is the
          // refresh. A linked installation with no grants at all needs exactly
          // that — surfaced as `needsReauth`.
          const grants = await workspaceGrantRows(
            workspaceId,
            installs.map((i) => i.installationId)
          )
          const grantedIds = new Set(grants.map((g) => g.installationId))
          for (const grant of grants) {
            if (seen.has(grant.fullName)) continue
            seen.add(grant.fullName)
            merged.push({
              fullName: grant.fullName,
              private: grant.private,
              defaultBranch: grant.defaultBranch ?? `main`,
              installationId: grant.installationId,
            })
          }
          for (const inst of installs) {
            withMeta.push({
              ...inst,
              hasMore: false,
              needsReauth: !grantedIds.has(inst.installationId),
            })
          }
        } else {
          // No OAuth secret ⇒ no user-scoped capture path exists (trusted
          // single-tenant self-host) — keep the installation-wide listing.
          for (const inst of installs) {
            let instHasMore = false
            try {
              const { repos, hasMore: more } = await listAllInstallationRepos(
                inst.installationId
              )
              instHasMore = more
              if (more) hasMore = true
              for (const repo of repos) {
                if (seen.has(repo.fullName)) continue
                seen.add(repo.fullName)
                merged.push(repo)
              }
            } catch {
              // A single revoked/404 installation must not fail the whole list.
            }
            withMeta.push({ ...inst, hasMore: instHasMore, needsReauth: false })
          }
        }
        merged.sort((a, b) => a.fullName.localeCompare(b.fullName))
        repoCache.set(workspaceId, {
          repos: merged,
          hasMore,
          installations: withMeta,
          expiresAt: Date.now() + REPOS_TTL_MS,
        })

        return {
          configured: true as const,
          installed: true,
          ...urls,
          repos: merged,
          hasMore,
          installations: withMeta.map((inst) => ({
            ...installationSummary(inst),
            hasMore: inst.hasMore,
            needsReauth: inst.needsReauth,
          })),
        }
      }),

    // The claim page's data: which GitHub accounts the OAuth callback proved
    // control of, and which are already linked to the target workspace.
    claimPreview: authedProcedure
      .input(z.object({ ticket: z.string().min(1) }))
      .query(async ({ ctx, input }) => {
        const claim = readGithubClaimTicket(input.ticket, ctx.session.user.id)
        if (!claim) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `This claim link expired or belongs to another session. Restart the connect flow from workspace settings.`,
          })
        }
        await assertCanManageRepos(ctx.session.user.id, claim.w)
        const rows = await db
          .select({
            id: githubInstallations.id,
            installationId: githubInstallations.installationId,
            accountLogin: githubInstallations.accountLogin,
            accountType: githubInstallations.accountType,
          })
          .from(githubInstallations)
          .where(inArray(githubInstallations.installationId, claim.ids))
        const linked = await db
          .select({
            githubInstallationId: githubInstallationLinks.githubInstallationId,
          })
          .from(githubInstallationLinks)
          .where(eq(githubInstallationLinks.workspaceId, claim.w))
        const linkedIds = new Set(linked.map((l) => l.githubInstallationId))
        return {
          workspaceId: claim.w,
          mobile: claim.m === true,
          dialog: claim.d === true,
          installations: rows.map((row) => ({
            installationId: row.installationId,
            accountLogin: row.accountLogin,
            accountType: row.accountType,
            alreadyLinked: linkedIds.has(row.id),
          })),
        }
      }),

    // Create the workspace ↔ installation links the user picked on the claim
    // page. The ticket bounds the choosable set to exactly what the OAuth
    // enumeration proved control of.
    claimLinks: authedProcedure
      .input(
        z.object({
          ticket: z.string().min(1),
          installationIds: z.array(z.number().int().positive()).min(1),
        })
      )
      .mutation(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const claim = readGithubClaimTicket(input.ticket, userId)
        if (!claim) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `This claim link expired or belongs to another session. Restart the connect flow from workspace settings.`,
          })
        }
        const allowed = new Set(claim.ids)
        if (input.installationIds.some((id) => !allowed.has(id))) {
          throw new TRPCError({
            code: `FORBIDDEN`,
            message: `Selection includes an installation this claim didn't verify.`,
          })
        }
        await assertCanManageRepos(userId, claim.w)
        const rows = await db
          .select({ id: githubInstallations.id })
          .from(githubInstallations)
          .where(
            inArray(githubInstallations.installationId, input.installationIds)
          )
        if (rows.length > 0) {
          await db
            .insert(githubInstallationLinks)
            .values(
              rows.map((row) => ({
                workspaceId: claim.w,
                githubInstallationId: row.id,
                createdByUserId: userId,
              }))
            )
            .onConflictDoNothing()
        }
        invalidateRepoCache(claim.w)
        return { linked: rows.length, workspaceId: claim.w }
      }),

    // Remove a workspace ↔ installation link. Blocked (CONFLICT) while the
    // workspace still has connected repos under that installation — mirroring
    // repositories.remove's projects-restrict — so no repo row silently loses
    // its token path.
    unlink: authedProcedure
      .input(
        z.object({
          workspaceId: z.string().uuid(),
          installationId: z.number().int().positive(),
        })
      )
      .mutation(async ({ ctx, input }) => {
        await assertCanManageRepos(ctx.session.user.id, input.workspaceId)
        // The dogfood board's GitHub connection is protected — bootstrap
        // re-heals the link on boot anyway; refuse explicitly and immediately.
        if (input.workspaceId === (await getFeedbackWorkspaceId())) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `The dogfood GitHub connection is protected`,
          })
        }
        const inUse = await db
          .select({ id: repositories.id })
          .from(repositories)
          .where(
            and(
              eq(repositories.workspaceId, input.workspaceId),
              eq(repositories.installationId, input.installationId),
              isNull(repositories.archivedAt)
            )
          )
        if (inUse.length > 0) {
          throw new TRPCError({
            code: `CONFLICT`,
            message: `Cannot disconnect — ${inUse.length} connected ${
              inUse.length === 1 ? `repository uses` : `repositories use`
            } this GitHub account. Remove them first.`,
          })
        }
        const [inst] = await db
          .select({ id: githubInstallations.id })
          .from(githubInstallations)
          .where(eq(githubInstallations.installationId, input.installationId))
          .limit(1)
        if (inst) {
          await db
            .delete(githubInstallationLinks)
            .where(
              and(
                eq(githubInstallationLinks.workspaceId, input.workspaceId),
                eq(githubInstallationLinks.githubInstallationId, inst.id)
              )
            )
        }
        invalidateRepoCache(input.workspaceId)
        return { ok: true as const }
      }),
  }),
})
