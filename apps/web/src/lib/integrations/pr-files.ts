import { TRPCError } from "@trpc/server"
import { fetchPullFiles, type PullFile } from "@/lib/integrations/github-pr"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"

// EXP-1154: every md+ issue view with an open PR asks `prFiles`, so a warm
// answer must not first resolve an installation token (an uncached App-JWT
// round-trip). Keyed by PR, same 60s as `fetchPullFiles`' own cache, which
// still holds the per-auth-posture answers underneath. Rejections evict.
// EXP-1194: shared by `issues.prFiles` and `codingSessions.prFiles` (an
// issue-less run's PR), so both readers of one PR hit one entry.
const prFilesAnswerCache = new TtlPromiseCache<PullFile[]>({
  ttlMs: 60_000,
  maxEntries: 200,
})

export function _clearPrFilesAnswerCache() {
  prFilesAnswerCache.clear()
}

export type PrFilesAnswer = {
  repo: string | null
  prNumber: number | null
  files: PullFile[]
}

/** The changed files of the PR at `prUrl` — the diff view's one GitHub read.
 *  The repo comes from the URL (a run has no board); no PR = no files. The
 *  caller has already gated membership. */
export async function loadPrFiles(
  prUrl: string | null,
  prNumber: number | null
): Promise<PrFilesAnswer> {
  const match = prUrl?.match(/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/)
  const repo = match ? match[1] : null
  if (prNumber == null || !repo) {
    return { repo: null, prNumber: null, files: [] }
  }
  try {
    const files = await prFilesAnswerCache.get(`${repo}#${prNumber}`, async () => {
      const resolved = await resolveRepoInstallationTokenInfo(repo)
      return fetchPullFiles(repo, prNumber, resolved?.token)
    })
    return { repo, prNumber, files }
  } catch (err) {
    throw new TRPCError({
      code: `BAD_GATEWAY`,
      message:
        err instanceof Error ? err.message : `Failed to load changes from GitHub`,
    })
  }
}
