// #LOGS-2: the ONE scrubber every web Sentry event, transaction, log and
// breadcrumb passes through (beforeSend / beforeSendTransaction /
// beforeSendLog / beforeBreadcrumb, client and server alike). Dependency-free.
// Filters `Authorization`, `Cookie` / `Set-Cookie` and every query, body or
// header key matching token|secret|password|key|ticket|session|dsn; the user
// is reduced to its id.

export const FILTERED = `[Filtered]`

/** Query/body/header keys whose values never leave the process. */
export const SENSITIVE_KEY = /token|secret|password|key|ticket|session|dsn/i

const SENSITIVE_HEADERS = new Set([`authorization`, `cookie`, `set-cookie`])

function isSensitiveHeader(name: string): boolean {
  return SENSITIVE_HEADERS.has(name.toLowerCase()) || SENSITIVE_KEY.test(name)
}

/** A query string (with or without the leading `?`) with sensitive values filtered. */
export function scrubQueryString(query: string): string {
  const leading = query.startsWith(`?`) ? `?` : ``
  const body = leading ? query.slice(1) : query
  if (!body) return query
  return (
    leading +
    body
      .split(`&`)
      .map((pair) => {
        const eq = pair.indexOf(`=`)
        const rawKey = eq === -1 ? pair : pair.slice(0, eq)
        let key = rawKey
        try {
          key = decodeURIComponent(rawKey.replace(/\+/g, ` `))
        } catch {
          /* keep the raw key */
        }
        return eq !== -1 && SENSITIVE_KEY.test(key) ? `${rawKey}=${FILTERED}` : pair
      })
      .join(`&`)
  )
}

/** A URL (absolute or relative) with sensitive query values filtered. */
export function scrubUrl(url: string): string {
  const q = url.indexOf(`?`)
  if (q === -1) return url
  const hash = url.indexOf(`#`, q)
  const query = hash === -1 ? url.slice(q) : url.slice(q, hash)
  const rest = hash === -1 ? `` : url.slice(hash)
  return url.slice(0, q) + scrubQueryString(query) + rest
}

/** Deep copy of a JSON-ish value with every sensitive key's value filtered. */
export function scrubData(value: unknown, depth = 0): unknown {
  if (depth > 8 || value === null || typeof value !== `object`) return value
  if (Array.isArray(value)) return value.map((v) => scrubData(v, depth + 1))
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
    out[k] = SENSITIVE_KEY.test(k) ? FILTERED : scrubData(v, depth + 1)
  }
  return out
}

function scrubHeaders(
  headers: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  if (!headers) return headers
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(headers)) {
    if (!isSensitiveHeader(k)) out[k] = v
  }
  return out
}

/** A request body as Sentry carries it: an object, a JSON string or a form string. */
function scrubBody(body: unknown): unknown {
  if (typeof body !== `string`) return scrubData(body)
  try {
    return JSON.stringify(scrubData(JSON.parse(body)))
  } catch {
    return body.includes(`=`) ? scrubQueryString(body) : body
  }
}

type RequestLike = {
  url?: string
  query_string?: unknown
  headers?: Record<string, unknown>
  cookies?: unknown
  data?: unknown
  [key: string]: unknown
}

function scrubRequest<T extends RequestLike | undefined>(request: T): T {
  if (!request) return request
  const out: RequestLike = { ...request }
  if (typeof out.url === `string`) out.url = scrubUrl(out.url)
  if (typeof out.query_string === `string`) {
    out.query_string = scrubQueryString(out.query_string)
  } else if (Array.isArray(out.query_string)) {
    out.query_string = out.query_string.map((pair: unknown) =>
      Array.isArray(pair) && typeof pair[0] === `string` && SENSITIVE_KEY.test(pair[0])
        ? [pair[0], FILTERED]
        : pair
    )
  } else if (out.query_string && typeof out.query_string === `object`) {
    out.query_string = scrubData(out.query_string)
  }
  out.headers = scrubHeaders(out.headers)
  delete out.cookies
  if (`data` in out) out.data = scrubBody(out.data)
  return out as T
}

/** Span/log attribute keys whose string values are URLs or query strings. */
const URL_ATTRIBUTES = new Set([
  `url`,
  `http.url`,
  `url.full`,
  `http.target`,
  `url.query`,
  `http.query`,
  `from`,
  `to`,
])

/**
 * Attributes (span data, breadcrumb data, log attributes): sensitive keys
 * filtered, header-shaped keys dropped, URL values scrubbed.
 */
export function scrubAttributes<T extends Record<string, unknown> | undefined>(
  data: T
): T {
  if (!data) return data
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(data)) {
    const header = /^http\.(request|response)\.header\.(.+)$/.exec(k)
    if (header && isSensitiveHeader(header[2].replace(/_/g, `-`))) continue
    if (URL_ATTRIBUTES.has(k) && typeof v === `string`) {
      out[k] = k.endsWith(`query`) ? scrubQueryString(v) : scrubUrl(v)
    } else if (!header && SENSITIVE_KEY.test(k.split(`.`).pop() ?? k)) {
      out[k] = FILTERED
    } else {
      out[k] = scrubData(v)
    }
  }
  return out as T
}

type SpanLike = { description?: string; data?: Record<string, unknown> }
type BreadcrumbLike = {
  message?: string
  data?: Record<string, unknown>
  [key: string]: unknown
}
type EventLike = {
  request?: RequestLike
  user?: { id?: string | number; [key: string]: unknown }
  extra?: Record<string, unknown>
  breadcrumbs?: BreadcrumbLike[]
  spans?: SpanLike[]
  transaction?: string
  contexts?: { trace?: { data?: Record<string, unknown> }; [key: string]: unknown }
  [key: string]: unknown
}

/** Only the user id ever leaves the process (no email, name, IP). */
function scrubUser(user: EventLike[`user`]): EventLike[`user`] {
  if (!user) return user
  return user.id === undefined ? undefined : { id: String(user.id) }
}

/** beforeBreadcrumb: the crumb with its URLs, data and message scrubbed. */
export function scrubBreadcrumb<T extends object | null>(crumb: T): T {
  if (!crumb) return crumb
  const out: BreadcrumbLike = { ...(crumb as BreadcrumbLike) }
  if (out.data) out.data = scrubAttributes(out.data)
  if (typeof out.message === `string`) out.message = scrubUrlsInText(out.message)
  return out as T
}

/** Free text (a span description, a crumb message): any embedded URL's query scrubbed. */
export function scrubUrlsInText(text: string): string {
  return text.replace(/(\S*\?[^\s#]*)/g, (m) => scrubUrl(m))
}

/** beforeSend / beforeSendTransaction: the whole event scrubbed. */
export function scrubEvent<T extends object | null>(event: T): T {
  if (!event) return event
  const out: EventLike = { ...(event as EventLike) }
  if (out.request) out.request = scrubRequest(out.request)
  if (`user` in out) out.user = scrubUser(out.user)
  if (out.extra) out.extra = scrubData(out.extra) as Record<string, unknown>
  if (out.breadcrumbs) out.breadcrumbs = out.breadcrumbs.map((b) => scrubBreadcrumb(b))
  if (typeof out.transaction === `string`) {
    out.transaction = scrubUrlsInText(out.transaction)
  }
  if (out.contexts?.trace?.data) {
    out.contexts = {
      ...out.contexts,
      trace: { ...out.contexts.trace, data: scrubAttributes(out.contexts.trace.data) },
    }
  }
  if (out.spans) {
    out.spans = out.spans.map((s) => ({
      ...s,
      ...(typeof s.description === `string`
        ? { description: scrubUrlsInText(s.description) }
        : {}),
      ...(s.data ? { data: scrubAttributes(s.data) } : {}),
    }))
  }
  return out as T
}

type LogLike = { message?: unknown; attributes?: Record<string, unknown> }

/** beforeSendLog: attributes and message scrubbed. */
export function scrubLog<T extends object>(input: T): T {
  const log = input as LogLike
  return {
    ...log,
    ...(typeof log.message === `string`
      ? { message: scrubUrlsInText(log.message) }
      : {}),
    ...(log.attributes ? { attributes: scrubAttributes(log.attributes) } : {}),
  } as T
}

/**
 * The SDKs' `dataCollection` (Sentry v11's replacement for
 * `sendDefaultPii: false`, stricter): no auto-collected user fields (only our
 * own `setUser({ id })`), no cookies, no request/response bodies, no DB
 * parameters, queue arguments, AI or MCP content. Headers and query strings
 * are still collected, then filtered by the scrubber above.
 */
export const DATA_COLLECTION = {
  userInfo: false,
  cookies: false,
  httpBodies: [],
  databaseQueryData: false,
  queues: false,
  graphQL: { document: false, variables: false },
  genAI: { inputs: false, outputs: false },
  mcp: { inputs: false, outputs: false },
} satisfies Record<string, unknown>
