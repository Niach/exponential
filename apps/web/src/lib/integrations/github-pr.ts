import {
  githubApiHeaders,
  resolveRepoInstallationToken,
} from "@/lib/integrations/github-app"

export interface PullFile {
  filename: string
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
  const data = (await res.json()) as { state: string; merged: boolean }
  return {
    state: data.state === `closed` ? `closed` : `open`,
    merged: Boolean(data.merged),
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

// List a repository's open pull requests. The Reviews queue shows every open
// PR of a team's repos — PRs opened outside the issue flow have no
// issues row to sync from, so they must come straight from GitHub. Token
// priority mirrors fetchPullFiles: App installation token, then the optional
// GITHUB_TOKEN env, then unauthenticated (public repos only).
export async function listOpenPulls(
  repo: string,
  token?: string | null
): Promise<OpenPull[]> {
  const headers = githubApiHeaders(token || process.env.GITHUB_TOKEN)
  const res = await fetch(
    `https://api.github.com/repos/${repo}/pulls?state=open&per_page=100`,
    { headers }
  )
  if (!res.ok) {
    throw new Error(`GitHub returned ${res.status} listing pulls for ${repo}`)
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
export async function fetchPullFiles(
  repo: string,
  prNumber: number,
  token?: string | null
): Promise<PullFile[]> {
  const headers = githubApiHeaders(token || process.env.GITHUB_TOKEN)
  const url = `https://api.github.com/repos/${repo}/pulls/${prNumber}/files?per_page=100`
  const res = await fetch(url, { headers })
  if (!res.ok) {
    throw new Error(`GitHub returned ${res.status} for ${repo}#${prNumber}`)
  }
  const data = (await res.json()) as PullFile[]
  return data.map((f) => ({
    filename: f.filename,
    status: f.status,
    additions: f.additions,
    deletions: f.deletions,
    patch: f.patch,
  }))
}
