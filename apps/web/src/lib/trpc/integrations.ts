import { eq, isNull, or } from "drizzle-orm"
import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import { githubInstallations } from "@/db/schema"
import { isUserAdmin } from "@/lib/admin"
import { TRPCError } from "@trpc/server"
import {
  githubAppConfigured,
  githubAppInstallUrl,
  installationIdForRepo,
  listAppInstallations,
  listInstallationRepos,
  type InstallationRepo,
} from "@/lib/integrations/github-app"
import { mintGithubSetupState } from "@/lib/integrations/github-setup-state"

// Every install link carries a signed single-use state token bound to the
// requesting user — the setup redirect only attributes an installation when
// that token round-trips through GitHub and matches the callback's session
// (see github-setup-state.ts). `dialog: true` makes the redirect land on the
// self-closing /integrations/github/installed page, which every client flow
// (web popup, native external browser) wants. `mobile: true` (requested via
// the repos query's `platform: "mobile"` input) additionally marks the state
// so the setup redirect serves the exp://github-connected deep-link page that
// hands the user back to the native app.
function installUrlFor(
  userId: string,
  opts?: { mobile?: boolean }
): string | null {
  return githubAppInstallUrl(
    mintGithubSetupState(userId, { dialog: true, mobile: opts?.mobile })
  )
}

// Short-lived in-process cache of a user's installable repos so re-opening the
// project dialog doesn't hammer GitHub (and its secondary rate limits).
const REPOS_TTL_MS = 60_000
const repoCache = new Map<
  string,
  { repos: InstallationRepo[]; hasMore: boolean; expiresAt: number }
>()

// Drop a user's cached repo list so the next `repos` query re-hits GitHub.
// Called after a mid-flow install lands (setup redirect) or when the UI asks
// for a forced refresh, both of which mean the installable set just changed.
export function invalidateRepoCache(userId: string): void {
  repoCache.delete(userId)
}

interface ResolvedInstallation {
  installationId: number
  accountLogin: string | null
}

// Same TTL idea for the GitHub-API fallback below: an instance with the App
// genuinely not installed anywhere would otherwise hit GitHub on every
// status/repos query.
const FALLBACK_TTL_MS = 60_000
let fallbackCache: {
  installs: ResolvedInstallation[]
  expiresAt: number
} | null = null

// The installations a user's UI should treat as "installed" — STRICTLY the
// caller's own attributed rows. Unattributed rows (user_id null: installation
// webhooks and logged-out setup redirects) must never be served to regular
// users: an installation grants access to its account's repos (browse via
// `repos`, connect via projects.create), so showing someone else's
// installation is a cross-user repo leak. The in-app install flow attributes
// via the signed state token round-trip (github-setup-state.ts); the setup
// redirect only ever fills a NULL user_id — it never reattributes a row that
// already has an owner.
//
// Instance ADMINS additionally see unattributed rows and (empty-table only)
// the App-API fallback: they operate the GitHub App anyway, and this keeps
// the fresh-instance / missed-redirect case self-healing for the person who
// actually set the instance up.
async function resolveInstallations(
  userId: string
): Promise<ResolvedInstallation[]> {
  const callerIsAdmin = await isUserAdmin(userId)
  const rows = await db
    .select({
      installationId: githubInstallations.installationId,
      accountLogin: githubInstallations.accountLogin,
    })
    .from(githubInstallations)
    .where(
      callerIsAdmin
        ? or(
            eq(githubInstallations.userId, userId),
            isNull(githubInstallations.userId)
          )
        : eq(githubInstallations.userId, userId)
    )
  if (rows.length > 0) return rows
  if (!callerIsAdmin) return []

  // Admin with no rows at all: the fresh-instance/missed-webhook case
  // (webhook not configured, install finished in another browser). Ask the
  // App API for the truth and self-heal the table so the next call is
  // DB-only.
  const [anyRow] = await db
    .select({ installationId: githubInstallations.installationId })
    .from(githubInstallations)
    .limit(1)
  if (anyRow) return []

  if (fallbackCache && fallbackCache.expiresAt > Date.now()) {
    return fallbackCache.installs
  }
  let installs: Awaited<ReturnType<typeof listAppInstallations>>
  try {
    installs = await listAppInstallations()
  } catch {
    // GitHub outage/rate limit must degrade to "not installed", not reject
    // the whole status/repos query. Cache the miss so a failing API isn't
    // hammered on every call.
    fallbackCache = { installs: [], expiresAt: Date.now() + FALLBACK_TTL_MS }
    return []
  }
  const resolved = installs.map((inst) => ({
    installationId: inst.id,
    accountLogin: inst.account || null,
  }))
  fallbackCache = {
    installs: resolved,
    expiresAt: Date.now() + FALLBACK_TTL_MS,
  }
  for (const inst of installs) {
    await db
      .insert(githubInstallations)
      .values({
        installationId: inst.id,
        accountLogin: inst.account || null,
        accountType: inst.accountType || null,
      })
      .onConflictDoNothing()
  }
  return resolved
}

// Connect-path authorization: connecting a repo (repositories.add /
// projects.create inline) must be limited to repos reachable through an
// installation the CALLER is attributed to — the App JWT itself can reach
// every installation of the App, so without this check any user who knows a
// repo's full name could bind someone else's private repo to their own
// workspace. Returns the authoritative installation id so callers persist
// that instead of trusting the client-supplied one.
export async function assertRepoInstallationAccess(
  userId: string,
  fullName: string
): Promise<number> {
  const repoInstallationId = await installationIdForRepo(fullName)
  if (repoInstallationId == null) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `The Exponential GitHub App is not installed on ${fullName}. Install it, then try again.`,
    })
  }
  const installs = await resolveInstallations(userId)
  if (!installs.some((i) => i.installationId === repoInstallationId)) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `${fullName} belongs to a GitHub App installation that isn't connected to your account. Install the App on that account first.`,
    })
  }
  return repoInstallationId
}

export const integrationsRouter = router({
  github: router({
    // GitHub App install state for this user (drives the web Install button).
    // Token resolution is storage-free (the App JWT looks up a repo's
    // installation on demand); this only reflects what's installed.
    status: authedProcedure.query(async ({ ctx }) => {
      if (!githubAppConfigured()) {
        return {
          configured: false as const,
          installed: false,
          installUrl: null as string | null,
          accounts: [] as string[],
        }
      }
      const installs = await resolveInstallations(ctx.session.user.id)
      return {
        configured: true as const,
        installed: installs.length > 0,
        installUrl: installUrlFor(ctx.session.user.id),
        accounts: installs
          .map((r) => r.accountLogin)
          .filter((a): a is string => Boolean(a)),
      }
    }),

    // Repos the user can connect, aggregated across all their installations and
    // deduped. Backs the repo-first project create flow. `hasMore` signals the
    // result was truncated so the UI can point at "manage repos on GitHub".
    // `refresh` bypasses the per-user cache so returning from a GitHub App
    // install (new repos granted) reflects immediately. `platform: "mobile"`
    // (native clients only) marks the returned installUrl's state so the setup
    // redirect deep-links back into the app; web callers omit it and keep the
    // web landing page.
    repos: authedProcedure
      .input(
        z
          .object({
            refresh: z.boolean().optional(),
            platform: z.enum([`web`, `mobile`]).optional(),
          })
          .optional()
      )
      .query(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const mobile = input?.platform === `mobile`
        if (input?.refresh) invalidateRepoCache(userId)
        if (!githubAppConfigured()) {
          return {
            configured: false as const,
            installed: false,
            installUrl: null as string | null,
            repos: [] as InstallationRepo[],
            hasMore: false,
          }
        }

        const installs = await resolveInstallations(userId)

        if (installs.length === 0) {
          return {
            configured: true as const,
            installed: false,
            installUrl: installUrlFor(userId, { mobile }),
            repos: [] as InstallationRepo[],
            hasMore: false,
          }
        }

        const cached = repoCache.get(userId)
        if (cached && cached.expiresAt > Date.now()) {
          return {
            configured: true as const,
            installed: true,
            installUrl: installUrlFor(userId, { mobile }),
            repos: cached.repos,
            hasMore: cached.hasMore,
          }
        }

        const seen = new Set<string>()
        const merged: InstallationRepo[] = []
        let hasMore = false
        for (const inst of installs) {
          try {
            const { repos, hasMore: more } = await listInstallationRepos(
              inst.installationId
            )
            if (more) hasMore = true
            for (const repo of repos) {
              if (seen.has(repo.fullName)) continue
              seen.add(repo.fullName)
              merged.push(repo)
            }
          } catch {
            // A single revoked/404 installation must not fail the whole list.
          }
        }
        merged.sort((a, b) => a.fullName.localeCompare(b.fullName))
        repoCache.set(userId, {
          repos: merged,
          hasMore,
          expiresAt: Date.now() + REPOS_TTL_MS,
        })

        return {
          configured: true as const,
          installed: true,
          installUrl: installUrlFor(userId, { mobile }),
          repos: merged,
          hasMore,
        }
      }),
  }),
})
