import { githubFetch } from "@/lib/integrations/github-client"
import {
  githubApiHeaders,
  resolveRepoInstallationToken,
} from "@/lib/integrations/github-app"
import {
  formatGithubError,
  githubErrorMessage,
} from "@/lib/integrations/github-error"
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
  /** EXP-1291: open it as a GitHub draft. */
  draft?: boolean
  fetchImpl?: GitHubFetch
}): Promise<CreatedPull> {
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(`https://api.github.com/repos/${opts.repo}/pulls`, {
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
      ...(opts.draft ? { draft: true } : {}),
    }),
  })
  if (!res.ok) {
    const text = await res.text()
    // FEED-74: GitHub's own words (`Validation Failed: head invalid`, or
    // `empty response body (request …)` on the 500 that sends nothing).
    const message = `GitHub PR create failed (${res.status}): ${formatGithubError({ status: res.status, headers: res.headers, text })}`
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
  const doFetch = fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const owner = repo.split(`/`)[0]
  const baseParam = base ? `&base=${encodeURIComponent(base)}` : ``
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls?state=open&head=${owner}:${encodeURIComponent(headRef)}${baseParam}&per_page=1`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (!res.ok) {
    throw new Error(
      `GitHub returned ${res.status} listing open pulls by head for ${repo}: ${await githubErrorMessage(res)}`
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

// Close a PR WITHOUT merging (the Reviews "reject" path — the work was done
// but the issue got dropped). Same server-side posture as merge: the App
// installation token acts, clients never touch git/gh. Throws
// GitHubMergeError (shared error shape — the status mapping is identical).
export async function closePullRequest(opts: {
  repo: string
  prNumber: number
  token: string
  fetchImpl?: GitHubFetch
}): Promise<void> {
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(`https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
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
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
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
  const res = await githubFetch(`https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
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
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
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
  // EXP-1608: the PR's head commit, out of the same response: the poller
  // repairs checks at that sha (and when a push left them at an older one).
  headSha?: string | null
  // EXP-1705: when GitHub merged it (the backfill records the real time).
  mergedAt?: Date | null
}

// Fetch a PR's open/closed/merged state (server-side merge detection).
export async function fetchPullState(
  repo: string,
  prNumber: number,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<PullState> {
  const doFetch = fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const headers = githubApiHeaders(token || process.env.GITHUB_TOKEN)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls/${prNumber}`,
    { headers }
  )
  if (!res.ok) {
    throw new Error(
      `GitHub returned ${res.status} for ${repo}#${prNumber}: ${await githubErrorMessage(res)}`
    )
  }
  const data = (await res.json()) as {
    state: string
    merged: boolean
    merged_by?: GithubActorRef | null
    merged_at?: string | null
    merge_commit_sha?: string | null
    base?: { ref?: string }
    head?: { sha?: string }
  }
  const merged = Boolean(data.merged)
  return {
    state: data.state === `closed` ? `closed` : `open`,
    merged,
    mergedBy: data.merged_by ?? null,
    baseRef: data.base?.ref || null,
    mergeCommitSha: merged ? (data.merge_commit_sha ?? null) : null,
    headSha: data.head?.sha || null,
    mergedAt: merged && data.merged_at ? new Date(data.merged_at) : null,
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
  /** The head commit. */
  headSha: string
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
  const doFetch = fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls/${prNumber}`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (!res.ok) {
    throw new Error(
      `GitHub returned ${res.status} for ${repo}#${prNumber}: ${await githubErrorMessage(res)}`
    )
  }
  const data = (await res.json()) as {
    state: string
    merged: boolean
    draft?: boolean
    head?: { ref?: string; sha?: string }
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
    headSha: data.head?.sha ?? ``,
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
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
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
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
  }
}

// EXP-1291: flip a PR between draft and ready for review. REST cannot
// (PATCH ignores `draft`); GraphQL's two mutations take the PR's node id.
export async function setPullRequestDraft(opts: {
  repo: string
  prNumber: number
  draft: boolean
  token: string
  fetchImpl?: GitHubFetch
}): Promise<void> {
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const read = await doFetch(
    `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
    { headers: githubApiHeaders(opts.token) }
  )
  if (!read.ok) {
    throw new GitHubMergeError(read.status, await githubErrorMessage(read))
  }
  const pull = (await read.json()) as { node_id?: string; draft?: boolean }
  if (!pull.node_id) throw new GitHubMergeError(502, `GitHub sent no node id for the pull request`)
  if (Boolean(pull.draft) === opts.draft) return
  const mutation = opts.draft
    ? `mutation($id: ID!) { convertPullRequestToDraft(input: { pullRequestId: $id }) { clientMutationId } }`
    : `mutation($id: ID!) { markPullRequestReadyForReview(input: { pullRequestId: $id }) { clientMutationId } }`
  const res = await doFetch(`https://api.github.com/graphql`, {
    method: `POST`,
    headers: { ...githubApiHeaders(opts.token), "content-type": `application/json` },
    body: JSON.stringify({ query: mutation, variables: { id: pull.node_id } }),
  })
  if (!res.ok) throw new GitHubMergeError(res.status, await githubErrorMessage(res))
  const data = (await res.json()) as { errors?: Array<{ message?: string }> }
  if (data.errors?.length) {
    throw new GitHubMergeError(422, data.errors.map((e) => e.message ?? `error`).join(`; `))
  }
}

// Open PRs BASED on `baseRef` — the children of a stack parent. Used by the
// parent-merge auto-retarget (EXP-324).
export async function listOpenPullsByBase(
  repo: string,
  baseRef: string,
  token?: string | null,
  fetchImpl?: GitHubFetch
): Promise<Array<{ number: number; url: string; headRef: string }>> {
  const doFetch = fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls?state=open&base=${encodeURIComponent(baseRef)}&per_page=100`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (!res.ok) {
    throw new Error(
      `GitHub returned ${res.status} listing pulls by base for ${repo}: ${await githubErrorMessage(res)}`
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
  const doFetch = fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/branches/${encodeURIComponent(branch)}`,
    { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) }
  )
  if (res.ok) return true
  if (res.status === 404) return false
  throw new Error(
    `GitHub returned ${res.status} for branch ${repo}:${branch}: ${await githubErrorMessage(res)}`
  )
}

// NEW-5: delete a branch (`DELETE /git/refs/heads/{branch}`) — a merged PR's
// head. `gone` = GitHub no longer has it (404, or the 422 "Reference does not
// exist"), which is the goal anyway. Throws on anything else.
export async function deleteBranch(opts: {
  repo: string
  branch: string
  token: string
  fetchImpl?: GitHubFetch
}): Promise<`deleted` | `gone`> {
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const ref = opts.branch.split(`/`).map(encodeURIComponent).join(`/`)
  const res = await doFetch(`https://api.github.com/repos/${opts.repo}/git/refs/heads/${ref}`, {
    method: `DELETE`,
    headers: githubApiHeaders(opts.token),
  })
  if (res.ok) return `deleted`
  if (res.status === 404 || res.status === 422) return `gone`
  throw new Error(
    `GitHub returned ${res.status} deleting branch ${opts.repo}:${opts.branch}: ${await githubErrorMessage(res)}`
  )
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
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
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
    throw new GitHubMergeError(res.status, await githubErrorMessage(res))
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
  const doFetch = fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${repo}/pulls?state=open&per_page=100`,
    { headers: githubApiHeaders(authToken) }
  )
  if (!res.ok) {
    throw new Error(
      githubRateLimitMessage(res, repo, Boolean(authToken)) ??
        `GitHub returned ${res.status} listing pulls for ${repo}: ${await githubErrorMessage(res)}`
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

/** `fresh` skips (and refreshes) the 60s cache: a reader that must see a
 *  push from seconds ago (the Guide's diff check, EXP-1251). */
export async function fetchPullFiles(
  repo: string,
  prNumber: number,
  token?: string | null,
  fetchImpl?: GitHubFetch,
  opts?: { fresh?: boolean }
): Promise<PullFile[]> {
  const authToken = token || process.env.GITHUB_TOKEN
  const key = `${repo}#${prNumber}#${authToken ? `auth` : `anon`}`
  if (opts?.fresh) pullFilesCache.delete(key)
  return pullFilesCache.get(key, async () => {
    const doFetch = fetchImpl ?? (githubFetch as unknown as GitHubFetch)
    const res = await doFetch(
      `https://api.github.com/repos/${repo}/pulls/${prNumber}/files?per_page=100`,
      { headers: githubApiHeaders(authToken) }
    )
    if (!res.ok) {
      throw new Error(
        githubRateLimitMessage(res, repo, Boolean(authToken)) ??
          `GitHub returned ${res.status} for ${repo}#${prNumber}: ${await githubErrorMessage(res)}`
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

// FEED-43: the stacks API pins the preview API version; every other
// read/write stays on the stable one (`githubApiHeaders`).
const GITHUB_STACKS_API_VERSION = `2026-03-10`

function stacksApiHeaders(token?: string | null): Record<string, string> {
  return {
    ...githubApiHeaders(token || process.env.GITHUB_TOKEN),
    "x-github-api-version": GITHUB_STACKS_API_VERSION,
  }
}

// ── GitHub native stacks (EXP-1248) ─────────────────────────────────────────
// REST `/repos/{r}/stacks` (API version 2026-03-10). EXP-1709: no PR joins
// one any more; the train only reads a PR's stack and leaves it (unstack)
// so the PR takes a new base. Every failure throws `GitHubStackError`.

export class GitHubStackError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message)
  }
}

/** A PR's own `stack` field: where it sits, 1-based from the bottom. */
export interface PullStackRef {
  number: number
  size: number
  position: number
  baseRef: string
}

async function stackRequest(
  opts: { repo: string; token: string; fetchImpl?: GitHubFetch },
  path: string,
  what: string,
  init?: { method: string; body?: unknown }
): Promise<unknown> {
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${opts.repo}/stacks${path}`,
    {
      method: init?.method ?? `GET`,
      headers: {
        ...stacksApiHeaders(opts.token),
        ...(init?.body !== undefined ? { "content-type": `application/json` } : {}),
      },
      ...(init?.body !== undefined ? { body: JSON.stringify(init.body) } : {}),
    }
  )
  if (!res.ok) {
    throw new GitHubStackError(
      res.status,
      `GitHub could not ${what} (${res.status}): ${await githubErrorMessage(res)}`
    )
  }
  return res.json().catch(() => null)
}

/** #EXP-1711: take every unmerged member out of stack `stackNumber`
 *  (`POST /stacks/{n}/unstack`). A PR in a stack refuses a new base. */
export async function unstack(opts: {
  repo: string
  stackNumber: number
  token: string
  fetchImpl?: GitHubFetch
}): Promise<void> {
  await stackRequest(opts, `/${opts.stackNumber}/unstack`, `unstack stack ${opts.stackNumber}`, { method: `POST` })
}

/** The PR's `stack` field (null = in no native stack), read with the API
 *  version that carries it. */
export async function fetchPullStack(opts: {
  repo: string
  prNumber: number
  token: string
  fetchImpl?: GitHubFetch
}): Promise<PullStackRef | null> {
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
    { headers: stacksApiHeaders(opts.token) }
  )
  if (!res.ok) {
    throw new Error(
      `GitHub returned ${res.status} for ${opts.repo}#${opts.prNumber}: ${await githubErrorMessage(res)}`
    )
  }
  const data = (await res.json()) as {
    stack?: {
      number?: number
      size?: number
      position?: number
      base?: { ref?: string }
    } | null
  }
  const stack = data.stack
  if (!stack || stack.number == null) return null
  return {
    number: stack.number,
    size: stack.size ?? 0,
    position: stack.position ?? 0,
    baseRef: stack.base?.ref ?? ``,
  }
}

export interface RefComparison {
  status: `ahead` | `behind` | `identical` | `diverged`
  aheadBy: number
  behindBy: number
}


/** A failed compare, with GitHub's status (a 5xx is GitHub's hiccup). */
export class GitHubCompareError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message)
  }
}

/** `GET /compare/{base}...{head}`; null when either ref is gone (404). */
export async function compareRefs(opts: {
  repo: string
  base: string
  head: string
  token: string
  fetchImpl?: GitHubFetch
}): Promise<RefComparison | null> {
  const doFetch = opts.fetchImpl ?? (githubFetch as unknown as GitHubFetch)
  const res = await doFetch(
    `https://api.github.com/repos/${opts.repo}/compare/${encodeURIComponent(opts.base)}...${encodeURIComponent(opts.head)}?per_page=1`,
    { headers: githubApiHeaders(opts.token) }
  )
  if (res.status === 404) return null
  if (!res.ok) {
    throw new GitHubCompareError(
      res.status,
      `GitHub returned ${res.status} comparing ${opts.base}...${opts.head} in ${opts.repo}: ${await githubErrorMessage(res)}`
    )
  }
  const data = (await res.json()) as {
    status?: string
    ahead_by?: number
    behind_by?: number
  }
  const status =
    data.status === `ahead` ||
    data.status === `behind` ||
    data.status === `identical`
      ? data.status
      : (`diverged` as const)
  return { status, aheadBy: data.ahead_by ?? 0, behindBy: data.behind_by ?? 0 }
}
