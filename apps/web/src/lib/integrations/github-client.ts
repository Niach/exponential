import crypto from "node:crypto"

// EXP-1609: the ONE client every GitHub REST call goes through. Before it,
// every call site was a bare `fetch("https://api.github.com…")`: nothing
// retried, nothing honoured `retry-after`, and all of the server's PR calls
// plus every agent sharing an installation burnt one 5,000/h budget blind.
//
// - Conditional GETs: a repeated GET sends `If-None-Match` with the ETag of
//   the last 200 and a 304 is served from that stored body (GitHub does not
//   count a 304 against the budget). Keyed per budget (installation) + URL —
//   installation tokens rotate hourly, the answer for a URL does not.
// - Retries: a 429, or a 403 carrying `x-ratelimit-remaining: 0` or GitHub's
//   secondary-limit message, is retried after `retry-after` (or the reset)
//   when that is short, else with bounded exponential backoff + jitter. A
//   limit that outlasts the retries throws ONE typed error,
//   `GithubRateLimited { resetAt }`, which reaches MCP as its message.
// - Budget: every response's `x-ratelimit-remaining/limit/reset` is recorded
//   per installation. The PR poller and the async-merge poll ask
//   `githubBudgetLow` and back off; the admin dashboard shows the numbers.
//
// Pure apart from `fetch` (no db import) — github-app/github-pr import it.

export const GITHUB_API = `https://api.github.com`

/** A GitHub rate limit that outlasted the client's retries. `resetAt` is
 *  GitHub's reset (or retry-after) when it said one, else null. */
export class GithubRateLimited extends Error {
  readonly resetAt: Date | null
  readonly secondary: boolean
  constructor(opts: { resetAt: Date | null; secondary: boolean; now?: number }) {
    const now = opts.now ?? Date.now()
    const when = opts.resetAt
      ? `It resets at ${opts.resetAt.toISOString().slice(11, 16)} UTC (in ~${Math.max(1, Math.ceil((opts.resetAt.getTime() - now) / 60_000))} min)`
      : `Try again in a few minutes`
    super(
      `GitHub rate limit reached${opts.secondary ? ` (secondary limit)` : ``}. ${when}. Do not retry before then and never poll api.github.com yourself: Exponential wakes your session when CI finishes.`
    )
    this.name = `GithubRateLimited`
    this.resetAt = opts.resetAt
    this.secondary = opts.secondary
  }
}

export function isGithubRateLimited(err: unknown): err is GithubRateLimited {
  return err instanceof GithubRateLimited
}

// ── budget ────────────────────────────────────────────────────────────────

export interface GithubBudget {
  /** `installation:<id>` | `app` | `anon` | `token:<hash8>` */
  key: string
  remaining: number
  limit: number
  /** epoch ms */
  resetAt: number
  /** epoch ms of the response that reported it */
  observedAt: number
}

const budgets = new Map<string, GithubBudget>()

// Installation tokens are minted in github-app; it tells us which
// installation a token belongs to so the budget is per installation.
const tokenInstallation = new Map<string, number>()
const TOKEN_REGISTRY_MAX = 500

export function registerGithubToken(token: string, installationId: number): void {
  if (tokenInstallation.size >= TOKEN_REGISTRY_MAX) {
    const oldest = tokenInstallation.keys().next().value
    if (oldest !== undefined) tokenInstallation.delete(oldest)
  }
  tokenInstallation.set(token, installationId)
}

function hash8(value: string): string {
  return crypto.createHash(`sha256`).update(value).digest(`hex`).slice(0, 8)
}

/** Which budget a token draws on. App JWTs (three dot-separated parts) share
 *  the App's own budget; an unknown token (a self-hoster's GITHUB_TOKEN, a
 *  user's OAuth token) gets its own hashed key — never the token itself. */
export function githubBudgetKey(token: string | null | undefined): string {
  if (!token) return `anon`
  const installation = tokenInstallation.get(token)
  if (installation !== undefined) return `installation:${installation}`
  if (token.split(`.`).length === 3) return `app`
  return `token:${hash8(token)}`
}

function tokenFromHeaders(headers: Record<string, string> | undefined): string | null {
  const auth = headers?.authorization ?? headers?.Authorization
  if (!auth) return null
  return auth.replace(/^(Bearer|token)\s+/i, ``)
}

function recordBudget(
  key: string,
  headers: { get: (name: string) => string | null } | undefined,
  now: number
): void {
  const remaining = Number(headers?.get(`x-ratelimit-remaining`) ?? Number.NaN)
  const limit = Number(headers?.get(`x-ratelimit-limit`) ?? Number.NaN)
  const reset = Number(headers?.get(`x-ratelimit-reset`) ?? Number.NaN)
  if (!Number.isFinite(remaining) || !Number.isFinite(reset)) return
  budgets.set(key, {
    key,
    remaining,
    limit: Number.isFinite(limit) ? limit : remaining,
    resetAt: reset * 1000,
    observedAt: now,
  })
}

/** Low = under 10% of the limit (at least 100 calls) with the reset still
 *  ahead. Below it the background loops back off; interactive calls still go. */
export function githubBudgetLow(
  key: string,
  now: number = Date.now()
): boolean {
  const budget = budgets.get(key)
  if (!budget || budget.resetAt <= now) return false
  return budget.remaining < Math.max(100, budget.limit * 0.1)
}

export function githubBudgetLowForToken(
  token: string | null | undefined,
  now: number = Date.now()
): boolean {
  return githubBudgetLow(githubBudgetKey(token), now)
}

/** Every budget seen, freshest first (the admin dashboard's numbers). A
 *  budget whose reset passed is reported as full again. */
export function githubBudgets(now: number = Date.now()): GithubBudget[] {
  return [...budgets.values()]
    .map((budget) =>
      budget.resetAt <= now ? { ...budget, remaining: budget.limit } : budget
    )
    .sort((a, b) => b.observedAt - a.observedAt)
}

// ── conditional GET cache ─────────────────────────────────────────────────

interface EtagEntry {
  etag: string
  body: string
  headers: Array<[string, string]>
}

const ETAG_MAX = 500
const etags = new Map<string, EtagEntry>()

function etagGet(key: string): EtagEntry | undefined {
  const entry = etags.get(key)
  if (entry) {
    // LRU touch
    etags.delete(key)
    etags.set(key, entry)
  }
  return entry
}

function etagSet(key: string, entry: EtagEntry): void {
  etags.delete(key)
  etags.set(key, entry)
  while (etags.size > ETAG_MAX) {
    const oldest = etags.keys().next().value
    if (oldest === undefined) break
    etags.delete(oldest)
  }
}

// ── the client ────────────────────────────────────────────────────────────

export interface GithubFetchInit {
  method?: string
  headers?: Record<string, string>
  body?: string
}

export interface GithubClientDeps {
  fetch?: (url: string, init: RequestInit) => Promise<Response>
  sleep?: (ms: number) => Promise<void>
  now?: () => number
  random?: () => number
}

const MAX_RETRIES = 3
const BACKOFF_BASE_MS = 1_000
const BACKOFF_MAX_MS = 8_000
/** A wait longer than this is not slept through: the call throws
 *  GithubRateLimited and the caller decides. */
const MAX_WAIT_MS = 10_000

const defaultSleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

type Classified =
  | { kind: `ok` }
  | { kind: `limited`; waitMs: number | null; resetAt: Date | null; secondary: boolean }
  | { kind: `transient` }

// Unit stubs of `fetch` hand back plain objects; the client never assumes
// more than ok/status, and reads headers and the body defensively.
type LooseResponse = Response

function header(res: LooseResponse, name: string): string | null {
  return typeof res.headers?.get === `function` ? res.headers.get(name) : null
}

async function peekText(res: LooseResponse): Promise<string> {
  try {
    if (typeof res.clone === `function`) return await res.clone().text()
  } catch {
    // unreadable = empty
  }
  return ``
}

async function classify(res: LooseResponse, method: string, now: number): Promise<Classified> {
  if (res.status !== 403 && res.status !== 429 && res.status < 500) return { kind: `ok` }
  if (res.status >= 500) {
    // Only idempotent reads are retried on a 5xx; a write's outcome is unknown.
    return method === `GET` && res.status !== 501 ? { kind: `transient` } : { kind: `ok` }
  }
  const remaining = header(res, `x-ratelimit-remaining`)
  const retryAfter = Number(header(res, `retry-after`) ?? Number.NaN)
  const secondary = /secondary rate limit|abuse detection/i.test(await peekText(res))
  // A 403 with budget left, no retry-after and no secondary-limit text is a
  // real permission refusal: the caller's to handle.
  if (res.status === 403 && remaining !== `0` && !Number.isFinite(retryAfter) && !secondary) {
    return { kind: `ok` }
  }
  if (Number.isFinite(retryAfter)) {
    const waitMs = Math.max(0, retryAfter * 1000)
    return { kind: `limited`, waitMs, resetAt: new Date(now + waitMs), secondary }
  }
  const reset = Number(header(res, `x-ratelimit-reset`) ?? Number.NaN)
  if (remaining === `0` && Number.isFinite(reset)) {
    const waitMs = Math.max(0, reset * 1000 - now)
    return { kind: `limited`, waitMs, resetAt: new Date(reset * 1000), secondary }
  }
  return { kind: `limited`, waitMs: null, resetAt: null, secondary }
}

function backoffMs(attempt: number, random: () => number): number {
  const ceiling = Math.min(BACKOFF_MAX_MS, BACKOFF_BASE_MS * 2 ** attempt)
  // full jitter over the upper half: never 0, never a thundering herd
  return Math.round(ceiling / 2 + random() * (ceiling / 2))
}

/** `fetch` for GitHub's REST API. `pathOrUrl` is a full URL or a path
 *  (`/repos/…`). Resolves with GitHub's response (a 304 is turned back into
 *  the cached 200); throws GithubRateLimited when a limit outlasts the
 *  retries, and the network error when the last attempt never got an answer. */
export async function githubFetch(
  pathOrUrl: string,
  init: GithubFetchInit = {},
  deps: GithubClientDeps = {}
): Promise<Response> {
  const doFetch = deps.fetch ?? ((url: string, i: RequestInit) => fetch(url, i))
  const sleep = deps.sleep ?? defaultSleep
  const now = deps.now ?? (() => Date.now())
  const random = deps.random ?? Math.random
  const url = pathOrUrl.startsWith(`http`) ? pathOrUrl : `${GITHUB_API}${pathOrUrl}`
  const method = (init.method ?? `GET`).toUpperCase()
  const budgetKey = githubBudgetKey(tokenFromHeaders(init.headers))
  const conditional = method === `GET` && init.body === undefined
  const etagKey = `${budgetKey}|${init.headers?.[`x-github-api-version`] ?? ``}|${url}`

  for (let attempt = 0; ; attempt++) {
    const cached = conditional ? etagGet(etagKey) : undefined
    const headers: Record<string, string> = { ...(init.headers ?? {}) }
    if (cached) headers[`if-none-match`] = cached.etag
    let res: Response
    try {
      res = await doFetch(url, { method, headers, ...(init.body !== undefined ? { body: init.body } : {}) })
    } catch (err) {
      if (method === `GET` && attempt < MAX_RETRIES - 1 && isNetworkError(err)) {
        await sleep(backoffMs(attempt, random))
        continue
      }
      throw err
    }
    recordBudget(budgetKey, typeof res.headers?.get === `function` ? res.headers : undefined, now())

    if (res.status === 304 && cached) {
      return new Response(cached.body, { status: 200, headers: cached.headers })
    }

    const verdict = await classify(res, method, now())
    if (verdict.kind === `ok`) {
      if (conditional && res.status === 200) {
        const etag = header(res, `etag`)
        if (etag && typeof res.clone === `function`) {
          const body = await peekText(res)
          etagSet(etagKey, { etag, body, headers: [...res.headers.entries()] })
        }
      }
      return res
    }
    const lastAttempt = attempt >= MAX_RETRIES - 1
    if (verdict.kind === `transient`) {
      if (lastAttempt) return res
      await sleep(backoffMs(attempt, random))
      continue
    }
    // rate limited
    const waitMs = verdict.waitMs ?? backoffMs(attempt, random)
    if (lastAttempt || waitMs > MAX_WAIT_MS) {
      throw new GithubRateLimited({
        resetAt: verdict.resetAt,
        secondary: verdict.secondary,
        now: now(),
      })
    }
    // retry-after 0 still waits a beat: GitHub asks for at least a second
    await sleep(Math.max(waitMs, verdict.secondary ? BACKOFF_BASE_MS : 0) + Math.round(random() * 250))
  }
}

function isNetworkError(err: unknown): boolean {
  if (err instanceof TypeError) return true
  if (!(err instanceof Error)) return false
  const code = (err as { code?: unknown }).code
  return typeof code === `string` && /^(ECONN|ETIMEDOUT|EAI_|EPIPE|UND_ERR)/.test(code)
}

/** Test hook: forget every budget, ETag and token registration. */
export function resetGithubClientForTests(): void {
  budgets.clear()
  etags.clear()
  tokenInstallation.clear()
}
