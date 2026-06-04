import { and, eq, isNotNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { accounts, workspaceAgents } from "@/db/schema"
import { auth } from "@/lib/auth"
import { decryptSecret } from "@/lib/crypto/secret-box"

// A user's connected-GitHub access token (Better Auth `linkSocial`), auto-
// refreshed. This is the primary credential now: the owner connects once in the
// web app and the server uses it for PR creation + diff (and hands it to the
// agent for clone/push). Returns null if the user hasn't connected GitHub.
export async function resolveOwnerGithubToken(
  userId: string
): Promise<string | null> {
  const [account] = await db
    .select({ id: accounts.id })
    .from(accounts)
    .where(and(eq(accounts.userId, userId), eq(accounts.providerId, `github`)))
    .limit(1)
  if (!account) return null
  try {
    const result = await auth.api.getAccessToken({
      body: { providerId: `github`, userId },
    })
    return result.accessToken ?? null
  } catch {
    return null
  }
}

// The GitHub token of whoever owns this workspace's agent (the human who
// registered it) — used when the viewer/actor hasn't personally connected.
export async function resolveWorkspaceAgentOwnerToken(
  workspaceId: string
): Promise<string | null> {
  const rows = await db
    .select({ ownerUserId: workspaceAgents.ownerUserId })
    .from(workspaceAgents)
    .where(
      and(
        eq(workspaceAgents.workspaceId, workspaceId),
        isNotNull(workspaceAgents.ownerUserId)
      )
    )
  for (const row of rows) {
    if (!row.ownerUserId) continue
    const token = await resolveOwnerGithubToken(row.ownerUserId)
    if (token) return token
  }
  return null
}

// Unified repo-token resolution for server-side GitHub calls (PR create, diff):
// the actor's own connected token first, then the workspace agent owner's, then
// the legacy agent-reported token (kept during the device-flow → linkSocial
// transition). GITHUB_TOKEN / unauthenticated is the caller's final fallback.
export async function resolveRepoToken(opts: {
  actorUserId?: string | null
  workspaceId: string
  repo: string
}): Promise<string | null> {
  if (opts.actorUserId) {
    const own = await resolveOwnerGithubToken(opts.actorUserId)
    if (own) return own
  }
  const owner = await resolveWorkspaceAgentOwnerToken(opts.workspaceId)
  if (owner) return owner
  return resolveAgentRepoToken(opts.workspaceId, opts.repo)
}

export interface PullFile {
  filename: string
  status: string
  additions: number
  deletions: number
  patch?: string
}

// Resolve a GitHub token that can read `repo`, from the workspace's registered
// agents (each reports its own token, stored encrypted). Prefer an agent whose
// reported repo list includes this repo; otherwise any agent token in the
// workspace. The agent that opened the PR necessarily had access, so its token
// can read the diff. Returns null if no agent has reported a token.
export async function resolveAgentRepoToken(
  workspaceId: string,
  repo: string
): Promise<string | null> {
  const rows = await db
    .select({
      githubToken: workspaceAgents.githubToken,
      githubRepos: workspaceAgents.githubRepos,
    })
    .from(workspaceAgents)
    .where(
      and(
        eq(workspaceAgents.workspaceId, workspaceId),
        isNotNull(workspaceAgents.githubToken)
      )
    )

  let fallback: string | null = null
  for (const row of rows) {
    const token = row.githubToken ? decryptSecret(row.githubToken) : null
    if (!token) continue
    fallback ??= token
    const repos = Array.isArray(row.githubRepos)
      ? (row.githubRepos as Array<{ fullName?: string }>)
      : []
    if (
      repos.some((r) => r.fullName?.toLowerCase() === repo.toLowerCase())
    ) {
      return token
    }
  }
  return fallback
}

export interface CreatedPull {
  url: string
  number: number
}

// Create a PR server-side (the agent pushes the branch, the server opens the PR
// with the owner's connected token). Throws on any non-2xx with GitHub's message.
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
      accept: `application/vnd.github+json`,
      "user-agent": `exponential`,
      "x-github-api-version": `2022-11-28`,
      authorization: `Bearer ${opts.token}`,
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
  const headers: Record<string, string> = {
    accept: `application/vnd.github+json`,
    "user-agent": `exponential`,
    "x-github-api-version": `2022-11-28`,
  }
  const auth = token || process.env.GITHUB_TOKEN
  if (auth) headers.authorization = `Bearer ${auth}`
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
// Token priority: a `token` passed in (an agent's reported token — covers
// private repos), then the optional `GITHUB_TOKEN` env (a self-hoster PAT),
// then unauthenticated (public repos only). A private repo with no token
// available returns a not-found error, surfaced to the UI.
export async function fetchPullFiles(
  repo: string,
  prNumber: number,
  token?: string | null
): Promise<PullFile[]> {
  const headers: Record<string, string> = {
    accept: `application/vnd.github+json`,
    "user-agent": `exponential`,
    "x-github-api-version": `2022-11-28`,
  }
  const auth = token || process.env.GITHUB_TOKEN
  if (auth) {
    headers.authorization = `Bearer ${auth}`
  }

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
