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
    throw new Error(
      `GitHub PR create failed (${res.status}): ${text.slice(0, 300)}`
    )
  }
  const data = (await res.json()) as { html_url: string; number: number }
  return { url: data.html_url, number: data.number }
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

// Pull-request resolution state (for the merge poller).
export interface PullState {
  state: `open` | `closed`
  merged: boolean
  // EXP-617: whoever pressed Merge, straight out of the response the poller
  // already fetches. Self-hosted instances have no webhook, so this is their
  // ONLY attribution source — without it every polled merge fans out
  // anonymously and reaches the person who merged it. Null on an open PR.
  mergedBy: GithubActorRef | null
}

// Fetch a PR's open/closed/merged state (server-side merge detection).
export async function fetchPullState(
  repo: string,
  prNumber: number,
  token?: string | null
): Promise<PullState> {
  const headers = githubApiHeaders(token || process.env.GITHUB_TOKEN)
  const res = await fetch(
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
  }
  return {
    state: data.state === `closed` ? `closed` : `open`,
    merged: Boolean(data.merged),
    mergedBy: data.merged_by ?? null,
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
  /** EXP-897: a draft can never be merged — the stack pre-flight refuses on it. */
  draft: boolean
  headRef: string
  baseRef: string
  mergeable: boolean | null
  mergeableState: string | null
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
  }
  return {
    state: data.state === `closed` ? `closed` : `open`,
    merged: Boolean(data.merged),
    draft: Boolean(data.draft),
    headRef: data.head?.ref ?? ``,
    baseRef: data.base?.ref ?? ``,
    mergeable: data.mergeable ?? null,
    mergeableState: data.mergeable_state ?? null,
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

// ── GitHub PR stacks (EXP-897 / FEED-43) ─────────────────────────────────────
//
// GitHub's stacked pull requests (public preview since 2026-07-30, same-repo
// only) are a FIRST-CLASS server object: a stack has its own number, an
// ordered member list (bottom → top) and its own merge endpoint. Two facts
// drive everything below.
//   1. A stack member CANNOT be merged with the legacy `PUT …/merge` — GitHub
//      answers "Merging stacked PRs via this endpoint is not supported. Use
//      the asynchronous merge endpoint instead." (FEED-43: that refusal is
//      what broke every Exponential merge path once a stack existed).
//   2. A stack member's base is MANAGED by the stack — `PATCH /pulls/{n}`
//      with a `{base}` is refused 422, and GitHub retargets the member above
//      itself when the one below merges.
// Every endpoint here may answer 404 on an instance/repo where the preview is
// off; that degrades SILENTLY to "no stack" so a plain base-branch PR keeps
// working (the `pr_open` degrade rule).
const GITHUB_STACK_API_VERSION = `2026-03-10`

// The stack endpoints are the ONLY calls that pin the preview API version —
// `githubApiHeaders` stays on the stable one for every other read/write.
export function githubStackHeaders(token?: string | null): Record<string, string> {
  return {
    ...githubApiHeaders(token || process.env.GITHUB_TOKEN),
    "x-github-api-version": GITHUB_STACK_API_VERSION,
  }
}

export interface StackMember {
  number: number
  state: `open` | `closed`
  draft: boolean
  merged: boolean
  headRef: string
  headSha: string | null
}

export interface PullStack {
  id: string | number | null
  /** The `{stack_number}` path segment of `/stacks/{n}/add|unstack`. */
  number: number
  /** The branch the BOTTOM member targets — where the whole stack lands. */
  baseRef: string
  open: boolean
  /** Bottom → top, exactly as GitHub orders them. */
  members: StackMember[]
}

interface RawStack {
  id?: string | number
  number?: number
  base?: { ref?: string }
  open?: boolean
  pull_requests?: Array<{
    number?: number
    state?: string
    draft?: boolean
    merged_at?: string | null
    head?: { ref?: string; sha?: string }
  }>
}

function parseStack(raw: RawStack): PullStack | null {
  if (raw.number == null) return null
  return {
    id: raw.id ?? null,
    number: raw.number,
    baseRef: raw.base?.ref ?? ``,
    open: raw.open !== false,
    members: (raw.pull_requests ?? [])
      .filter((pull): pull is { number: number } & typeof pull =>
        pull.number != null
      )
      .map((pull) => ({
        number: pull.number,
        state: pull.state === `closed` ? (`closed` as const) : (`open` as const),
        draft: pull.draft === true,
        merged: pull.merged_at != null,
        headRef: pull.head?.ref ?? ``,
        headSha: pull.head?.sha ?? null,
      })),
  }
}

async function githubErrorMessage(res: {
  text: () => Promise<string>
}): Promise<string> {
  const text = await res.text()
  try {
    const parsed = JSON.parse(text) as { message?: string }
    if (parsed.message) return parsed.message
  } catch {
    // Non-JSON error body — surface the raw text.
  }
  return text.slice(0, 300)
}

/** The stack a PR belongs to, or null (no stack / preview unavailable). */
export async function findStackForPull(
  repo: string,
  prNumber: number,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<PullStack | null> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/stacks?pull_request=${prNumber}`,
    { headers: githubStackHeaders(token) }
  )
  // 404 = the preview is off on this repo (or the PR is gone) — "no stack".
  if (res.status === 404) return null
  if (!res.ok) {
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
  }
  const data = (await res.json()) as RawStack[] | RawStack | null
  const list = Array.isArray(data) ? data : data ? [data] : []
  for (const raw of list) {
    const stack = parseStack(raw)
    if (stack) return stack
  }
  return null
}

/**
 * Create a stack out of an ordered PR list (bottom → top). Null when GitHub
 * refuses the chain (422 — each member's base must equal the previous head)
 * or the preview is unavailable (404): the caller degrades to a plain
 * base-branch PR.
 */
export async function createStack(
  repo: string,
  prNumbers: number[],
  token: string,
  fetchImpl?: GitHubFetch
): Promise<PullStack | null> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(`https://api.github.com/repos/${repo}/stacks`, {
    method: `POST`,
    headers: {
      ...githubStackHeaders(token),
      "content-type": `application/json`,
    },
    body: JSON.stringify({ pull_requests: prNumbers }),
  })
  if (res.status === 404 || res.status === 422) return null
  if (!res.ok) {
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
  }
  return parseStack((await res.json()) as RawStack)
}

/**
 * Extend an existing stack with more PRs (top-most last). 409 means another
 * writer touched the stack concurrently — retried ONCE, because the loser of
 * that race is usually a second agent adding its own PR and the second
 * attempt lands on the refreshed stack. Null on 404/422 (degrade).
 */
export async function addToStack(
  repo: string,
  stackNumber: number,
  prNumbers: number[],
  token: string,
  fetchImpl?: GitHubFetch
): Promise<PullStack | null> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const call = async () =>
    doFetch(
      `https://api.github.com/repos/${repo}/stacks/${stackNumber}/add`,
      {
        method: `POST`,
        headers: {
          ...githubStackHeaders(token),
          "content-type": `application/json`,
        },
        body: JSON.stringify({ pull_requests: prNumbers }),
      }
    )
  let res = await call()
  if (res.status === 409) res = await call()
  if (res.status === 404 || res.status === 422 || res.status === 409) {
    return null
  }
  if (!res.ok) {
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
  }
  return parseStack((await res.json()) as RawStack)
}

/**
 * Dissolve a stack (200 = some members remain stacked, 204 = fully
 * dissolved). False on 404/422 — nothing to dissolve, or GitHub refused.
 */
export async function unstack(
  repo: string,
  stackNumber: number,
  token: string,
  fetchImpl?: GitHubFetch
): Promise<boolean> {
  const doFetch = fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/stacks/${stackNumber}/unstack`,
    { method: `POST`, headers: githubStackHeaders(token) }
  )
  if (res.status === 404 || res.status === 422) return false
  if (!res.ok) {
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
  }
  return true
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
 * `PUT …/pulls/{n}/merge-async` — the ONLY way to merge a stack member (and a
 * perfectly valid way to merge any PR). 202 hands back a job; 200 means the
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
        ...githubStackHeaders(opts.token),
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
    const res = await doFetch(
      `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}/merge-async/${encodeURIComponent(opts.uuid)}`,
      { headers: githubStackHeaders(opts.token) }
    )
    if (!res.ok) {
      throw new GitHubMergeError(res.status, await githubErrorMessage(res))
    }
    const data = (await res.json().catch(() => null)) as Parameters<
      typeof parseAsyncMerge
    >[0]
    last = { ...parseAsyncMerge(data), uuid: opts.uuid }
    if (last.status !== `pending`) return last
    if (now() + stepMs > deadline) return last
    await sleep(stepMs)
  }
}

/**
 * The merge-async job is still running at our deadline. NOT a failure: the
 * merge may land seconds later, so the caller keeps its actor claim and says
 * so instead of reporting an error GitHub never returned.
 */
export class GitHubAsyncMergePending extends Error {
  constructor(
    public prNumber: number,
    public stackNumber: number | null,
    public uuid: string | null
  ) {
    super(`GitHub is still merging PR #${prNumber}`)
  }
}

/** GitHub's refusal to merge a stack member through the legacy endpoint. */
export const STACKED_PR_REFUSAL = /stacked PRs?/i

export interface SmartMergeResult {
  merged: boolean
  /** The merge is running on GitHub's merge queue — success, not yet landed. */
  queued: boolean
  sha: string | null
  viaStack: boolean
  stackNumber: number | null
  /** Every member at-or-below the merged one (bottom → top, incl. itself). */
  stackMemberNumbers: number[]
}

/**
 * The ONE merge entry point for a PR that MIGHT be stacked (FEED-43).
 *
 * Legacy `PUT …/merge` first — it is the cheaper call and the overwhelmingly
 * common case — unless the row already knows a stack number. GitHub's
 * stacked-PR refusal (405/422 naming "stacked PRs") is not an error here: it
 * is the discovery that this PR is a stack member, so the call is retried
 * through `merge-async` + poll. Merging member k merges every unmerged member
 * BELOW it atomically, which is why `stackMemberNumbers` comes back — the
 * caller completes those issues too.
 */
export async function mergePullRequestSmart(opts: {
  repo: string
  prNumber: number
  token: string
  commitTitle?: string
  /** `issues.pr_stack_number` — skips the legacy attempt when set. */
  knownStackNumber?: number | null
  fetchImpl?: GitHubFetch
  sleepImpl?: (ms: number) => Promise<void>
  timeoutMs?: number
  stepMs?: number
}): Promise<SmartMergeResult> {
  const { repo, prNumber, token, commitTitle } = opts
  if (opts.knownStackNumber == null) {
    try {
      const merged = await mergePullRequest({
        repo,
        prNumber,
        token,
        ...(commitTitle !== undefined ? { commitTitle } : {}),
      })
      return {
        merged: merged.merged,
        queued: false,
        sha: merged.sha,
        viaStack: false,
        stackNumber: null,
        stackMemberNumbers: [prNumber],
      }
    } catch (err) {
      if (!isStackedRefusal(err)) throw err
      // Fall through: this PR is a stack member after all.
    }
  }

  const stack = await findStackForPull(
    repo,
    prNumber,
    token,
    opts.fetchImpl
  ).catch(() => null)
  const stackNumber = stack?.number ?? opts.knownStackNumber ?? null
  const atOrBelow = stack
    ? membersAtOrBelowNumber(stack, prNumber)
    : [prNumber]

  const started = await mergePullRequestAsync({
    repo,
    prNumber,
    token,
    ...(commitTitle !== undefined ? { commitTitle } : {}),
    ...(opts.fetchImpl ? { fetchImpl: opts.fetchImpl } : {}),
  })
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
    throw new GitHubAsyncMergePending(prNumber, stackNumber, state.uuid)
  }
  return {
    merged: true,
    queued: state.status === `enqueued`,
    sha: state.sha,
    viaStack: true,
    stackNumber,
    stackMemberNumbers: atOrBelow,
  }
}

function isStackedRefusal(err: unknown): boolean {
  return (
    err instanceof GitHubMergeError &&
    (err.status === 405 || err.status === 422) &&
    STACKED_PR_REFUSAL.test(err.message)
  )
}

/** The stack's members at or below `prNumber` (bottom → top, inclusive). */
export function membersAtOrBelowNumber(
  stack: PullStack,
  prNumber: number
): number[] {
  const index = stack.members.findIndex(
    (member) => member.number === prNumber
  )
  if (index < 0) return [prNumber]
  return stack.members.slice(0, index + 1).map((member) => member.number)
}
