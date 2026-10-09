// EXP-1251: the diff a Guide write checks its listed files against, so the
// tool answer names the paths the branch never changed (a typo, a file not
// pushed yet, a section describing another PR). The run's OWN PR files when
// it has one (or the topic's PR, same repository only: an agent-supplied url
// must never read another repository), else its pushed branch compared with
// the board's default branch. Best effort: null whenever nothing can be read.
// GitHub helpers load lazily: they open the db connection at module scope.

export interface GuideDiffFile {
  path: string
  previousPath?: string | null
}

export interface GuideDiffRun {
  prUrl: string | null
  branch: string | null
  boardId: string | null
}

const PR_URL = /github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)/

function parsePrUrl(url: string | null | undefined): { repo: string; number: number } | null {
  const match = url?.match(PR_URL)
  return match ? { repo: match[1]!, number: Number(match[2]) } : null
}

/** The PR the check reads: the topic's when it sits in the run's PR
 *  repository, else the run's own; null without a run PR. */
export function guideDiffPrUrl(run: GuideDiffRun, topicPrUrl: string | null): string | null {
  const own = parsePrUrl(run.prUrl)
  if (!own) return null
  const topic = parsePrUrl(topicPrUrl)
  return topic && topic.repo === own.repo ? topicPrUrl : run.prUrl
}

export async function loadGuideDiff(
  run: GuideDiffRun,
  topicPrUrl: string | null
): Promise<GuideDiffFile[] | null> {
  try {
    const prUrl = guideDiffPrUrl(run, topicPrUrl)
    const pr = parsePrUrl(prUrl)
    if (pr) {
      const { loadPrFiles } = await import(`@/lib/integrations/pr-files`)
      const answer = await loadPrFiles(prUrl, pr.number)
      return answer.files.map((file) => ({
        path: file.filename,
        previousPath: file.previous_filename ?? null,
      }))
    }
    if (!run.branch || !run.boardId) return null
    const { resolveBoardRepository } = await import(`@/lib/trpc/repositories`)
    const repo = await resolveBoardRepository(run.boardId)
    if (!repo) return null
    const { fetchBranchDiff, resolveRepoInstallationTokenInfo } = await import(
      `@/lib/integrations/github-app`
    )
    const resolved = await resolveRepoInstallationTokenInfo(repo.fullName, {
      fallbackInstallationId: repo.installationId,
    })
    const diff = await fetchBranchDiff({
      repo: repo.fullName,
      base: repo.defaultBranch,
      branch: run.branch,
      token: resolved?.token ?? null,
    })
    if (!diff) return null
    return diff.files.map((file) => ({
      path: file.filename,
      previousPath: file.previous_filename ?? null,
    }))
  } catch (err) {
    console.warn(`[session-guide-diff] diff read failed`, err)
    return null
  }
}
