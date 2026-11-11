// EXP-1709: the GitHub implementation of the forge seam (`forge.ts`). The
// App installation token acts; clients never touch git. The pipeline is the
// repo's `.github/workflows/train.yml` (#EXP-1708), dispatched on the base
// branch; its report is the `exp-train` check run on `exp-train/<train_id>`.
import {
  TRAIN_PIPELINE_CHECK,
  trainRef,
  type TrainCheck,
  type TrainPipelineOutcome,
} from "@exp/domain-contract/merge-train"
import { githubApiHeaders, resolveRepoInstallationToken } from "@/lib/integrations/github-app"
import { githubFetch, type GithubClientDeps } from "@/lib/integrations/github-client"
import { githubErrorMessage } from "@/lib/integrations/github-error"
import { fetchGithubCheckRuns, requiredChecksFor } from "@/lib/integrations/pr-checks"
import type { Forge, ForgePullRequest, ForgeRepo } from "@/lib/ecode/forge"
import { checkFromGithubStatus } from "@/lib/ecode/github-status"

/** The pipeline's workflow file (#EXP-1708 ships it into the repo). */
export const GITHUB_TRAIN_WORKFLOW = `train.yml`

let deps: GithubClientDeps = {}
let tokenFor: (repo: ForgeRepo) => Promise<string | null> = (repo) =>
  resolveRepoInstallationToken(repo.fullName, { fallbackInstallationId: repo.installationId ?? null })

/** Tests: a stub fetch and a fixed token. */
export function setGithubForgeDepsForTests(next: {
  deps?: GithubClientDeps
  token?: string | null
} | null): void {
  deps = next?.deps ?? {}
  tokenFor = next
    ? async () => next.token ?? null
    : (repo) => resolveRepoInstallationToken(repo.fullName, { fallbackInstallationId: repo.installationId ?? null })
}

async function call(
  repo: ForgeRepo,
  path: string,
  init: { method?: string; body?: unknown } = {}
): Promise<{ res: Response; token: string | null }> {
  const token = await tokenFor(repo)
  const res = await githubFetch(
    path,
    {
      method: init.method ?? `GET`,
      headers: {
        ...githubApiHeaders(token ?? undefined),
        ...(init.body !== undefined ? { "content-type": `application/json` } : {}),
      },
      ...(init.body !== undefined ? { body: JSON.stringify(init.body) } : {}),
    },
    deps
  )
  return { res, token }
}

function branchPath(branch: string): string {
  return branch.split(`/`).map(encodeURIComponent).join(`/`)
}

/** `Name <email>` for a GitHub user: the noreply address GitHub itself uses. */
export function githubAuthorLine(user: { id?: number | null; login?: string | null } | null | undefined): string | null {
  if (!user?.login) return null
  const email = user.id ? `${user.id}+${user.login}@users.noreply.github.com` : `${user.login}@users.noreply.github.com`
  return `${user.login} <${email}>`
}

/** The `exp-train` check run's output → the outcome JSON, null when absent
 *  or unreadable. */
export function parsePipelineOutcome(text: string | null | undefined): TrainPipelineOutcome | null {
  if (!text) return null
  try {
    const parsed = JSON.parse(text) as TrainPipelineOutcome
    return parsed && typeof parsed === `object` && Array.isArray(parsed.units) ? parsed : null
  } catch {
    return null
  }
}

export const githubForge: Forge = {
  kind: `github`,

  async readPullRequest(repo, number) {
    const { res } = await call(repo, `/repos/${repo.fullName}/pulls/${number}`)
    if (res.status === 404) return null
    if (!res.ok) throw new Error(`GitHub returned ${res.status} for ${repo.fullName}#${number}: ${await githubErrorMessage(res)}`)
    const data = (await res.json()) as {
      state?: string
      merged?: boolean
      draft?: boolean
      title?: string
      merged_at?: string | null
      merged_by?: { id?: number; login?: string; type?: string } | null
      user?: { id?: number; login?: string } | null
      head?: { sha?: string; ref?: string }
      base?: { ref?: string }
      mergeable?: boolean | null
      updated_at?: string | null
    }
    const merged = Boolean(data.merged)
    const pr: ForgePullRequest = {
      number,
      state: data.state === `closed` ? `closed` : `open`,
      merged,
      draft: Boolean(data.draft),
      headSha: data.head?.sha || null,
      headRef: data.head?.ref || null,
      baseRef: data.base?.ref || null,
      title: data.title ?? ``,
      mergedAt: merged && data.merged_at ? new Date(data.merged_at) : null,
      mergedBy: merged ? (data.merged_by ?? null) : null,
      author: githubAuthorLine(data.user),
      mergeable: data.mergeable ?? null,
      updatedAt: data.updated_at ? new Date(data.updated_at) : null,
    }
    return pr
  },

  async readBranch(repo, branch) {
    const { res } = await call(repo, `/repos/${repo.fullName}/git/ref/heads/${branchPath(branch)}`)
    if (res.status === 404) return null
    if (!res.ok) throw new Error(`GitHub returned ${res.status} reading ${repo.fullName}@${branch}: ${await githubErrorMessage(res)}`)
    const data = (await res.json()) as { object?: { sha?: string } }
    return data.object?.sha ?? null
  },

  async requiredChecks(repo, base) {
    const token = await tokenFor(repo)
    if (!token) return []
    const names = await requiredChecksFor(repo.fullName, base, token)
    if (!names) throw new Error(`the required checks of ${repo.fullName}@${base} are unreadable`)
    return [...names]
  },

  async dispatchPipeline(repo, inputs) {
    const { res } = await call(repo, `/repos/${repo.fullName}/actions/workflows/${GITHUB_TRAIN_WORKFLOW}/dispatches`, {
      method: `POST`,
      body: { ref: inputs.base, inputs, return_run_details: true },
    })
    if (res.status === 404) return { ok: false, missing: true, message: `no ${GITHUB_TRAIN_WORKFLOW} on ${inputs.base}` }
    if (!res.ok) return { ok: false, missing: false, message: `GitHub returned ${res.status}: ${await githubErrorMessage(res)}` }
    // 200 with the run's id (return_run_details), or 204 without a body.
    let runRef: string | null = null
    if (res.status === 200) {
      const data = (await res.json().catch(() => null)) as { workflow_run_id?: number } | null
      if (data?.workflow_run_id) runRef = String(data.workflow_run_id)
    }
    return { ok: true, runRef }
  },

  async readPipeline(repo, trainId, runRef) {
    const ref = branchPath(trainRef(trainId))
    const { res } = await call(
      repo,
      `/repos/${repo.fullName}/commits/${ref}/check-runs?check_name=${encodeURIComponent(TRAIN_PIPELINE_CHECK)}&filter=latest`
    )
    let outcome: TrainPipelineOutcome | null = null
    if (res.ok) {
      const data = (await res.json()) as {
        check_runs?: Array<{ external_id?: string | null; status?: string; output?: { text?: string | null } }>
      }
      const run = (data.check_runs ?? []).find((r) => r.status === `completed` && (!r.external_id || r.external_id === trainId))
      outcome = parsePipelineOutcome(run?.output?.text)
    }
    if (outcome) return { outcome, finished: true }
    if (!runRef) return { outcome: null, finished: false }
    const run = await call(repo, `/repos/${repo.fullName}/actions/runs/${encodeURIComponent(runRef)}`)
    if (!run.res.ok) return { outcome: null, finished: false }
    const data = (await run.res.json()) as { status?: string }
    return { outcome: null, finished: data.status === `completed` }
  },

  async checksAt(repo, sha) {
    const token = await tokenFor(repo)
    if (!token) return null
    const read = await fetchGithubCheckRuns(repo.fullName, sha, token, deps)
    if (!read) return null
    const checks: TrainCheck[] = read.checks.map((c) => ({ name: c.name, status: c.status, conclusion: c.conclusion }))
    // A required check may be a commit STATUS (a context), not a check run.
    const { res } = await call(repo, `/repos/${repo.fullName}/commits/${encodeURIComponent(sha)}/status`)
    if (res.ok) {
      const data = (await res.json()) as { statuses?: Array<{ context?: string; state?: string }> }
      for (const status of data.statuses ?? []) {
        if (!status.context || checks.some((c) => c.name === status.context)) continue
        checks.push({ name: status.context, ...checkFromGithubStatus(status.state) })
      }
    }
    return checks
  },

  async branchContains(repo, branch, sha) {
    // compare/{sha}...{branch}: `identical` = the branch IS sha, `ahead` =
    // sha is an ancestor of it; `behind`/`diverged` = it is not on it.
    const { res } = await call(
      repo,
      `/repos/${repo.fullName}/compare/${encodeURIComponent(sha)}...${branchPath(branch)}`
    )
    if (res.status === 404) return false
    if (!res.ok) return null
    const data = (await res.json().catch(() => null)) as { status?: string } | null
    if (data?.status === `identical` || data?.status === `ahead`) return true
    if (data?.status === `behind` || data?.status === `diverged`) return false
    return null
  },

  async fastForward(repo, { branch, from, to }) {
    const current = await githubForge.readBranch(repo, branch)
    if (current !== from) return { ok: false, reason: `moved`, current }
    // force: false = GitHub refuses anything but a fast-forward, so a branch
    // that moved between the read and the write is refused too.
    const { res } = await call(repo, `/repos/${repo.fullName}/git/refs/heads/${branchPath(branch)}`, {
      method: `PATCH`,
      body: { sha: to, force: false },
    })
    if (res.ok) return { ok: true }
    const message = await githubErrorMessage(res)
    if (res.status === 422 && /fast.?forward/i.test(message)) {
      return { ok: false, reason: `moved`, current: await githubForge.readBranch(repo, branch).catch(() => null) }
    }
    return { ok: false, reason: `refused`, message: `GitHub returned ${res.status}: ${message}` }
  },

  async closeAsLanded(repo, { number, sha, base }) {
    const comment = await call(repo, `/repos/${repo.fullName}/issues/${number}/comments`, {
      method: `POST`,
      body: { body: `Landed in \`${base}\` as ${sha} by the Exponential merge train.` },
    })
    if (!comment.res.ok) {
      console.error(`[forge] comment on ${repo.fullName}#${number} failed: ${comment.res.status}`)
    }
    const { res } = await call(repo, `/repos/${repo.fullName}/pulls/${number}`, {
      method: `PATCH`,
      body: { state: `closed` },
    })
    if (!res.ok) throw new Error(`GitHub returned ${res.status} closing ${repo.fullName}#${number}: ${await githubErrorMessage(res)}`)
  },

  async squashMerge(repo, { number, title, expectedHead }) {
    const { res } = await call(repo, `/repos/${repo.fullName}/pulls/${number}/merge`, {
      method: `PUT`,
      body: {
        merge_method: `squash`,
        commit_title: `${title} (#${number})`,
        ...(expectedHead ? { sha: expectedHead } : {}),
      },
    })
    if (res.ok) {
      const data = (await res.json().catch(() => null)) as { sha?: string } | null
      return { ok: true, sha: data?.sha ?? null }
    }
    const message = await githubErrorMessage(res)
    return { ok: false, conflict: res.status === 405 && /conflict|not mergeable/i.test(message), message }
  },
}

