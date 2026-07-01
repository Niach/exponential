import { eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { githubInstallations } from "@/db/schema"
import {
  githubAppConfigured,
  githubAppInstallUrl,
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

export const integrationsRouter = router({
  github: router({
    // GitHub App install state for this user (drives the web Install button).
    // Token resolution is storage-free (the App JWT looks up a repo's
    // installation on demand); this only reflects what the user has installed.
    status: authedProcedure.query(async ({ ctx }) => {
      if (!githubAppConfigured()) {
        return {
          configured: false as const,
          installed: false,
          installUrl: null as string | null,
          accounts: [] as string[],
        }
      }
      const rows = await ctx.db
        .select({ accountLogin: githubInstallations.accountLogin })
        .from(githubInstallations)
        .where(eq(githubInstallations.userId, ctx.session.user.id))
      return {
        configured: true as const,
        installed: rows.length > 0,
        installUrl: githubAppInstallUrl(),
        accounts: rows
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

      const installs = await ctx.db
        .select({ installationId: githubInstallations.installationId })
        .from(githubInstallations)
        .where(eq(githubInstallations.userId, userId))

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
