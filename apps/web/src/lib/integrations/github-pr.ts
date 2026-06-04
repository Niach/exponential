import { and, eq, isNotNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { workspaceAgents } from "@/db/schema"
import { decryptSecret } from "@/lib/crypto/secret-box"

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
