import crypto from "node:crypto"

// GitHub App auth: mint a short-lived **installation token** for a repo, scoped
// to exactly the permissions the App was granted (contents + pull_requests).
// This replaces the per-user OAuth token. The App's private key is stored
// base64-encoded (env-safe) in GITHUB_APP_PRIVATE_KEY.

const APP_ID = process.env.GITHUB_APP_ID
const APP_SLUG = process.env.GITHUB_APP_SLUG
const PRIVATE_KEY_B64 = process.env.GITHUB_APP_PRIVATE_KEY

export function githubAppConfigured(): boolean {
  return Boolean(APP_ID && PRIVATE_KEY_B64)
}

// `state` is echoed back to the App's Setup URL (our /api/integrations/github/
// setup route), letting the callback distinguish an in-dialog install from the
// standalone /account/integrations flow.
export function githubAppInstallUrl(state?: string): string | null {
  if (!APP_SLUG) return null
  const base = `https://github.com/apps/${APP_SLUG}/installations/new`
  return state ? `${base}?state=${encodeURIComponent(state)}` : base
}

function privateKeyPem(): string {
  // Stored base64 so a multi-line PEM survives env vars / compose / .env.
  return Buffer.from(PRIVATE_KEY_B64!, `base64`).toString(`utf8`)
}

function b64url(buf: Buffer): string {
  return buf.toString(`base64url`)
}

// App JWT (RS256, iss = app id), valid ~9 min. `crypto.createPrivateKey` accepts
// GitHub's PKCS#1 (`BEGIN RSA PRIVATE KEY`) key directly.
function appJwt(): string {
  const now = Math.floor(Date.now() / 1000)
  const header = b64url(Buffer.from(JSON.stringify({ alg: `RS256`, typ: `JWT` })))
  const payload = b64url(
    Buffer.from(JSON.stringify({ iat: now - 60, exp: now + 9 * 60, iss: APP_ID }))
  )
  const signingInput = `${header}.${payload}`
  const sig = crypto.sign(
    `RSA-SHA256`,
    Buffer.from(signingInput),
    crypto.createPrivateKey(privateKeyPem())
  )
  return `${signingInput}.${b64url(sig)}`
}

async function ghApp(path: string, init?: RequestInit): Promise<Response> {
  return fetch(`https://api.github.com${path}`, {
    ...init,
    headers: {
      accept: `application/vnd.github+json`,
      "user-agent": `exponential`,
      "x-github-api-version": `2022-11-28`,
      authorization: `Bearer ${appJwt()}`,
      ...(init?.headers ?? {}),
    },
  })
}

// repo "owner/name" → installation id, or null if the App isn't installed there.
async function installationIdForRepo(repo: string): Promise<number | null> {
  const res = await ghApp(`/repos/${repo}/installation`)
  if (res.status === 404) return null
  if (!res.ok) {
    throw new Error(`GitHub installation lookup failed (${res.status}) for ${repo}`)
  }
  const data = (await res.json()) as { id: number }
  return data.id
}

// Per-installation token cache (GitHub installation tokens last 1h).
const tokenCache = new Map<number, { token: string; expiresAt: number }>()

async function installationToken(installationId: number): Promise<string> {
  const cached = tokenCache.get(installationId)
  if (cached && cached.expiresAt - 60_000 > Date.now()) return cached.token
  const res = await ghApp(`/app/installations/${installationId}/access_tokens`, {
    method: `POST`,
  })
  if (!res.ok) {
    throw new Error(`GitHub installation token failed (${res.status})`)
  }
  const data = (await res.json()) as { token: string; expires_at: string }
  tokenCache.set(installationId, {
    token: data.token,
    expiresAt: new Date(data.expires_at).getTime(),
  })
  return data.token
}

// The main entry: a short-lived, repo-scoped token that can clone/push/open PRs
// on `repo` ("owner/name"). Null when the App isn't configured or not installed
// on that repo.
export async function resolveRepoInstallationToken(
  repo: string
): Promise<string | null> {
  if (!githubAppConfigured()) return null
  const id = await installationIdForRepo(repo)
  if (id == null) return null
  return installationToken(id)
}

// GitHub's authoritative default branch for a repo ("owner/name"), or null if
// unknown (App not configured, not installed, repo gone). Used to override a
// stale/misseeded `repositories.defaultBranch` at token-mint time so the
// launcher's `git worktree add … origin/<default>` can't fail on a wrong ref
// (e.g. a row seeded `main` for a `master` repo).
export async function resolveRepoDefaultBranch(
  repo: string
): Promise<string | null> {
  if (!githubAppConfigured()) return null
  const res = await ghApp(`/repos/${repo}`)
  if (!res.ok) return null
  const data = (await res.json()) as { default_branch?: string }
  return data.default_branch ?? null
}

export interface AppInstallation {
  id: number
  account: string
  accountType: string
}

// One installation's account info (used by the install setup route).
export async function getInstallation(
  installationId: number
): Promise<AppInstallation | null> {
  if (!githubAppConfigured()) return null
  const res = await ghApp(`/app/installations/${installationId}`)
  if (!res.ok) return null
  const i = (await res.json()) as {
    id: number
    account: { login: string; type: string }
  }
  return {
    id: i.id,
    account: i.account?.login ?? ``,
    accountType: i.account?.type ?? ``,
  }
}

export interface InstallationRepo {
  fullName: string // "owner/name"
  private: boolean
  defaultBranch: string
  installationId: number
}

// One page of the repos an installation can access. Uses the **installation
// token** (not the App JWT) — a distinct auth from `ghApp`. Paginated so the
// caller (the repo picker) can lazy-load rather than fan out across huge orgs.
export async function listInstallationRepos(
  installationId: number,
  page = 1,
  perPage = 100
): Promise<{ repos: InstallationRepo[]; hasMore: boolean }> {
  if (!githubAppConfigured()) return { repos: [], hasMore: false }
  const token = await installationToken(installationId)
  const res = await fetch(
    `https://api.github.com/installation/repositories?per_page=${perPage}&page=${page}`,
    {
      headers: {
        accept: `application/vnd.github+json`,
        "user-agent": `exponential`,
        "x-github-api-version": `2022-11-28`,
        authorization: `Bearer ${token}`,
      },
    }
  )
  if (!res.ok) {
    throw new Error(`GitHub repo list failed (${res.status})`)
  }
  const data = (await res.json()) as {
    total_count: number
    repositories: Array<{
      full_name: string
      private: boolean
      default_branch: string
    }>
  }
  return {
    repos: data.repositories.map((r) => ({
      fullName: r.full_name,
      private: r.private,
      defaultBranch: r.default_branch,
      installationId,
    })),
    hasMore: page * perPage < data.total_count,
  }
}

// All installations of this App (used to reflect install state in the UI).
export async function listAppInstallations(): Promise<AppInstallation[]> {
  if (!githubAppConfigured()) return []
  const res = await ghApp(`/app/installations?per_page=100`)
  if (!res.ok) return []
  const data = (await res.json()) as Array<{
    id: number
    account: { login: string; type: string }
  }>
  return data.map((i) => ({
    id: i.id,
    account: i.account?.login ?? ``,
    accountType: i.account?.type ?? ``,
  }))
}
