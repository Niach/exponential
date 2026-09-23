// EXP-630: the first GraphQL client in this repo — a thin POST wrapper for
// Linear's API with the two things that matter there: authentication is the
// RAW personal key in `Authorization` (a `Bearer` prefix is for OAuth tokens
// and fails for `lin_api_…` keys), and the rate limits ride on headers
// (requests/hour, complexity/hour, plus a hard cap of 10,000 complexity per
// query). The client backs off on the headers and on a `RATELIMITED` error,
// retries 5xx with capped backoff, and surfaces a too-complex query as a
// typed error so the paginator can halve its page size.
//
// Injectable fetch/sleep/clock — the tests drive it with fixtures.

export const LINEAR_GRAPHQL_URL = `https://api.linear.app/graphql`
export const LINEAR_MAX_QUERY_COMPLEXITY = 10_000

export interface LinearResponseLike {
  ok: boolean
  status: number
  headers: { get(name: string): string | null }
  text(): Promise<string>
}

export type LinearFetch = (
  url: string,
  init: {
    method: string
    headers: Record<string, string>
    body?: string
  }
) => Promise<LinearResponseLike>

export type LinearErrorKind =
  | `auth`
  | `complexity`
  | `rate_limit`
  | `http`
  | `graphql`
  | `network`

export class LinearApiError extends Error {
  constructor(
    message: string,
    readonly kind: LinearErrorKind,
    readonly status?: number
  ) {
    super(message)
    this.name = `LinearApiError`
  }
}

export interface LinearRateLimit {
  requestsRemaining: number | null
  requestsResetMs: number | null
  complexityRemaining: number | null
  complexityResetMs: number | null
  // The cost Linear charged for the last query.
  complexity: number | null
}

// Never sleep longer than this on a rate-limit reset; past it the job fails
// with a readable reason and the operator retries later (the worker resumes).
const MAX_RATE_LIMIT_WAIT_MS = 15 * 60 * 1000
// Below this many requests left, wait for the reset before spending more.
const REQUEST_HEADROOM = 3
const COMPLEXITY_HEADROOM = 20_000

const RETRY_BASE_DELAY_MS = 1_000
const RETRY_MAX_DELAY_MS = 30_000

function intHeader(headers: LinearResponseLike[`headers`], name: string): number | null {
  const raw = headers.get(name)
  if (raw === null || raw === ``) return null
  const value = Number(raw)
  return Number.isFinite(value) ? value : null
}

export function readRateLimit(headers: LinearResponseLike[`headers`]): LinearRateLimit {
  return {
    requestsRemaining: intHeader(headers, `x-ratelimit-requests-remaining`),
    requestsResetMs: intHeader(headers, `x-ratelimit-requests-reset`),
    complexityRemaining: intHeader(headers, `x-ratelimit-complexity-remaining`),
    complexityResetMs: intHeader(headers, `x-ratelimit-complexity-reset`),
    complexity: intHeader(headers, `x-complexity`),
  }
}

interface GraphQLError {
  message?: string
  extensions?: { code?: string; type?: string; userPresentableMessage?: string }
}

export function isComplexityError(error: unknown): boolean {
  return error instanceof LinearApiError && error.kind === `complexity`
}

export class LinearClient {
  readonly rateLimit: LinearRateLimit = {
    requestsRemaining: null,
    requestsResetMs: null,
    complexityRemaining: null,
    complexityResetMs: null,
    complexity: null,
  }
  // Every `x-complexity` seen, in order — the fixture test asserts the
  // issue page stays under the per-query cap.
  readonly complexityLog: number[] = []
  requestCount = 0

  private readonly fetchImpl: LinearFetch
  private readonly sleep: (ms: number) => Promise<void>
  private readonly now: () => number
  private readonly maxAttempts: number

  constructor(
    private readonly apiKey: string,
    options: {
      fetchImpl?: LinearFetch
      sleep?: (ms: number) => Promise<void>
      now?: () => number
      maxAttempts?: number
    } = {}
  ) {
    this.fetchImpl =
      options.fetchImpl ??
      ((url, init) => globalThis.fetch(url, init) as Promise<LinearResponseLike>)
    this.sleep =
      options.sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)))
    this.now = options.now ?? (() => Date.now())
    this.maxAttempts = options.maxAttempts ?? 5
  }

  private async waitForReset(resetMs: number | null, what: string): Promise<void> {
    const wait = resetMs === null ? 60_000 : Math.max(1_000, resetMs - this.now())
    if (wait > MAX_RATE_LIMIT_WAIT_MS) {
      throw new LinearApiError(
        `Linear ${what} rate limit reached; it resets in ${Math.ceil(wait / 60_000)} minutes. Retry the import later.`,
        `rate_limit`,
        429
      )
    }
    await this.sleep(wait)
  }

  private async respectHeadroom(): Promise<void> {
    const { requestsRemaining, requestsResetMs, complexityRemaining, complexityResetMs } =
      this.rateLimit
    if (requestsRemaining !== null && requestsRemaining < REQUEST_HEADROOM) {
      await this.waitForReset(requestsResetMs, `request`)
      this.rateLimit.requestsRemaining = null
    }
    if (complexityRemaining !== null && complexityRemaining < COMPLEXITY_HEADROOM) {
      await this.waitForReset(complexityResetMs, `complexity`)
      this.rateLimit.complexityRemaining = null
    }
  }

  async query<T>(query: string, variables: Record<string, unknown> = {}): Promise<T> {
    for (let attempt = 1; ; attempt++) {
      await this.respectHeadroom()
      let response: LinearResponseLike
      try {
        this.requestCount += 1
        response = await this.fetchImpl(LINEAR_GRAPHQL_URL, {
          method: `POST`,
          headers: {
            "content-type": `application/json`,
            // Personal API keys go raw — no `Bearer`.
            authorization: this.apiKey,
          },
          body: JSON.stringify({ query, variables }),
        })
      } catch (err) {
        if (attempt >= this.maxAttempts) {
          throw new LinearApiError(
            `Could not reach Linear: ${err instanceof Error ? err.message : String(err)}`,
            `network`
          )
        }
        await this.sleep(retryDelay(attempt))
        continue
      }

      Object.assign(this.rateLimit, readRateLimit(response.headers))
      if (this.rateLimit.complexity !== null) {
        this.complexityLog.push(this.rateLimit.complexity)
      }

      if (response.status === 401 || response.status === 403) {
        throw new LinearApiError(
          `Linear rejected the API key (${response.status}). Check the key and that it has read access.`,
          `auth`,
          response.status
        )
      }
      if (response.status === 429) {
        await this.waitForReset(
          this.rateLimit.requestsResetMs ?? this.rateLimit.complexityResetMs,
          `request`
        )
        continue
      }
      if (response.status >= 500) {
        if (attempt >= this.maxAttempts) {
          throw new LinearApiError(
            `Linear returned ${response.status}`,
            `http`,
            response.status
          )
        }
        await this.sleep(retryDelay(attempt))
        continue
      }

      const text = await response.text()
      let parsed: { data?: T; errors?: GraphQLError[] }
      try {
        parsed = JSON.parse(text) as { data?: T; errors?: GraphQLError[] }
      } catch {
        throw new LinearApiError(
          `Linear returned a non-JSON response (${response.status})`,
          `http`,
          response.status
        )
      }
      if (!response.ok) {
        const message = parsed.errors?.[0]?.message ?? `HTTP ${response.status}`
        throw new LinearApiError(`Linear: ${message}`, `http`, response.status)
      }
      if (parsed.errors && parsed.errors.length > 0) {
        const first = parsed.errors[0]!
        const code = first.extensions?.code ?? first.extensions?.type ?? ``
        const message = first.extensions?.userPresentableMessage ?? first.message ?? `unknown error`
        if (code === `RATELIMITED` || /rate ?limit/i.test(message)) {
          await this.waitForReset(
            this.rateLimit.requestsResetMs ?? this.rateLimit.complexityResetMs,
            `request`
          )
          continue
        }
        if (/complexity/i.test(message)) {
          throw new LinearApiError(`Linear query too complex: ${message}`, `complexity`)
        }
        if (/authentication|not authorized|unauthorized|api key/i.test(message)) {
          throw new LinearApiError(`Linear: ${message}`, `auth`, 401)
        }
        throw new LinearApiError(`Linear: ${message}`, `graphql`)
      }
      if (parsed.data === undefined || parsed.data === null) {
        throw new LinearApiError(`Linear returned no data`, `graphql`)
      }
      return parsed.data
    }
  }
}

function retryDelay(attempt: number): number {
  return Math.min(RETRY_BASE_DELAY_MS * 2 ** (attempt - 1), RETRY_MAX_DELAY_MS)
}

export interface Connection<T> {
  nodes: T[]
  pageInfo: { hasNextPage: boolean; endCursor: string | null }
}

export const DEFAULT_PAGE_SIZE = 50
export const MIN_PAGE_SIZE = 5

// Walks a connection page by page. A too-complex page halves `first` (down
// to MIN_PAGE_SIZE) and retries the SAME cursor; the page size stays reduced
// for the rest of the walk.
export async function* paginate<T>(
  fetchPage: (first: number, after: string | null) => Promise<Connection<T>>,
  options: { pageSize?: number; onPage?: (nodes: T[]) => void } = {}
): AsyncGenerator<T[], void, void> {
  let first = options.pageSize ?? DEFAULT_PAGE_SIZE
  let after: string | null = null
  while (true) {
    let page: Connection<T>
    try {
      page = await fetchPage(first, after)
    } catch (err) {
      if (isComplexityError(err) && first > MIN_PAGE_SIZE) {
        first = Math.max(MIN_PAGE_SIZE, Math.floor(first / 2))
        continue
      }
      throw err
    }
    options.onPage?.(page.nodes)
    yield page.nodes
    if (!page.pageInfo.hasNextPage || !page.pageInfo.endCursor) return
    after = page.pageInfo.endCursor
  }
}
