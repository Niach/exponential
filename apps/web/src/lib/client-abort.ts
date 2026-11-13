// #ERR: a client that hangs up mid-request (a closed tab, a cancelled upload,
// a superseded fetch, an SSE or long-poll the client dropped) is expected
// traffic, not a server fault. Bun surfaces it as a bare
// `DOMException AbortError: The connection was closed.` (empty stack) from
// the request's body reads and as `request.signal.reason`. Those get at most a
// debug line with the route; an abort WE caused (our own AbortController or a
// timeout while the client is still connected) stays an error, with context.

/** Bun's message for a request whose client closed the connection. */
export const CLIENT_CLOSED_MESSAGE = `The connection was closed.`

/** nginx's "Client Closed Request": nobody reads it, but logs and metrics do. */
export const CLIENT_CLOSED_STATUS = 499

function errorName(error: unknown): string | undefined {
  if (typeof error !== `object` || error === null) return undefined
  const name = (error as { name?: unknown }).name
  return typeof name === `string` ? name : undefined
}

function errorMessage(error: unknown): string | undefined {
  if (typeof error !== `object` || error === null) return undefined
  const message = (error as { message?: unknown }).message
  return typeof message === `string` ? message : undefined
}

/** An abort or a timeout, whoever caused it. */
export function isAbortLike(error: unknown): boolean {
  const name = errorName(error)
  return name === `AbortError` || name === `TimeoutError`
}

/**
 * Did the CLIENT end this request? With the request at hand its signal is the
 * authority (Bun aborts it when the connection closes); without one, only
 * Bun's own connection-closed AbortError counts, so an abort from our own
 * controller or a timeout is never mistaken for one.
 */
export function isClientAbort(error: unknown, request?: Request): boolean {
  if (errorName(error) !== `AbortError`) return false
  if (request?.signal?.aborted) return true
  return errorMessage(error) === CLIENT_CLOSED_MESSAGE
}

/**
 * A loggable route for a pathname: capability tokens (upload, share, support
 * links) and ids live in path segments, so any long or dotted segment becomes
 * `:id`. Never the query string.
 */
export function routeLabel(pathname: string): string {
  return pathname
    .split(`/`)
    .map((segment) => (segment.length > 24 || (segment.includes(`.`) && segment.length > 16) ? `:id` : segment))
    .join(`/`)
}

const startedAt = new WeakMap<Request, number>()

/** server-bun.ts stamps every request so a failure can report its duration. */
export function markRequestStart(request: Request, now = performance.now()): void {
  startedAt.set(request, now)
}

export function requestDurationMs(request: Request, now = performance.now()): number | undefined {
  const start = startedAt.get(request)
  return start === undefined ? undefined : Math.round(now - start)
}

/** `GET /api/x after 123ms`: the context every request-failure line carries. */
export function describeRequest(request: Request): string {
  let route = `?`
  try {
    route = routeLabel(new URL(request.url).pathname)
  } catch {
    // an unparsable URL keeps `?`
  }
  const ms = requestDurationMs(request)
  return `${request.method} ${route}${ms === undefined ? `` : ` after ${ms}ms`}`
}

/**
 * Log a request that failed with `error`: a client abort is one debug line;
 * anything else (our own abort or timeout included) is an error with the
 * route, method and duration. Returns whether it was a client abort.
 */
export function logRequestFailure(error: unknown, request?: Request): boolean {
  const where = request ? describeRequest(request) : `(no request)`
  if (isClientAbort(error, request)) {
    console.debug(`[client-abort] ${where}`)
    return true
  }
  const tag = isAbortLike(error) ? `[request aborted]` : `[request error]`
  console.error(`${tag} ${where}`, error)
  return false
}

type ExceptionEvent = {
  exception?: { values?: Array<{ type?: string; value?: string }> }
}

/**
 * GlitchTip filter: an error event whose exception is Bun's connection-closed
 * AbortError is a client abort (an unhandled rejection or a captured route
 * error that slipped past the handlers); drop it.
 */
export function isClientAbortEvent(event: ExceptionEvent | null | undefined): boolean {
  const values = event?.exception?.values
  if (!values?.length) return false
  return values.some((v) => v.type === `AbortError` && v.value === CLIENT_CLOSED_MESSAGE)
}
