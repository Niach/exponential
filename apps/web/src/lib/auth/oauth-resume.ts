/**
 * When the MCP OAuth flow (better-auth 1.7 oauth-provider) bounces an
 * unauthenticated user to the login page, the original authorize query
 * (client_id, redirect_uri, response_type, code_challenge, state, ...) rides
 * along in the URL, signed (`sig`, `exp`, `ba_*`). Once a session exists, the
 * BROWSER must navigate back to the authorize endpoint so the provider can
 * continue to the consent page and on to the client's callback — the
 * background sign-in fetch can't complete that hop (a redirect to the
 * client's http://localhost callback from a fetch is blocked as mixed content
 * on https, stranding the user). The sign-in request never carries the
 * signed query, so the provider's own after-sign-in continuation stays idle.
 *
 * Call this once on page load and use the returned URL as the post-sign-in
 * destination.
 */
const SIGNING_PARAMS = [`sig`, `exp`, `ba_iat`, `ba_pl`, `ba_param`, `redirect`]

export function oauthResumeUrlFor(search: string): string | null {
  const params = new URLSearchParams(search)
  if (!params.has(`client_id`) || !params.has(`response_type`)) return null
  for (const name of SIGNING_PARAMS) params.delete(name)
  return `/api/auth/oauth2/authorize?${params.toString()}`
}

export function captureOAuthResumeUrl(): string | null {
  if (typeof window === `undefined`) return null
  return oauthResumeUrlFor(window.location.search)
}
