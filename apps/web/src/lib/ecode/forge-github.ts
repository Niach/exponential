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
import { byCodePoint, capabilityValues, supports, type ForgeReason } from "@exp/ecode"
import {
  githubApiHeaders,
  githubAppConfigured,
  fetchBranchDiff,
  resolveRepoDefaultBranchCached,
  resolveRepoInstallationTokenInfo,
} from "@/lib/integrations/github-app"
import { githubFetch, type GithubClientDeps } from "@/lib/integrations/github-client"
import { githubErrorMessage } from "@/lib/integrations/github-error"
import {
  closePullRequest,
  createPullRequest,
  updatePullRequest,
  deleteBranch,
  fetchPullFiles,
  fetchPullStack,
  findOpenPullByHead,
  getPullRequest,
  GitHubMergeError,
  listOpenPulls,
  listOpenPullsByBase,
  PullAlreadyExistsError,
  retargetPullRequest,
  setPullRequestDraft,
  unstack,
  type GitHubFetch,
  type PullFile,
} from "@/lib/integrations/github-pr"
import { fetchGithubCheckRuns, requiredChecksFor } from "@/lib/integrations/pr-checks"
import { loadPrFiles } from "@/lib/integrations/pr-files"
import { inferBaseBranch } from "@/lib/pr-stacks"
import {
  ForgeError,
  type Forge,
  type ForgeFile,
  type ForgeMergeOrderedResult,
  type ForgePullRequest,
  type ForgeRepo,
} from "@/lib/ecode/forge"
import { checkFromGithubStatus } from "@/lib/ecode/github-status"

/** The pipeline's workflow file (#EXP-1708 ships it into the repo). */
export const GITHUB_TRAIN_WORKFLOW = `train.yml`

let deps: GithubClientDeps = {}
const resolveToken = async (repo: ForgeRepo): Promise<string | null> =>
  (await resolveRepoInstallationTokenInfo(repo.fullName, { fallbackInstallationId: repo.installationId ?? null }))?.token ??
  null
let tokenFor: (repo: ForgeRepo) => Promise<string | null> = resolveToken

/** Tests: a stub fetch and a fixed token. */
export function setGithubForgeDepsForTests(next: {
  deps?: GithubClientDeps
  token?: string | null
} | null): void {
  deps = next?.deps ?? {}
  prFetch = next?.deps?.fetch ? (url, init) => githubFetch(url, init, deps) : undefined
  tokenFor = next
    ? async () => next.token ?? null
    : resolveToken
}

/** The stub behind the `integrations/github-pr` calls (tests only). */
let prFetch: GitHubFetch | undefined

/** The credential the PR-tree ops act with: the caller's (already gated)
 *  token, else the forge's own resolution. */
async function tokenOf(repo: ForgeRepo): Promise<string | null> {
  return repo.token !== undefined ? repo.token : tokenFor(repo)
}

async function requireToken(repo: ForgeRepo): Promise<string> {
  const token = await tokenOf(repo)
  if (!token) throw new ForgeError(403, `The GitHub App has no access to ${repo.fullName}`)
  return token
}

/** GitHub's refusal message → eCode's reason, where GitHub names one. */
export function githubReason(status: number, message: string): ForgeReason | null {
  if (/already exists/i.test(message)) return `exists`
  if (status === 409 || /head branch was modified/i.test(message)) return `head_moved`
  if (/draft/i.test(message)) return `draft`
  if (/merge conflict|conflict/i.test(message)) return `dirty`
  if (/closed|not open/i.test(message)) return `not_open`
  if (/cycle|circular/i.test(message)) return `cycle`
  if (/same branch|head and base|no commits between/i.test(message)) return `same_branch`
  if (/base\b.*invalid|invalid.*\bbase|base branch/i.test(message)) return `base`
  if (/head\b.*invalid|invalid.*\bhead/i.test(message)) return `head`
  if (status === 404) return `pr`
  if (status === 405) return `dirty`
  return null
}

/** Run a github-pr call; its GitHubMergeError/PullAlreadyExistsError
 *  surface as ForgeError (same status and message). */
async function mapped<T>(run: () => Promise<T>): Promise<T> {
  try {
    return await run()
  } catch (e) {
    if (e instanceof GitHubMergeError) throw new ForgeError(e.status, e.message, githubReason(e.status, e.message))
    if (e instanceof PullAlreadyExistsError) throw new ForgeError(422, e.message, `exists`)
    throw e
  }
}

/** A GitHub repo's https remote and the user an installation token
 *  authenticates as (devices stop building GitHub URLs, #EXP-1614). */
export function githubRemote(fullName: string): { remoteUrl: string; username: string } {
  return { remoteUrl: `https://github.com/${fullName}.git`, username: `x-access-token` }
}

/** A github-pr read's "GitHub returned 404" → ForgeError reason `pr`. */
function notFoundAsForgeError(e: unknown): unknown {
  if (e instanceof Error && !(e instanceof ForgeError) && /returned 404\b/.test(e.message)) {
    return new ForgeError(404, e.message, `pr`)
  }
  return e
}

/** GitHub's file status → the neutral change kind. */
export function forgeFileOf(file: PullFile): ForgeFile {
  const known = [`added`, `modified`, `removed`, `renamed`, `copied`, `changed`, `unchanged`] as const
  const change = (known as readonly string[]).includes(file.status) ? (file.status as ForgeFile[`change`]) : `changed`
  return {
    path: file.filename,
    previousPath: file.previous_filename ?? null,
    change,
    additions: file.additions,
    deletions: file.deletions,
    patch: file.patch ?? null,
  }
}

async function call(
  repo: ForgeRepo,
  path: string,
  init: { method?: string; body?: unknown } = {}
): Promise<{ res: Response; token: string | null }> {
  const token = await tokenOf(repo)
  const res = await githubFetch(
    path,
    {
      method: init.method ?? `GET`,
      headers: {
        // A read without an App token falls back to a self-hoster's PAT
        // (GITHUB_TOKEN), like every github-pr read.
        ...githubApiHeaders(token || ((init.method ?? `GET`) === `GET` ? process.env.GITHUB_TOKEN : undefined) || undefined),
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

export const githubForge: Forge & {
  rawFiles(repo: ForgeRepo, number: number, opts?: { fresh?: boolean }): Promise<PullFile[]>
} = {
  kind: `github`,
  capabilities: capabilityValues.filter((capability) => supports(`github`, capability)),

  async openPullRequest(repo, { head, base, title, body, draft }) {
    const token = await requireToken(repo)
    return mapped(async () => {
      try {
        return await createPullRequest({
          repo: repo.fullName,
          head,
          base,
          title,
          body: body ?? ``,
          token,
          ...(draft ? { draft: true } : {}),
          ...(prFetch ? { fetchImpl: prFetch } : {}),
        })
      } catch (e) {
        // createPullRequest's refusals are plain errors naming the status.
        const status = e instanceof Error ? Number(e.message.match(/PR create failed \((\d+)\)/)?.[1]) : NaN
        if (e instanceof Error && !(e instanceof PullAlreadyExistsError) && Number.isFinite(status)) {
          throw new ForgeError(status, e.message, githubReason(status, e.message))
        }
        throw e
      }
    })
  },

  async findOpenByHead(repo, head, base) {
    const found = await findOpenPullByHead(repo.fullName, head, await tokenOf(repo), prFetch, base)
    return found ? { number: found.number, url: found.url, headRef: head, baseRef: found.baseRef } : null
  },

  async listOpenByBase(repo, base) {
    const pulls = await listOpenPullsByBase(repo.fullName, base, await tokenOf(repo), prFetch)
    // forge.json lists by number; GitHub lists newest first.
    return pulls
      .map((pull) => ({ number: pull.number, url: pull.url, headRef: pull.headRef, baseRef: base }))
      .sort((a, b) => a.number - b.number)
  },

  async listOpen(repo) {
    const token = await tokenOf(repo)
    const pulls = prFetch ? await listOpenPulls(repo.fullName, token, prFetch) : await listOpenPulls(repo.fullName, token)
    return pulls.map((pull) => ({
      number: pull.number,
      url: pull.url,
      headRef: pull.branch,
      baseRef: pull.baseBranch,
      title: pull.title,
      draft: pull.draft,
      authorLogin: pull.authorLogin,
      authorAvatarUrl: pull.authorAvatarUrl,
      createdAt: pull.createdAt,
    }))
  },

  async retarget(repo, { number, base }) {
    const token = await requireToken(repo)
    await mapped(() =>
      retargetPullRequest({ repo: repo.fullName, prNumber: number, base, token, ...(prFetch ? { fetchImpl: prFetch } : {}) })
    )
  },

  async setDraft(repo, { number, draft }) {
    const token = await requireToken(repo)
    await mapped(() =>
      setPullRequestDraft({ repo: repo.fullName, prNumber: number, draft, token, ...(prFetch ? { fetchImpl: prFetch } : {}) })
    )
  },

  async mergeability(repo, number) {
    let pull: Awaited<ReturnType<typeof getPullRequest>>
    try {
      pull = await getPullRequest(repo.fullName, number, await tokenOf(repo), prFetch)
    } catch (e) {
      throw notFoundAsForgeError(e)
    }
    // GitHub names no conflicting paths; null = not computed yet.
    const mergeability = pull.mergeable == null ? null : pull.mergeable ? (`clean` as const) : (`dirty` as const)
    return { mergeability, conflicts: [] }
  },

  async mergeOrdered(repo, { numbers, expectedHeads }) {
    // GitHub lacks `ordered_merge`: a loop of squash merges, one PR at a
    // time, stopping at the first refusal (forge.json `pr.merge_ordered`).
    if (new Set(numbers).size !== numbers.length) throw new ForgeError(422, `a PR is named twice`, `duplicate`)
    const pulls = new Map<number, ForgePullRequest>()
    for (const number of numbers) {
      const pr = await githubForge.readPullRequest(repo, number)
      if (!pr) throw new ForgeError(404, `${repo.fullName}#${number} not found`, `pr`)
      pulls.set(number, pr)
    }
    const result: ForgeMergeOrderedResult = { merged: [], failed: null }
    for (const number of numbers) {
      const merge = await githubForge.squashMerge(repo, {
        number,
        title: pulls.get(number)!.title,
        expectedHead: expectedHeads?.[number] ?? null,
      })
      if (!merge.ok) {
        result.failed = { number, reason: merge.reason ?? null, message: merge.message }
        break
      }
      result.merged.push({ number, sha: merge.sha })
    }
    return result
  },

  async close(repo, { number }) {
    const token = await requireToken(repo)
    await mapped(() => closePullRequest({ repo: repo.fullName, prNumber: number, token, ...(prFetch ? { fetchImpl: prFetch } : {}) }))
  },

  async files(repo, number, opts) {
    const files = await githubForge.rawFiles(repo, number, opts).catch((e: unknown) => {
      throw notFoundAsForgeError(e)
    })
    // forge.json: code-point path order (GitHub lists them its own way).
    return files.map(forgeFileOf).sort((a, b) => byCodePoint(a.path, b.path))
  },

  async rawFiles(repo: ForgeRepo, number: number, opts?: { fresh?: boolean }): Promise<PullFile[]> {
    return prFetch
      ? await fetchPullFiles(repo.fullName, number, await tokenOf(repo), prFetch, { fresh: true })
      : opts?.fresh || repo.token !== undefined
        ? await fetchPullFiles(repo.fullName, number, await tokenOf(repo), undefined, opts)
        : (await loadPrFiles(`https://github.com/${repo.fullName}/pull/${number}`, number)).files
  },

  async branchFiles(repo, { base, branch }) {
    const diff = await fetchBranchDiff({ repo: repo.fullName, base, branch, token: await tokenOf(repo) })
    return diff ? diff.files.map(forgeFileOf).sort((a, b) => byCodePoint(a.path, b.path)) : null
  },

  async update(repo, { number, title, body }) {
    const token = await requireToken(repo)
    await mapped(() =>
      updatePullRequest({
        repo: repo.fullName,
        prNumber: number,
        title,
        body,
        token,
        ...(prFetch ? { fetchImpl: prFetch } : {}),
      })
    )
  },

  async inferBase(repo, args) {
    const token = await requireToken(repo)
    return inferBaseBranch({ repo: repo.fullName, token, ...args, ...(prFetch ? { fetchImpl: prFetch } : {}) })
  },

  async inNativeStack(repo, number) {
    const token = await requireToken(repo)
    return (await fetchPullStack({ repo: repo.fullName, prNumber: number, token, ...(prFetch ? { fetchImpl: prFetch } : {}) })) !== null
  },

  async leaveNativeStack(repo, number) {
    const token = await requireToken(repo)
    const fetchImpl = prFetch ? { fetchImpl: prFetch } : {}
    const stack = await fetchPullStack({ repo: repo.fullName, prNumber: number, token, ...fetchImpl })
    if (stack) await unstack({ repo: repo.fullName, stackNumber: stack.number, token, ...fetchImpl })
  },

  async credentials(repo) {
    if (!githubAppConfigured()) return null
    const resolved = await resolveRepoInstallationTokenInfo(repo.fullName, {
      fallbackInstallationId: repo.installationId ?? null,
    })
    if (!resolved) return null
    return {
      ...githubRemote(repo.fullName),
      token: resolved.token,
      expiresAt: new Date(resolved.expiresAt),
      installationId: resolved.installationId,
    }
  },

  async deleteBranch(repo, branch) {
    const token = await requireToken(repo)
    return deleteBranch({ repo: repo.fullName, branch, token, ...(prFetch ? { fetchImpl: prFetch } : {}) })
  },

  async defaultBranch(repo) {
    return resolveRepoDefaultBranchCached(repo.fullName)
  },

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
      html_url?: string | null
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
      url: data.html_url ?? null,
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
    const token = await tokenOf(repo)
    if (!token) return []
    const names = await requiredChecksFor(repo.fullName, base, token)
    if (!names) throw new Error(`the required checks of ${repo.fullName}@${base} are unreadable`)
    return [...names]
  },

  async readFile(repo, path, ref) {
    const filePath = path.split(`/`).map(encodeURIComponent).join(`/`)
    const { res } = await call(repo, `/repos/${repo.fullName}/contents/${filePath}?ref=${encodeURIComponent(ref)}`)
    if (res.status === 404) return null
    if (!res.ok) throw new Error(`GitHub returned ${res.status} reading ${repo.fullName}:${path}@${ref}: ${await githubErrorMessage(res)}`)
    const data = (await res.json()) as { type?: string; encoding?: string; content?: string }
    if (data.type !== `file` || typeof data.content !== `string`) return null
    return data.encoding === `base64` ? Buffer.from(data.content, `base64`).toString(`utf8`) : data.content
  },

  async listFiles(repo, dir, ref) {
    const dirPath = dir.split(`/`).map(encodeURIComponent).join(`/`)
    const { res } = await call(repo, `/repos/${repo.fullName}/contents/${dirPath}?ref=${encodeURIComponent(ref)}`)
    if (res.status === 404) return null
    if (!res.ok) throw new Error(`GitHub returned ${res.status} listing ${repo.fullName}:${dir}@${ref}: ${await githubErrorMessage(res)}`)
    const data = (await res.json()) as unknown
    if (!Array.isArray(data)) return null
    return data.flatMap((entry: { type?: string; name?: string }) => (entry.type === `file` && entry.name ? [entry.name] : []))
  },

  async reportStatus(repo, sha, status) {
    const { res } = await call(repo, `/repos/${repo.fullName}/statuses/${encodeURIComponent(sha)}`, {
      method: `POST`,
      body: {
        state: status.state,
        context: status.context,
        ...(status.description ? { description: status.description.slice(0, 140) } : {}),
        ...(status.targetUrl ? { target_url: status.targetUrl } : {}),
      },
    })
    if (!res.ok) throw new Error(`GitHub returned ${res.status} posting a status on ${repo.fullName}@${sha}: ${await githubErrorMessage(res)}`)
  },

  async dispatchPipeline(repo, inputs) {
    const { res } = await call(repo, `/repos/${repo.fullName}/actions/workflows/${GITHUB_TRAIN_WORKFLOW}/dispatches`, {
      method: `POST`,
      body: { ref: inputs.base, inputs, return_run_details: true },
    })
    if (res.status === 404) return { ok: false, missing: true, message: `no ${GITHUB_TRAIN_WORKFLOW} on ${inputs.base}` }
    if (!res.ok) {
      const message = `GitHub returned ${res.status}: ${await githubErrorMessage(res)}`
      // #FEED-223: the installation was never granted Actions (a 403 "not
      // accessible by integration", never a rate limit): this App can run no
      // pipeline here, so the repo has none for the train. It used to error
      // every five minutes forever, the PRs silently `queued`.
      return { ok: false, missing: res.status === 403 && /not accessible by integration/i.test(message), message }
    }
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
    const token = await tokenOf(repo)
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
    const conflict = res.status === 405 && /conflict|not mergeable/i.test(message)
    return { ok: false, conflict, message, reason: githubReason(res.status, message) }
  },
}

