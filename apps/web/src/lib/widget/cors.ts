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
  }
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
