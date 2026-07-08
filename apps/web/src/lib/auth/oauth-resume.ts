/**
 * When the MCP OAuth flow (Better Auth `mcp` plugin) bounces an
 * unauthenticated user to the login page, the original authorize query
 * (client_id, redirect_uri, response_type, code_challenge, state, ...) rides
 * along in the URL. Once a session exists, the BROWSER must navigate back to
 * the authorize endpoint so the plugin can mint the code and redirect to the
 * client's callback — the background sign-in fetch can't complete that hop.
 *
 * Call this once on page load (before the router can touch the URL) and use
 * the returned URL as the post-sign-in destination.
 */
export function captureOAuthResumeUrl(): string | null {
  if (typeof window === `undefined`) return null
  const params = new URLSearchParams(window.location.search)
  if (!params.has(`client_id`) || !params.has(`response_type`)) return null
  // The mcp plugin also parked the query in an `oidc_login_prompt` cookie so
  // its after-hook can finish the flow right on the sign-in response. That
  // hook would turn the sign-in FETCH into a redirect to the client's
  // http://localhost callback — which browsers block as mixed content on
  // https, stranding the user. We resume via top-level navigation instead,
  // so drop the cookie (it's ours to read: not HttpOnly, path=/).
  document.cookie = `oidc_login_prompt=; Max-Age=0; path=/`
  params.delete(`redirect`)
  return `/api/auth/mcp/authorize?${params.toString()}`
}
