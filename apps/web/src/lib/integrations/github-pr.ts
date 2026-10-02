import {
  githubApiHeaders,
  resolveRepoInstallationToken,
} from "@/lib/integrations/github-app"
import type { GithubActorRef } from "@/lib/integrations/github-identity"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"

export interface PullFile {
  filename: string
  /** EXP-895: where a `renamed`/`copied` file came from — the shared diff model
   *  carries it as `DiffFile.previousPath`, and the clients print it. */
  previous_filename?: string
  status: string
  additions: number
  deletions: number
  patch?: string
}

// Server-side repo token for GitHub calls (PR create, diff, merge poll): a
// short-lived **GitHub App installation token** scoped to `repo`. Migrated off
// the per-user OAuth token. `teamId`/`actorUserId` are accepted (so the
// call sites don't change) but no longer used — the App resolves the repo's
// installation directly. Null when the App isn't installed on that repo.
export async function resolveRepoToken(opts: {
  actorUserId?: string | null
  teamId?: string
  repo: string
}): Promise<string | null> {
  return resolveRepoInstallationToken(opts.repo)
}

export interface CreatedPull {
  url: string
  number: number
}

// Create a PR server-side (the desktop coding session pushes the branch, the
// server opens the PR with the App installation token). Throws on any non-2xx
// with GitHub's message.
export async function createPullRequest(opts: {
  repo: string
  head: string
  base: string
  title: string
  body: string
  token: string
}): Promise<CreatedPull> {
  const res = await fetch(`https://api.github.com/repos/${opts.repo}/pulls`, {
    method: `POST`,
    headers: {
      ...githubApiHeaders(opts.token),
      "content-type": `application/json`,
    },
    body: JSON.stringify({
      title: opts.title,
      head: opts.head,
      base: opts.base,
      body: opts.body,
    }),
  })
  if (!res.ok) {
    const text = await res.text()
    const message = `GitHub PR create failed (${res.status}): ${text.slice(0, 300)}`
    if (res.status === 422 && text.includes(`A pull request already exists`)) {
      throw new PullAlreadyExistsError(message)
    }
    throw new Error(message)
  }
  const data = (await res.json()) as { html_url: string; number: number }
  return { url: data.html_url, number: data.number }
}

// FEED-59: GitHub's 422 for a head that already has an OPEN PR. `pr_open`
// catches it and links the issues to that PR instead of failing.
export class PullAlreadyExistsError extends Error {}

export interface OpenPullByHead extends CreatedPull {
  baseRef: string
}

// The OPEN PR whose head is `headRef` (same-repo branches only, like
// `listPullsByHead`), or null when there is none. `base` narrows it to the
// PR on that base (the one behind a create's 422).
export async function findOpenPullByHead(
  repo: string,
  headRef: string,
  token?: string | null,
  fetchImpl?: GitHubFetch,
  base?: string
): Promise<OpenPullByHead | null> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const owner = repo.split(`/`)[0]
  const baseParam = base ? `&base=${encodeURIComponent(base)}` : ``
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls?state=open&head=${owner}:${encodeURIComponent(headRef)}${baseParam}&per_page=1`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (!res.ok) {
    throw new Error(
      `GitHub returned ${res.status} listing open pulls by head for ${repo}`
    )
  }
  const data = (await res.json()) as Array<{
    html_url: string
    number: number
    base?: { ref?: string }
  }>
  const pull = data[0]
  if (!pull) return null
  return { url: pull.html_url, number: pull.number, baseRef: pull.base?.ref ?? `` }
}

// GitHub's merge endpoint uses the HTTP status to distinguish failure modes
// (405 not mergeable / method disallowed, 409 head changed, 404 gone), so the
// error carries the status for the caller to map onto user-facing messages.
export class GitHubMergeError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message)
  }
}

export interface MergedPull {
  merged: boolean
  sha: string
}

// Squash-merge a PR server-side with the App installation token (the Reviews
// surfaces merge through the server — clients never touch git/gh locally).
// Throws GitHubMergeError with GitHub's own message on any non-2xx.
export async function mergePullRequest(opts: {
  repo: string
  prNumber: number
  token: string
  commitTitle?: string
}): Promise<MergedPull> {
  const res = await fetch(
    `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}/merge`,
    {
      method: `PUT`,
      headers: {
        ...githubApiHeaders(opts.token),
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        merge_method: `squash`,
        ...(opts.commitTitle !== undefined
          ? { commit_title: opts.commitTitle }
          : {}),
      }),
    }
  )
  if (!res.ok) {
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
  }
  const data = (await res.json()) as { merged: boolean; sha: string }
  return { merged: data.merged, sha: data.sha }
}

// Close a PR WITHOUT merging (the Reviews "reject" path — the work was done
// but the issue got dropped). Same server-side posture as merge: the App
// installation token acts, clients never touch git/gh. Throws
// GitHubMergeError (shared error shape — the status mapping is identical).
export async function closePullRequest(opts: {
  repo: string
  prNumber: number
  token: string
}): Promise<void> {
  const res = await fetch(
    `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
    {
      method: `PATCH`,
      headers: {
        ...githubApiHeaders(opts.token),
        "content-type": `application/json`,
      },
      body: JSON.stringify({ state: `closed` }),
    }
  )
  if (!res.ok) {
    const text = await res.text()
    let message = text.slice(0, 300)
    try {
      const parsed = JSON.parse(text) as { message?: string }
      if (parsed.message) message = parsed.message
    } catch {
      // Non-JSON error body — surface the raw text.
    }
    throw new GitHubMergeError(res.status, message)
  }
}

// EXP-1059: reopen a pull request that was closed WITHOUT merging (the
// workflow's final PR, closed by hand). GitHub refuses (422) when the head
// branch is gone or the PR was merged — the caller decides what to do then.
export async function reopenPullRequest(opts: {
  repo: string
  prNumber: number
  token: string
}): Promise<void> {
  const res = await fetch(
    `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
    {
      method: `PATCH`,
      headers: {
        ...githubApiHeaders(opts.token),
        "content-type": `application/json`,
      },
      body: JSON.stringify({ state: `open` }),
    }
  )
  if (!res.ok) {
    const text = await res.text()
    let message = text.slice(0, 300)
    try {
      const parsed = JSON.parse(text) as { message?: string }
      if (parsed.message) message = parsed.message
    } catch {
      // Non-JSON error body — surface the raw text.
    }
    throw new GitHubMergeError(res.status, message)
  }
}

// Pull-request resolution state (for the merge poller).
export interface PullState {
  state: `open` | `closed`
  merged: boolean
  // EXP-617: whoever pressed Merge, straight out of the response the poller
  // already fetches. Self-hosted instances have no webhook, so this is their
  // ONLY attribution source — without it every polled merge fans out
  // anonymously and reaches the person who merged it. Null on an open PR.
  mergedBy: GithubActorRef | null
  // The PR's live base branch, out of the same response: the GITHUB_POLLING
  // poller mirrors it into `issues.pr_base_branch` without a second read.
  // Null when GitHub omits it.
  baseRef: string | null
  // FEED-64: the squash commit of a merged PR — what the verify-after-failure
  // read hands back in place of the merge response's `sha`. Null on an open
  // PR (GitHub sends a test-merge sha there, deliberately dropped).
  mergeCommitSha: string | null
}

// Fetch a PR's open/closed/merged state (server-side merge detection).
export async function fetchPullState(
  repo: string,
  prNumber: number,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<PullState> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const headers = githubApiHeaders(token || process.env.GITHUB_TOKEN)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls/${prNumber}`,
    { headers }
  )
  if (!res.ok) {
    throw new Error(`GitHub returned ${res.status} for ${repo}#${prNumber}`)
  }
  const data = (await res.json()) as {
    state: string
    merged: boolean
    merged_by?: GithubActorRef | null
    merge_commit_sha?: string | null
    base?: { ref?: string }
  }
  const merged = Boolean(data.merged)
  return {
    state: data.state === `closed` ? `closed` : `open`,
    merged,
    mergedBy: data.merged_by ?? null,
    baseRef: data.base?.ref || null,
    mergeCommitSha: merged ? (data.merge_commit_sha ?? null) : null,
  }
}

// Injectable fetch surface for the stacked-PR helpers below — the real
// `fetch`, or a stub in unit tests (the `fetchBranchDiff` pattern).
export type GitHubFetch = (
  url: string,
  init: {
    method?: string
    headers: Record<string, string>
    body?: string
  }
) => Promise<{
  ok: boolean
  status: number
  // Optional so unit stubs stay two-liners; the real `fetch` always has it,
  // and githubRateLimitMessage needs the x-ratelimit-* headers.
  headers?: { get: (name: string) => string | null }
  text: () => Promise<string>
  json: () => Promise<unknown>
}>

// A single PR's live head/base identity (EXP-324). `fetchPullState` stays the
// poller's lean read; this is the stacked-PR read — the base ref is the part
// the DB deliberately does not persist (GitHub is the source of truth).
export interface PullDetails {
  state: `open` | `closed`
  merged: boolean
  /** A draft can never be merged. */
  draft: boolean
  headRef: string
  baseRef: string
  mergeable: boolean | null
  mergeableState: string | null
  /** EXP-1139: the description the Reviews card shows and `updatePullRequest`
   *  rewrites. GitHub sends `body: null` for an empty one; normalised to ``. */
  title: string
  body: string
  url: string
}

export async function getPullRequest(
  repo: string,
  prNumber: number,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<PullDetails> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls/${prNumber}`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (!res.ok) {
    throw new Error(`GitHub returned ${res.status} for ${repo}#${prNumber}`)
  }
  const data = (await res.json()) as {
    state: string
    merged: boolean
    draft?: boolean
    head?: { ref?: string }
    base?: { ref?: string }
    mergeable?: boolean | null
    mergeable_state?: string
    title?: string
    body?: string | null
    html_url?: string
  }
  return {
    state: data.state === `closed` ? `closed` : `open`,
    merged: Boolean(data.merged),
    draft: Boolean(data.draft),
    headRef: data.head?.ref ?? ``,
    baseRef: data.base?.ref ?? ``,
    mergeable: data.mergeable ?? null,
    mergeableState: data.mergeable_state ?? null,
    title: data.title ?? ``,
    body: data.body ?? ``,
    url: data.html_url ?? `https://github.com/${repo}/pull/${prNumber}`,
  }
}

// EXP-1139: rewrite an open PR's title and/or body — the description a run
// wrote at `pr_open` goes stale the moment a later commit changes the scope,
// and agents never hold a `gh`. An omitted field keeps its value (GitHub's
// PATCH is partial); the caller guarantees at least one is present.
export async function updatePullRequest(opts: {
  repo: string
  prNumber: number
  title?: string
  body?: string
  token: string
  fetchImpl?: GitHubFetch
}): Promise<void> {
  const doFetch = opts.fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const patch: { title?: string; body?: string } = {}
  if (opts.title !== undefined) patch.title = opts.title
  if (opts.body !== undefined) patch.body = opts.body
  const res = await doFetch(
    `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
    {
      method: `PATCH`,
      headers: {
        ...githubApiHeaders(opts.token),
        "content-type": `application/json`,
      },
      body: JSON.stringify(patch),
    }
  )
  if (!res.ok) {
    const text = await res.text()
    let message = text.slice(0, 300)
    try {
      const parsed = JSON.parse(text) as { message?: string }
      if (parsed.message) message = parsed.message
    } catch {
      // Non-JSON error body — surface the raw text.
    }
    throw new GitHubMergeError(res.status, message)
  }
}

// Every PR (any state) whose HEAD is `headRef` — the "does this base branch
// belong to a merged parent PR?" lookup. Same-repo stacking only by design:
// the `owner:` qualifier scopes to the repo's own branches, so a cross-fork
// base finds nothing and classifies as custom/missing (never retargeted).
export async function listPullsByHead(
  repo: string,
  headRef: string,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<Array<{ number: number; state: `open` | `closed`; merged: boolean }>> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const owner = repo.split(`/`)[0]
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls?state=all&head=${owner}:${encodeURIComponent(headRef)}&per_page=30`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (!res.ok) {
    throw new Error(
      `GitHub returned ${res.status} listing pulls by head for ${repo}`
    )
  }
  const data = (await res.json()) as Array<{
    number: number
    state: string
    merged_at?: string | null
  }>
  return data.map((pull) => ({
    number: pull.number,
    state: pull.state === `closed` ? (`closed` as const) : (`open` as const),
    merged: pull.merged_at != null,
  }))
}

// Open PRs BASED on `baseRef` — the children of a stack parent. Used by the
// parent-merge auto-retarget (EXP-324).
export async function listOpenPullsByBase(
  repo: string,
  baseRef: string,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<Array<{ number: number; url: string; headRef: string }>> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls?state=open&base=${encodeURIComponent(baseRef)}&per_page=100`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (!res.ok) {
    throw new Error(
      `GitHub returned ${res.status} listing pulls by base for ${repo}`
    )
  }
  const data = (await res.json()) as Array<{
    number: number
    html_url: string
    head?: { ref?: string }
  }>
  return data.map((pull) => ({
    number: pull.number,
    url: pull.html_url,
    headRef: pull.head?.ref ?? ``,
  }))
}

export async function branchExists(
  repo: string,
  branch: string,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<boolean> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/branches/${encodeURIComponent(branch)}`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (res.ok) return true
  if (res.status === 404) return false
  throw new Error(`GitHub returned ${res.status} for branch ${repo}:${branch}`)
}

// Change a PR's base branch (`PATCH /pulls/{n}` with `{base}`) — the stacked-PR
// self-heal: after a parent is squash-merged its branch is stale, and GitHub
// only auto-retargets children when the base branch is DELETED (we leave it).
// GitHub answers 422 for an invalid/unknown base. Same server-side posture and
// error shape as closePullRequest.
export async function retargetPullRequest(opts: {
  repo: string
  prNumber: number
  base: string
  token: string
  fetchImpl?: GitHubFetch
}): Promise<void> {
  const doFetch = opts.fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
    {
      method: `PATCH`,
      headers: {
        ...githubApiHeaders(opts.token),
        "content-type": `application/json`,
      },
      body: JSON.stringify({ base: opts.base }),
    }
  )
  if (!res.ok) {
    const text = await res.text()
    let message = text.slice(0, 300)
    try {
      const parsed = JSON.parse(text) as { message?: string }
      if (parsed.message) message = parsed.message
    } catch {
      // Non-JSON error body — surface the raw text.
    }
    throw new GitHubMergeError(res.status, message)
  }
}

// What a PR's base ref IS, and what to do about it (EXP-324).
export type PrBaseKind =
  | `default` //        base is the repo default branch — the normal case
  | `open-parent` //    live stack: base is an open PR's head — rebase onto it
  | `merged-parent` //  the EXP-320 shape: parent squash-merged, branch left — retarget
  | `closed-parent` //  parent abandoned unmerged — retarget, keep its commits
  | `custom-branch` //  deliberate long-lived base (release/1.x) — respect it
  | `missing-branch` // base branch is gone — retarget

export interface PrBaseClassification {
  kind: PrBaseKind
  // The ref a conflict-fix run should rebase onto (post-retarget when one
  // applies).
  rebaseOnto: string
  // Non-null ⇒ the PR's base is dead and it should be retargeted here first.
  retargetTo: string | null
  parentPrNumber: number | null
}

// Pure classification of a PR's base ref. `parentPulls` are the PRs whose HEAD
// is the base ref (from listPullsByHead). Precedence open > merged > closed:
// an open parent means a live stack regardless of older recycled-branch PRs.
// A merged parent is detected ONLY through its PR — we squash-merge, so the
// parent's commits are rewritten on landing and content-containment checks
// (compare base...default) can never prove the merge happened.
// Retarget target is always the repo default, no recursion: squash-merge lands
// the parent's content on the default branch, and a deeper stack collapses one
// parent-merge at a time.
export function classifyPrBase(opts: {
  baseRef: string
  defaultBranch: string
  parentPulls: Array<{ number: number; state: `open` | `closed`; merged: boolean }>
  baseBranchExists: boolean
}): PrBaseClassification {
  const { baseRef, defaultBranch, parentPulls, baseBranchExists } = opts
  if (baseRef === defaultBranch) {
    return {
      kind: `default`,
      rebaseOnto: defaultBranch,
      retargetTo: null,
      parentPrNumber: null,
    }
  }
  const open = parentPulls.find((pull) => pull.state === `open`)
  if (open) {
    return {
      kind: `open-parent`,
      rebaseOnto: baseRef,
      retargetTo: null,
      parentPrNumber: open.number,
    }
  }
  const merged = parentPulls.find((pull) => pull.merged)
  if (merged) {
    return {
      kind: `merged-parent`,
      rebaseOnto: defaultBranch,
      retargetTo: defaultBranch,
      parentPrNumber: merged.number,
    }
  }
  const closed = parentPulls.find((pull) => pull.state === `closed`)
  if (closed) {
    return {
      kind: `closed-parent`,
      rebaseOnto: defaultBranch,
      retargetTo: defaultBranch,
      parentPrNumber: closed.number,
    }
  }
  if (baseBranchExists) {
    return {
      kind: `custom-branch`,
      rebaseOnto: baseRef,
      retargetTo: null,
      parentPrNumber: null,
    }
  }
  return {
    kind: `missing-branch`,
    rebaseOnto: defaultBranch,
    retargetTo: defaultBranch,
    parentPrNumber: null,
  }
}

export interface PrBaseState extends PrBaseClassification {
  prState: `open` | `closed`
  merged: boolean
  headRef: string
  baseRef: string
}

// Live read + classification of a PR's base. Short-circuits without extra
// GitHub calls when the base is the default branch; the branch-existence probe
// only runs when no PR ever had the base as its head.
export async function resolvePrBaseState(opts: {
  repo: string
  prNumber: number
  token: string | null
  defaultBranch: string
  fetchImpl?: GitHubFetch
}): Promise<PrBaseState> {
  const { repo, prNumber, token, defaultBranch, fetchImpl } = opts
  const pull = await getPullRequest(repo, prNumber, token, fetchImpl)
  const shared = {
    prState: pull.state,
    merged: pull.merged,
    headRef: pull.headRef,
    baseRef: pull.baseRef,
  }
  if (pull.baseRef === defaultBranch) {
    return {
      ...shared,
      ...classifyPrBase({
        baseRef: pull.baseRef,
        defaultBranch,
        parentPulls: [],
        baseBranchExists: true,
      }),
    }
  }
  const parentPulls = await listPullsByHead(repo, pull.baseRef, token, fetchImpl)
  const baseBranchExists =
    parentPulls.length > 0
      ? true
      : await branchExists(repo, pull.baseRef, token, fetchImpl)
  return {
    ...shared,
    ...classifyPrBase({
      baseRef: pull.baseRef,
      defaultBranch,
      parentPulls,
      baseBranchExists,
    }),
  }
}

// Only these two base kinds mean the two trees actually disagree; every other
// kind is a stale/dead base, which a rebase-and-resolve run cannot fix
// (EXP-533: it is also what gates the clients' "Fix conflicts" button).
export function isContentConflictKind(kind: PrBaseKind): boolean {
  return kind === `default` || kind === `custom-branch`
}

export interface UnmergeableDiagnosis {
  /** The actionable sentence shown to the user (and to MCP agents). */
  message: string
  /** True only for a real content conflict, i.e. rebase-and-resolve helps. */
  conflict: boolean
}

// Turn GitHub's bare "Pull Request is not mergeable" into an actionable
// diagnosis (EXP-324 criterion 3): a stale/merged base is NOT a content
// conflict, and telling the agent to rebase harder sends it in circles
// (exactly what happened on EXP-320). Null on any failure — the caller keeps
// GitHub's original message.
export async function diagnoseUnmergeablePr(opts: {
  repo: string
  prNumber: number
  token: string | null
  defaultBranch: string
  fetchImpl?: GitHubFetch
}): Promise<UnmergeableDiagnosis | null> {
  try {
    const state = await resolvePrBaseState(opts)
    const base = state.baseRef
    const fallback = opts.defaultBranch
    const conflict = isContentConflictKind(state.kind)
    // Message strings are byte-locked: MCP agents read them, and
    // `issues-pr-base.test.ts` keys on them.
    switch (state.kind) {
      case `merged-parent`:
        return {
          conflict,
          message: `Pull Request is not mergeable: its base branch '${base}' is the head of already-merged PR #${state.parentPrNumber}. Retarget this PR to '${fallback}' (call exponential_pr_retarget), rebase onto origin/${fallback} if needed, then retry the merge.`,
        }
      case `closed-parent`:
        return {
          conflict,
          message: `Pull Request is not mergeable: its base branch '${base}' is the head of closed, unmerged PR #${state.parentPrNumber}. Reopen the parent, or retarget this PR to '${fallback}' (call exponential_pr_retarget) and rebase onto origin/${fallback}.`,
        }
      case `missing-branch`:
        return {
          conflict,
          message: `Pull Request is not mergeable: its base branch '${base}' no longer exists. Retarget this PR to '${fallback}' (call exponential_pr_retarget), then retry the merge.`,
        }
      case `open-parent`:
        return {
          conflict,
          message: `Pull Request is not mergeable: it is stacked on open PR #${state.parentPrNumber} (base '${base}'). Merge the parent first, or rebase onto origin/${base} and resolve the conflicts.`,
        }
      case `default`:
      case `custom-branch`:
        return {
          conflict,
          message: `Pull Request has merge conflicts with '${base}': rebase onto origin/${base}, resolve the conflicts, push with --force-with-lease, then retry the merge.`,
        }
    }
  } catch {
    return null
  }
}

export interface OpenPull {
  number: number
  url: string
  title: string
  branch: string
  baseBranch: string
  draft: boolean
  authorLogin: string | null
  authorAvatarUrl: string | null
  createdAt: string
}

// GitHub answers a burnt rate limit with 403 (secondary/primary) or 429 plus
// `x-ratelimit-remaining: 0`. Unauthenticated reads share a 60 req/h bucket per
// IP, so one busy self-hosted instance (or a screenshot lane) burns it for
// everyone and the UI showed a bare "GitHub returned 403". Returns null when
// the response is not a rate limit — the caller keeps its own message.
export function githubRateLimitMessage(
  res: { status: number; headers?: { get: (name: string) => string | null } },
  repo: string,
  authed: boolean
): string | null {
  if (res.status !== 403 && res.status !== 429) return null
  const remaining = res.headers?.get(`x-ratelimit-remaining`)
  if (remaining !== `0`) return null
  const resetRaw = res.headers?.get(`x-ratelimit-reset`)
  const reset = resetRaw ? Number(resetRaw) : Number.NaN
  const when = Number.isFinite(reset)
    ? `Try again in ~${Math.max(1, Math.ceil((reset * 1000 - Date.now()) / 60_000))} min`
    : `Try again later`
  const hint = authed
    ? `.`
    : ` — or set GITHUB_TOKEN on the server for public-repo reads.`
  return `GitHub rate limit reached for ${repo}. ${when}${hint}`
}

// List a repository's open pull requests. The Reviews queue shows every open
// PR of a team's repos — PRs opened outside the issue flow have no
// issues row to sync from, so they must come straight from GitHub. Token
// priority mirrors fetchPullFiles: App installation token, then the optional
// GITHUB_TOKEN env, then unauthenticated (public repos only).
export async function listOpenPulls(
  repo: string,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<OpenPull[]> {
  const authToken = token || process.env.GITHUB_TOKEN
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls?state=open&per_page=100`,
    { headers: githubApiHeaders(authToken) }
  )
  if (!res.ok) {
    throw new Error(
      githubRateLimitMessage(res, repo, Boolean(authToken)) ??
        `GitHub returned ${res.status} listing pulls for ${repo}`
    )
  }
  const data = (await res.json()) as Array<{
    number: number
    html_url: string
    title: string
    draft?: boolean
    created_at: string
    head?: { ref?: string }
    base?: { ref?: string }
    user?: { login?: string; avatar_url?: string }
  }>
  return data.map((pull) => ({
    number: pull.number,
    url: pull.html_url,
    title: pull.title,
    branch: pull.head?.ref ?? ``,
    baseBranch: pull.base?.ref ?? ``,
    draft: Boolean(pull.draft),
    authorLogin: pull.user?.login ?? null,
    authorAvatarUrl: pull.user?.avatar_url ?? null,
    createdAt: pull.created_at,
  }))
}

// Fetch a pull request's changed files from GitHub for the diff view.
//
// Token priority: a `token` passed in (the App installation token — covers
// private repos), then the optional `GITHUB_TOKEN` env (a self-hoster PAT),
// then unauthenticated (public repos only). A private repo with no token
// available returns a not-found error, surfaced to the UI.
//
// Cached for 60s per (repo, PR, auth posture) — the diff view is opened,
// closed and reopened while reviewing, and every native client refetches on
// focus; unauthenticated reads share GitHub's 60 req/h per-IP bucket, so the
// uncached path burnt it and answered 403 (EXP-642). Rejections evict, so a
// transient failure never sticks. The TOKEN is deliberately not part of the
// key (secrets don't belong in cache keys) — only whether one was present,
// which is what changes the visibility of the answer.
const pullFilesCache = new TtlPromiseCache<PullFile[]>({
  ttlMs: 60_000,
  maxEntries: 200,
})

export async function fetchPullFiles(
  repo: string,
  prNumber: number,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<PullFile[]> {
  const authToken = token || process.env.GITHUB_TOKEN
  const key = `${repo}#${prNumber}#${authToken ? `auth` : `anon`}`
  return pullFilesCache.get(key, async () => {
    const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
    const res = await doFetch(
      `https://api.github.com/repos/${repo}/pulls/${prNumber}/files?per_page=100`,
      { headers: githubApiHeaders(authToken) }
    )
    if (!res.ok) {
      throw new Error(
        githubRateLimitMessage(res, repo, Boolean(authToken)) ??
          `GitHub returned ${res.status} for ${repo}#${prNumber}`
      )
    }
    const data = (await res.json()) as PullFile[]
    return data.map((f) => ({
      filename: f.filename,
      previous_filename: f.previous_filename,
      status: f.status,
      additions: f.additions,
      deletions: f.deletions,
      patch: f.patch,
    }))
  })
}

// FEED-43: merge-async pins the preview API version; every other read/write
// stays on the stable one (`githubApiHeaders`).
const GITHUB_ASYNC_MERGE_API_VERSION = `2026-03-10`

function asyncMergeHeaders(token?: string | null): Record<string, string> {
  return {
    ...githubApiHeaders(token || process.env.GITHUB_TOKEN),
    "x-github-api-version": GITHUB_ASYNC_MERGE_API_VERSION,
  }
}

// FEED-64: a 5xx body says nothing ("Server Error"); GitHub's request id is
// what their support can trace, so the message carries it when the response
// exposes headers (the real `fetch` does; unit stubs may not).
async function githubErrorMessage(res: {
  status?: number
  headers?: { get: (name: string) => string | null }
  text: () => Promise<string>
}): Promise<string> {
  const text = await res.text()
  let message = text.slice(0, 300)
  try {
    const parsed = JSON.parse(text) as { message?: string }
    if (parsed.message) message = parsed.message
  } catch {
    // Non-JSON error body — surface the raw text.
  }
  const requestId =
    res.status !== undefined && res.status >= 500
      ? res.headers?.get(`x-github-request-id`)
      : null
  return requestId ? `${message} (request ${requestId})` : message
}

/** The async merge job's state, as GitHub reports it. */
export interface AsyncMergeStatus {
  status: `pending` | `merged` | `enqueued` | `failed`
  message: string | null
  uuid: string | null
  sha: string | null
}

function parseAsyncMerge(
  data: {
    status?: string
    sha?: string
    details?: {
      message?: string
      uuid?: string
      expected_head_sha?: string
    }
  } | null
): AsyncMergeStatus {
  const raw = data?.status
  const status =
    raw === `merged` || raw === `enqueued` || raw === `failed`
      ? raw
      : (`pending` as const)
  return {
    status,
    message: data?.details?.message ?? null,
    uuid: data?.details?.uuid ?? null,
    sha: data?.sha ?? data?.details?.expected_head_sha ?? null,
  }
}

/**
 * `PUT …/pulls/{n}/merge-async` — the ONLY way to merge a PR GitHub holds in
 * a stack (and a valid way to merge any PR). 202 hands back a job; 200 means the
 * merge already happened or is already queued; 409 means a merge is already
 * enqueued for this PR, which for our purposes IS `enqueued` (the work is
 * underway; a second request would only duplicate it).
 */
export async function mergePullRequestAsync(opts: {
  repo: string
  prNumber: number
  token: string
  commitTitle?: string
  sha?: string
  fetchImpl?: GitHubFetch
}): Promise<AsyncMergeStatus> {
  const doFetch = opts.fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}/merge-async`,
    {
      method: `PUT`,
      headers: {
        ...asyncMergeHeaders(opts.token),
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        merge_method: `squash`,
        ...(opts.commitTitle !== undefined
          ? { commit_title: opts.commitTitle }
          : {}),
        ...(opts.sha !== undefined ? { sha: opts.sha } : {}),
      }),
    }
  )
  if (res.status === 409) {
    return {
      status: `enqueued`,
      message: await githubErrorMessage(res),
      uuid: null,
      sha: null,
    }
  }
  if (!res.ok) {
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
  }
  const data = (await res.json().catch(() => null)) as Parameters<
    typeof parseAsyncMerge
  >[0]
  return parseAsyncMerge(data)
}

const ASYNC_MERGE_TIMEOUT_MS = 60_000
const ASYNC_MERGE_STEP_MS = 2_000

const defaultSleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

/**
 * Poll `GET …/pulls/{n}/merge-async/{uuid}` until GitHub stops saying
 * `pending`. Returns the last observed state — a still-`pending` answer at the
 * deadline is the caller's cue to keep its merge claim and tell the user the
 * merge is still running (it is a GitHub-side job; giving up here never
 * cancels it). `sleepImpl` is injected so tests drive the deadline without
 * real time.
 */
export async function pollAsyncMerge(opts: {
  repo: string
  prNumber: number
  uuid: string
  token: string
  timeoutMs?: number
  stepMs?: number
  fetchImpl?: GitHubFetch
  sleepImpl?: (ms: number) => Promise<void>
  nowImpl?: () => number
}): Promise<AsyncMergeStatus> {
  const doFetch = opts.fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const sleep = opts.sleepImpl ?? defaultSleep
  const now = opts.nowImpl ?? (() => Date.now())
  const stepMs = opts.stepMs ?? ASYNC_MERGE_STEP_MS
  const deadline = now() + (opts.timeoutMs ?? ASYNC_MERGE_TIMEOUT_MS)
  let last: AsyncMergeStatus = {
    status: `pending`,
    message: null,
    uuid: opts.uuid,
    sha: null,
  }
  for (;;) {
    // FEED-64: a 5xx or a dropped connection on ONE poll is not a merge
    // outcome — the job keeps running on GitHub — so it counts as "still
    // pending" and the loop goes on to its deadline. A 4xx is GitHub's answer
    // about the job itself and still throws.
    let res: Awaited<ReturnType<GitHubFetch>> | null
    try {
      res = await doFetch(
        `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}/merge-async/${encodeURIComponent(opts.uuid)}`,
        { headers: asyncMergeHeaders(opts.token) }
      )
    } catch (err) {
      // Only a dropped connection is "no answer yet"; anything else is a bug
      // (or a caller's own throw) that must surface instead of burning the
      // deadline one silent step at a time.
      if (!isFetchFailure(err)) throw err
      res = null
    }
    if (res && !res.ok && res.status < 500) {
      throw new GitHubMergeError(res.status, await githubErrorMessage(res))
    }
    if (res?.ok) {
      const data = (await res.json().catch(() => null)) as Parameters<
        typeof parseAsyncMerge
      >[0]
      last = { ...parseAsyncMerge(data), uuid: opts.uuid }
      if (last.status !== `pending`) return last
    }
    if (now() + stepMs > deadline) return last
    await sleep(stepMs)
  }
}

/**
 * FEED-64: a request that never got GitHub's answer — undici/Bun throw a
 * `TypeError` ("fetch failed"), a timed-out or aborted request an
 * `AbortError`/`TimeoutError`, a socket error carries an `ECONN*`/
 * `ETIMEDOUT`/`EAI_*`/`EPIPE`/`UND_ERR_*` code (sometimes one level down, on
 * `cause`). Anything else is a programming error, not a network one.
 */
export function isFetchFailure(err: unknown): boolean {
  if (err instanceof TypeError) return true
  if (!(err instanceof Error)) return false
  if (err.name === `AbortError` || err.name === `TimeoutError`) return true
  const code = (err as { code?: unknown }).code
  if (typeof code === `string` && /^(ECONN|ETIMEDOUT|EAI_|EPIPE|UND_ERR)/.test(code)) {
    return true
  }
  const cause = (err as { cause?: unknown }).cause
  return cause instanceof Error && cause !== err && isFetchFailure(cause)
}

/**
 * The merge-async job is still running at our deadline. NOT a failure: the
 * merge may land seconds later, so the caller keeps its actor claim and says
 * so instead of reporting an error GitHub never returned.
 */
export class GitHubAsyncMergePending extends Error {
  constructor(
    public prNumber: number,
    public uuid: string | null
  ) {
    super(`GitHub is still merging PR #${prNumber}`)
  }
}

/** GitHub's refusal to merge a stacked PR through the legacy endpoint. */
export const STACKED_PR_REFUSAL = /stacked PRs?/i

export function isStackedPrRefusal(err: unknown): boolean {
  return (
    err instanceof GitHubMergeError &&
    (err.status === 405 || err.status === 422) &&
    STACKED_PR_REFUSAL.test(err.message)
  )
}

/**
 * GitHub's 405 "unmergeable" refusal — the only 405 worth a base diagnosis.
 *
 * GitHub has shipped two wordings for the same state: the classic
 * `Pull Request is not mergeable` and, since 2026, the more specific
 * `Pull Request has merge conflicts` (EXP-737: that one slipped through as a
 * verbatim 412 policy refusal, so no client offered "Fix conflicts" on a real
 * conflict). Both mean the trees disagree; policy refusals ("Squash merges are
 * not allowed…", required reviews/checks) and the transient "Base branch was
 * modified" use neither phrase. FEED-64: it is ALSO what a merged PR answers
 * to a second merge call, which is why the smart merge verifies before
 * passing it on.
 */
export const UNMERGEABLE_405 = /not mergeable|merge conflicts?/i

/**
 * FEED-64: the merge call failed WITHOUT GitHub deciding against the merge —
 * a 5xx (GitHub's `PUT …/merge` is known to answer 500 "Server Error" after
 * it already wrote the squash commit) or no answer at all (a dropped
 * connection). The PR's real state has to be read before that counts as
 * "not merged". A 4xx is GitHub's decision and never verified here; the
 * async-pending signal is a state of its own, never "unknown".
 */
export function isMergeOutcomeUnknown(err: unknown): boolean {
  if (err instanceof GitHubAsyncMergePending) return false
  if (!(err instanceof GitHubMergeError)) return true
  return err.status >= 500
}

function isUnmergeable405(err: unknown): boolean {
  return (
    err instanceof GitHubMergeError &&
    err.status === 405 &&
    UNMERGEABLE_405.test(err.message)
  )
}

/**
 * FEED-64: the PR's own state after a merge call that did not answer
 * "refused". Two reads a beat apart, because GitHub can still be writing the
 * merge when the 502/504 arrives; a read that itself fails answers null, and
 * the caller then reports the ORIGINAL error — this never masks one.
 * `settled`: GitHub DID answer about the PR (the 405 an already-merged PR
 * gives a retry, which a real conflict shares), so a first read that shows it
 * open is final: no second read, no sleep on a genuine conflict.
 */
async function confirmMergedDespiteError(opts: {
  repo: string
  prNumber: number
  token: string
  fetchImpl?: GitHubFetch
  sleepImpl?: (ms: number) => Promise<void>
  settled?: boolean
}): Promise<{ sha: string | null; mergedBy: GithubActorRef | null } | null> {
  const sleep = opts.sleepImpl ?? defaultSleep
  for (let attempt = 0; attempt < 2; attempt++) {
    if (attempt > 0) await sleep(VERIFY_MERGE_RETRY_MS)
    let state: PullState
    try {
      state = await fetchPullState(
        opts.repo,
        opts.prNumber,
        opts.token,
        opts.fetchImpl
      )
    } catch {
      return null
    }
    if (state.merged) {
      return { sha: state.mergeCommitSha, mergedBy: state.mergedBy }
    }
    if (opts.settled && state.state === `open`) return null
  }
  return null
}

const VERIFY_MERGE_RETRY_MS = 1_000

/**
 * FEED-64: a verified merge's `merged_by` names a PERSON — the merge was
 * theirs (pressed on github.com while our call failed for a real reason),
 * never this call's. The caller then drops its actor claims and leaves the
 * bookkeeping to the webhook, which attributes it to them (EXP-617). Our own
 * App acts as `<slug>[bot]`; mirrors `isBotActor` (github-identity.ts, which
 * this module must not import — it opens the db connection at import).
 */
export function mergedByPerson(actor: GithubActorRef | null): boolean {
  if (!actor || actor.type === `Bot`) return false
  const login = actor.login?.trim().toLowerCase()
  return Boolean(login) && !login!.endsWith(`[bot]`)
}

/** The note a caller hands back for a merge that turned out to be a person's. */
export function mergedByPersonNote(
  prNumber: number,
  actor: GithubActorRef | null
): string {
  const who = actor?.login ? ` by ${actor.login}` : ``
  return `PR #${prNumber} was already merged on GitHub${who}; its issues complete when the merge webhook lands.`
}

export interface SmartMergeResult {
  merged: boolean
  /** The merge is running on GitHub's merge queue — success, not yet landed. */
  queued: boolean
  sha: string | null
  /** FEED-64: set ONLY when the merge was confirmed from the PR's state after
   *  the merge call failed — GitHub's `merged_by`. A person there means the
   *  merge was theirs, not this call's (the caller leaves the bookkeeping to
   *  the webhook, attributed to them). Null on the normal path. */
  mergedBy: GithubActorRef | null
}

/**
 * The ONE merge entry point (FEED-43): a legacy squash `PUT …/merge`, retried
 * through `merge-async` + poll when GitHub refuses it as a stacked PR. Any PR
 * that merge also lands is closed by its own `pull_request.closed` webhook
 * (or the poller), never here.
 */
export async function mergePullRequestSmart(opts: {
  repo: string
  prNumber: number
  token: string
  commitTitle?: string
  fetchImpl?: GitHubFetch
  sleepImpl?: (ms: number) => Promise<void>
  timeoutMs?: number
  stepMs?: number
}): Promise<SmartMergeResult> {
  const { repo, prNumber, token, commitTitle } = opts
  const verifyOpts = {
    repo,
    prNumber,
    token,
    ...(opts.fetchImpl ? { fetchImpl: opts.fetchImpl } : {}),
    ...(opts.sleepImpl ? { sleepImpl: opts.sleepImpl } : {}),
  }
  try {
    const merged = await mergePullRequest({
      repo,
      prNumber,
      token,
      ...(commitTitle !== undefined ? { commitTitle } : {}),
    })
    return { merged: merged.merged, queued: false, sha: merged.sha, mergedBy: null }
  } catch (err) {
    if (!isStackedPrRefusal(err)) {
      // FEED-64: a 5xx/dropped answer, or the 405 an already-merged PR
      // gives a retry — read the PR before calling either a failure.
      if (isMergeOutcomeUnknown(err) || isUnmergeable405(err)) {
        const confirmed = await confirmMergedDespiteError({
          ...verifyOpts,
          // A 405 is GitHub's answer about the PR: one open read settles it.
          settled: !isMergeOutcomeUnknown(err),
        })
        if (confirmed) {
          return {
            merged: true,
            queued: false,
            sha: confirmed.sha,
            mergedBy: confirmed.mergedBy,
          }
        }
      }
      throw err
    }
  }

  let started: AsyncMergeStatus
  try {
    started = await mergePullRequestAsync({
      repo,
      prNumber,
      token,
      ...(commitTitle !== undefined ? { commitTitle } : {}),
      ...(opts.fetchImpl ? { fetchImpl: opts.fetchImpl } : {}),
    })
  } catch (err) {
    // FEED-64: same verification as the legacy call — merge-async can land
    // the merge and still answer 5xx.
    if (isMergeOutcomeUnknown(err)) {
      const confirmed = await confirmMergedDespiteError(verifyOpts)
      if (confirmed) {
        return {
          merged: true,
          queued: false,
          sha: confirmed.sha,
          mergedBy: confirmed.mergedBy,
        }
      }
    }
    throw err
  }
  let state = started
  if (state.status === `pending` && state.uuid) {
    state = await pollAsyncMerge({
      repo,
      prNumber,
      uuid: state.uuid,
      token,
      ...(opts.fetchImpl ? { fetchImpl: opts.fetchImpl } : {}),
      ...(opts.sleepImpl ? { sleepImpl: opts.sleepImpl } : {}),
      ...(opts.timeoutMs !== undefined ? { timeoutMs: opts.timeoutMs } : {}),
      ...(opts.stepMs !== undefined ? { stepMs: opts.stepMs } : {}),
    })
  }
  if (state.status === `failed`) {
    // Same shape as a legacy refusal so `prMergeFailureError` maps it onto the
    // conflict/precondition split every client already gates on.
    throw new GitHubMergeError(
      405,
      state.message ?? `GitHub could not merge PR #${prNumber}`
    )
  }
  if (state.status === `pending`) {
    throw new GitHubAsyncMergePending(prNumber, state.uuid)
  }
  return {
    merged: true,
    queued: state.status === `enqueued`,
    sha: state.sha,
    mergedBy: null,
  }
}
