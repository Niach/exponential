import crypto from "node:crypto"
import type { PullFile } from "@/lib/integrations/github-pr"

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
// setup route), letting the callback distinguish an in-dialog install (popup,
// self-closing landing page) from a plain full-page install.
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

// The GitHub REST header triple every call shares — `accept`, `user-agent`, and
// the pinned API version. Pass a token to add the `authorization: Bearer` header
// (App JWT or installation token, per call site).
export function githubApiHeaders(token?: string): Record<string, string> {
  const headers: Record<string, string> = {
    accept: `application/vnd.github+json`,
    "user-agent": `exponential`,
    "x-github-api-version": `2022-11-28`,
  }
  if (token) headers.authorization = `Bearer ${token}`
  return headers
}

async function ghApp(path: string, init?: RequestInit): Promise<Response> {
  return fetch(`https://api.github.com${path}`, {
    ...init,
    headers: {
      ...githubApiHeaders(appJwt()),
      ...(init?.headers ?? {}),
    },
  })
}

// repo "owner/name" → installation id, or null if the App isn't installed there.
// Exported for the connect-path authorization check (the repo's installation
// must be one the caller is attributed to).
export async function installationIdForRepo(
  repo: string
): Promise<number | null> {
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
// on `repo` ("owner/name"). Null when the App isn't configured or the repo
// can't be resolved to any installation the App can mint a token for.
//
// `fallbackInstallationId` is the installation persisted on the `repositories`
// row at connect time (`repositories.installation_id`, verified authoritative
// by `connectRepositoryInTx`). GitHub's per-repo installation endpoint
// (`GET /repos/{repo}/installation`) intermittently reports 404 "not installed"
// even when the App still covers the repo through a known installation — e.g.
// the App is installed on several accounts (org + user), or the repo's owner
// login differs from the account the visible install is attributed to. Observed
// in prod: workspace settings shows the repo connected, yet the desktop token
// mint 412s. When the live lookup misses we fall back to minting against the
// stored installation id; only when THAT also fails has the App genuinely lost
// access (→ the caller's actionable "reconnect" 412).
export async function resolveRepoInstallationToken(
  repo: string,
  opts?: { fallbackInstallationId?: number | null }
): Promise<string | null> {
  if (!githubAppConfigured()) return null
  return resolveInstallationTokenWith(repo, opts?.fallbackInstallationId, {
    resolveId: installationIdForRepo,
    mintToken: installationToken,
  })
}

// Pure resolution policy behind `resolveRepoInstallationToken`, with the two
// GitHub round-trips injected so it stays unit-testable without a real App JWT.
// Prefers GitHub's authoritative per-repo installation lookup (handles
// transfers/renames/re-installs); falls back to a known installation id when
// that lookup 404s. A throw on the LIVE path propagates (a transient GitHub
// error must not masquerade as "not installed"); a throw on the FALLBACK mint
// is swallowed to null — that path is only reached after the live lookup
// already said "not installed", so a null (→ 412) is the correct outcome.
export async function resolveInstallationTokenWith(
  repo: string,
  fallbackInstallationId: number | null | undefined,
  ops: {
    resolveId: (repo: string) => Promise<number | null>
    mintToken: (installationId: number) => Promise<string>
  }
): Promise<string | null> {
  const liveId = await ops.resolveId(repo)
  if (liveId != null) return ops.mintToken(liveId)
  if (fallbackInstallationId != null) {
    try {
      return await ops.mintToken(fallbackInstallationId)
    } catch {
      return null
    }
  }
  return null
}

// GitHub's authoritative default branch for a repo ("owner/name"), or null if
// unknown (App not configured, not installed, repo gone). Used to override a
// stale/misseeded `repositories.defaultBranch` at token-mint time so the
// launcher's `git worktree add … origin/<default>` can't fail on a wrong ref
// (e.g. a row seeded `main` for a `master` repo).
// NOTE: must authenticate with an **installation token**, not the App JWT —
// GitHub only accepts the JWT on app-management endpoints, so a JWT-authed
// `GET /repos/{repo}` always 401s and the heal silently no-ops.
export async function resolveRepoDefaultBranch(
  repo: string
): Promise<string | null> {
  if (!githubAppConfigured()) return null
  try {
    const token = await resolveRepoInstallationToken(repo)
    if (!token) return null
    const res = await fetch(`https://api.github.com/repos/${repo}`, {
      headers: githubApiHeaders(token),
    })
    if (!res.ok) return null
    const data = (await res.json()) as { default_branch?: string }
    return data.default_branch ?? null
  } catch {
    return null
  }
}

// A short in-process cache over `resolveRepoDefaultBranch`, keyed by repo. Fan-out
// callers (`repositories.list` heals every row on every read) must not hammer
// GitHub — a ~5 min TTL absorbs the churn while still healing shortly after an
// install/rename. Both hits and misses are cached so a repo the App can't reach
// doesn't cost a round-trip per list. `now` is injectable for tests. Mirrors the
// branch-diff cache's in-process-map style.
const DEFAULT_BRANCH_TTL_MS = 5 * 60_000
const defaultBranchCache = new Map<string, { at: number; value: string | null }>()

export async function resolveRepoDefaultBranchCached(
  repo: string,
  now: number = Date.now()
): Promise<string | null> {
  const cached = defaultBranchCache.get(repo)
  if (cached && now - cached.at < DEFAULT_BRANCH_TTL_MS) return cached.value
  const value = await resolveRepoDefaultBranch(repo)
  defaultBranchCache.set(repo, { at: now, value })
  return value
}

// A changed file in a branch/PR diff reuses github-pr.ts's `PullFile` so every
// client renders one diff shape across the PR-diff and pushed-branch-no-PR tiers
// (§4.8). Re-exported for callers that reach for it via this module.
export type { PullFile }

// The `prFiles`-shaped result of a branch compare. `prNumber` is always null
// here (there is no PR yet — this is the "pushed, no PR" tier); it stays in the
// shape so clients can render it exactly like the PR-diff tier.
export interface BranchDiff {
  repo: string
  prNumber: number | null
  files: PullFile[]
}

// Injectable fetch surface — the real `fetch`, or a stub in unit tests.
export type CompareFetch = (
  url: string,
  init: { headers: Record<string, string> }
) => Promise<{ ok: boolean; status: number; json: () => Promise<unknown> }>

// GitHub's compare API is stable within a short window and the diff view polls
// (§4.8 freshness) — a ~60s per-branch cache absorbs the churn. Module-level so
// it survives across requests, keyed by `repo#base#branch` (the base ref is part
// of the compare identity — a default-branch change must not serve a stale diff).
// Only successful compares are cached; a 404 (branch not pushed) is never cached
// so a manual Refresh re-checks immediately once the branch lands. Mirrors the
// widget rate-limiter's in-process-map style.
const BRANCH_DIFF_TTL_MS = 60_000
const branchDiffCache = new Map<string, { at: number; value: BranchDiff }>()

function branchDiffKey(repo: string, base: string, branch: string): string {
  return `${repo}#${base}#${branch}`
}

// Warm-cache peek: the fresh cached diff for `<base>...<branch>`, or null when
// there's no live entry. Callers use this to short-circuit BEFORE resolving a
// GitHub installation token (the token/installation lookups are uncached), so a
// cache hit costs zero GitHub round-trips.
export function peekBranchDiff(
  repo: string,
  base: string,
  branch: string,
  now: number = Date.now()
): BranchDiff | null {
  const cached = branchDiffCache.get(branchDiffKey(repo, base, branch))
  if (cached && now - cached.at < BRANCH_DIFF_TTL_MS) return cached.value
  return null
}

// Compare `<base>...<branch>` on GitHub and return the changed files in the
// shared diff shape. Returns null when the branch was never pushed (GitHub 404).
// The installation `token` covers private repos; a self-hoster `GITHUB_TOKEN`
// is the fallback. `now`/`fetchImpl` are injectable for tests.
export async function fetchBranchDiff(opts: {
  repo: string // "owner/name"
  base: string // default branch
  branch: string // e.g. "exp/EXP-42"
  token: string | null
  now?: number
  fetchImpl?: CompareFetch
}): Promise<BranchDiff | null> {
  const { repo, base, branch, token } = opts
  const now = opts.now ?? Date.now()
  const doFetch = opts.fetchImpl ?? (globalThis.fetch as unknown as CompareFetch)

  const cached = peekBranchDiff(repo, base, branch, now)
  if (cached) return cached

  const headers = githubApiHeaders(token || process.env.GITHUB_TOKEN)

  const url = `https://api.github.com/repos/${repo}/compare/${base}...${branch}?per_page=100`
  const res = await doFetch(url, { headers })
  if (res.status === 404) {
    // Branch not pushed (or gone) — never cache the miss so a manual Refresh
    // re-checks immediately once the branch is pushed.
    return null
  }
  if (!res.ok) {
    throw new Error(
      `GitHub compare failed (${res.status}) for ${repo} ${base}...${branch}`
    )
  }
  const data = (await res.json()) as {
    files?: Array<{
      filename: string
      status: string
      additions: number
      deletions: number
      patch?: string
    }>
  }
  const value: BranchDiff = {
    repo,
    prNumber: null,
    files: (data.files ?? []).map((f) => ({
      filename: f.filename,
      status: f.status,
      additions: f.additions,
      deletions: f.deletions,
      patch: f.patch,
    })),
  }
  branchDiffCache.set(branchDiffKey(repo, base, branch), { at: now, value })
  return value
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
    { headers: githubApiHeaders(token) }
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
