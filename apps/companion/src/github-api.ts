// Thin REST wrapper around the bits of the GitHub API the companion uses.
// Authentication is always via an OAuth user access token obtained through
// github-auth.ts.

export interface GithubUser {
  login: string
  id: number
}

export interface GithubRepoMinimal {
  fullName: string
  defaultBranch: string
  private: boolean
}

export interface GithubPullRequest {
  url: string
  number: number
  state: `open` | `closed`
  merged: boolean
  mergedAt: string | null
  closedAt: string | null
}

const BASE = `https://api.github.com`

interface GhRaw {
  full_name: string
  default_branch: string
  private: boolean
}

interface PrRaw {
  number: number
  html_url: string
  state: `open` | `closed`
  merged: boolean
  merged_at: string | null
  closed_at: string | null
}

async function gh<T>(
  token: string,
  path: string,
  init?: { method?: string; body?: unknown; accept?: string }
): Promise<T> {
  const headers: Record<string, string> = {
    accept: init?.accept ?? `application/vnd.github+json`,
    authorization: `Bearer ${token}`,
    "x-github-api-version": `2022-11-28`,
  }
  if (init?.body !== undefined) headers[`content-type`] = `application/json`
  const res = await fetch(`${BASE}${path}`, {
    method: init?.method ?? `GET`,
    headers,
    body: init?.body !== undefined ? JSON.stringify(init.body) : undefined,
  })
  if (!res.ok) {
    const text = await res.text().catch(() => ``)
    throw new Error(
      `GitHub ${init?.method ?? `GET`} ${path} failed: ${res.status} ${text.slice(0, 500)}`
    )
  }
  return (await res.json()) as T
}

export async function getAuthedUser(token: string): Promise<GithubUser> {
  return gh<GithubUser>(token, `/user`)
}

export async function listAccessibleRepos(
  token: string
): Promise<GithubRepoMinimal[]> {
  // GET /user/repos. Includes repos the user owns, collaborates on, and is
  // a member of through an org. Paginate to 100 per page (the API max) and
  // cap at 1000 results to avoid runaway requests.
  const out: GithubRepoMinimal[] = []
  for (let page = 1; page <= 10; page++) {
    const rows = await gh<GhRaw[]>(
      token,
      `/user/repos?per_page=100&page=${page}&affiliation=owner,collaborator,organization_member&sort=updated`
    )
    for (const r of rows) {
      out.push({
        fullName: r.full_name,
        defaultBranch: r.default_branch,
        private: r.private,
      })
    }
    if (rows.length < 100) break
  }
  return out
}

export async function createPullRequest(
  token: string,
  args: {
    owner: string
    repo: string
    head: string
    base: string
    title: string
    body: string
    draft?: boolean
  }
): Promise<{ url: string; number: number }> {
  const raw = await gh<PrRaw>(token, `/repos/${args.owner}/${args.repo}/pulls`, {
    method: `POST`,
    body: {
      title: args.title,
      head: args.head,
      base: args.base,
      body: args.body,
      draft: args.draft ?? false,
    },
  })
  return { url: raw.html_url, number: raw.number }
}

export async function getPullRequest(
  token: string,
  args: { owner: string; repo: string; number: number }
): Promise<GithubPullRequest> {
  const raw = await gh<PrRaw>(
    token,
    `/repos/${args.owner}/${args.repo}/pulls/${args.number}`
  )
  return {
    url: raw.html_url,
    number: raw.number,
    state: raw.state,
    merged: raw.merged,
    mergedAt: raw.merged_at,
    closedAt: raw.closed_at,
  }
}

/**
 * Parse `https://github.com/owner/repo/pull/123` (or .../pulls/123) into its
 * components. Returns null if the URL doesn't look like a PR URL.
 */
export function parsePrUrl(url: string): {
  owner: string
  repo: string
  number: number
} | null {
  const m = url.match(
    /^https:\/\/github\.com\/([A-Za-z0-9._-]+)\/([A-Za-z0-9._-]+)\/pulls?\/(\d+)/
  )
  if (!m) return null
  return { owner: m[1]!, repo: m[2]!, number: Number(m[3]) }
}
