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
// the per-user OAuth token. `workspaceId`/`actorUserId` are accepted (so the
// call sites don't change) but no longer used — the App resolves the repo's
// installation directly. Null when the App isn't installed on that repo.
export async function resolveRepoToken(opts: {
  actorUserId?: string | null
  workspaceId?: string
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
