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
  params.delete(`redirect`)
  return `/api/auth/mcp/authorize?${params.toString()}`
}
