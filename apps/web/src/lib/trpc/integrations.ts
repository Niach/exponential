import { eq, isNull, or } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import { githubInstallations } from "@/db/schema"
import {
  githubAppConfigured,
  githubAppInstallUrl,
  listAppInstallations,
  listInstallationRepos,
  type InstallationRepo,
} from "@/lib/integrations/github-app"

// Short-lived in-process cache of a user's installable repos so re-opening the
// project dialog doesn't hammer GitHub (and its secondary rate limits).
const REPOS_TTL_MS = 60_000
const repoCache = new Map<
  string,
  { repos: InstallationRepo[]; hasMore: boolean; expiresAt: number }
>()

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

// The installations a user's UI should treat as "installed". DB rows are the
// fast path: the user's own rows PLUS unattributed ones (the installation
// webhook and a logged-out setup redirect insert with user_id null — those
// can't be attributed to anyone, and hiding them made the UI claim "not
// installed" right after a successful install). Only when the TABLE ITSELF is
// empty — the fresh-instance/missed-webhook case (webhook not configured,
// install finished in another browser) — ask the App API for the truth and
// self-heal the table so the next call is DB-only. The emptiness gate is
// deliberate: falling back whenever the CALLER sees zero rows would leak
// other users' attributed installations (their accounts + all the App's
// repos) and re-hit GitHub every TTL forever, because the self-heal upsert
// no-ops on rows that already exist attributed to someone else.
async function resolveInstallations(
  userId: string
): Promise<ResolvedInstallation[]> {
  const rows = await db
    .select({
      installationId: githubInstallations.installationId,
      accountLogin: githubInstallations.accountLogin,
    })
    .from(githubInstallations)
    .where(
      or(
        eq(githubInstallations.userId, userId),
        isNull(githubInstallations.userId)
      )
    )
  if (rows.length > 0) return rows

  // Existence probe with NO user filter: any row at all means the writers are
  // working and this caller simply has no installations of their own.
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
  fallbackCache = { installs: resolved, expiresAt: Date.now() + FALLBACK_TTL_MS }
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
        installUrl: githubAppInstallUrl(),
        accounts: installs
          .map((r) => r.accountLogin)
          .filter((a): a is string => Boolean(a)),
      }
    }),

    // Repos the user can connect, aggregated across all their installations and
    // deduped. Backs the repo-first project create flow. `hasMore` signals the
    // result was truncated so the UI can point at "manage repos on GitHub".
    repos: authedProcedure.query(async ({ ctx }) => {
      const userId = ctx.session.user.id
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
          installUrl: githubAppInstallUrl(`dialog`),
          repos: [] as InstallationRepo[],
          hasMore: false,
        }
      }

      const cached = repoCache.get(userId)
      if (cached && cached.expiresAt > Date.now()) {
        return {
          configured: true as const,
          installed: true,
          installUrl: githubAppInstallUrl(`dialog`),
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
        installUrl: githubAppInstallUrl(`dialog`),
        repos: merged,
        hasMore,
      }
    }),
  }),
})
