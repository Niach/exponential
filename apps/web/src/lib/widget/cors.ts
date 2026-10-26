// Hand-rolled CORS for the public widget endpoints — the rest of the app is
// strictly same-origin, so there is intentionally no global CORS middleware.
// No Allow-Credentials: the widget always fetches with `credentials: "omit"`.
export function corsHeaders(echoOrigin: string | null): Record<string, string> {
  if (!echoOrigin) return {}
  return {
    "Access-Control-Allow-Origin": echoOrigin,
    Vary: `Origin`,
    "Access-Control-Allow-Methods": `GET, POST, OPTIONS`,
    "Access-Control-Allow-Headers": `content-type`,
    "Access-Control-Max-Age": `86400`,
    // The widget reads Retry-After off a 429 to tell the reporter when to
    // retry; non-safelisted response headers are hidden without this.
    "Access-Control-Expose-Headers": `Retry-After`,
  }
}

// Structured `code` on every widget error body (NEW-19), additive to the
// human `error` string. The email/name codes come from WidgetRequestError.
export type WidgetErrorCode =
  | `origin_not_allowed`
  | `widget_disabled`
  | `unknown_key`
  | `rate_limited`
  | `too_large`
  | `bad_request`
  | `internal`
  | `invalid_email`
  | `email_required`
  | `name_required`

// An error response for the public widget endpoints. EVERY error carries CORS
// headers so the browser lets the widget read status + body (FEED-99/115):
// pass the allowlist-derived headers when the origin is known-good, otherwise
// the request's own Origin is echoed — the same reasoning as the permissive
// preflight: no credentials ever, and the body is only a generic error.
export function widgetErrorResponse(
  request: Request,
  status: number,
  body: { error: string; code?: WidgetErrorCode } & Record<string, unknown>,
  opts: { cors?: Record<string, string>; retryAfterSeconds?: number } = {}
): Response {
  const cors = opts.cors ?? corsHeaders(request.headers.get(`origin`))
  if (opts.retryAfterSeconds === undefined) {
    return jsonResponse(status, body, cors)
  }
  return jsonResponse(
    status,
    { ...body, retryAfterSeconds: opts.retryAfterSeconds },
    { ...cors, "Retry-After": String(opts.retryAfterSeconds) }
  )
}

export function jsonResponse(
  status: number,
  body: unknown,
  extraHeaders: Record<string, string> = {}
): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": `application/json`, ...extraHeaders },
  })
}

// Both widget requests are CORS-"simple" (GET / multipart POST, no custom
// headers), so browsers usually skip the preflight — this handler is the
// defensive path for clients that send one anyway.
export function preflightResponse(echoOrigin: string | null): Response {
  return new Response(null, { status: 204, headers: corsHeaders(echoOrigin) })
}
