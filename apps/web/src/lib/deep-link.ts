// The custom URL scheme the native clients (iOS, Android, desktop) register
// for server → app handoffs. EXP-41 flipped this from `exp://` to
// `exponential://` as a HARD cutover — no `exp://` alias remains registered,
// minted, or parsed anywhere. The server mints exactly two deep links, built
// here so no route carries its own scheme literal. Import-light on purpose:
// client-rendered routes (e.g. the GitHub claim page) import this module too.
export const DEEP_LINK_SCHEME = `exponential`

// `exponential://oauth-return?code=…#code=…` — the OAuth handoff (REV-13).
// The link carries a short-TTL single-use code, NOT the session token: the
// app redeems it via POST /api/mobile-oauth-exchange with the PKCE
// code_verifier it kept in memory, so an intercepting scheme handler learns
// nothing usable. The code rides in BOTH the query AND the fragment (EXP-21):
// when a browser hands a custom scheme to the OS it drops the URL #fragment
// (a client-only construct), so Linux xdg handlers only see the query — while
// iOS's ASWebAuthenticationSession keeps the whole URL and reads the
// fragment. Every native client must keep parsing both forms.
export function oauthReturnCodeDeepLink(code: string): string {
  const enc = encodeURIComponent(code)
  return `${DEEP_LINK_SCHEME}://oauth-return?code=${enc}#code=${enc}`
}

// `exponential://github-connected` — fired after the GitHub App install /
// OAuth-claim flow to hand the user back to the native app. An optional
// `?error=<code>` marks a flow that ended on an error card (EXP-365) so newer
// clients can explain instead of silently refreshing; older clients treat the
// URL as an opaque trigger and ignore the query.
export function githubConnectedDeepLink(error?: string): string {
  return error
    ? `${DEEP_LINK_SCHEME}://github-connected?error=${encodeURIComponent(error)}`
    : `${DEEP_LINK_SCHEME}://github-connected`
}

// The reason slug used when nothing more specific is known.
export const OAUTH_ERROR_FALLBACK = `oauth_failed`

// `exponential://oauth-return?error=…#error=…` — the FAILURE twin of the two
// success handoffs (REV2-53). Every failing branch of the mobile OAuth hop
// must reach the app through this link: the native completion channels only
// recognise `exponential://` navigations (iOS's ASWebAuthenticationSession,
// Android's Custom Tab intent, the desktop's registered handler), so an https
// error page strands the auth sheet with nothing for the user to act on. Same
// query-AND-fragment doubling as the success forms, for the same reason.
export function oauthReturnErrorDeepLink(reason: string): string {
  const enc = encodeURIComponent(normalizeOauthErrorReason(reason))
  return `${DEEP_LINK_SCHEME}://oauth-return?error=${enc}#error=${enc}`
}

// Reason slugs ride a deep link and a login-page query param, and come partly
// from an upstream provider (Better Auth forwards the IdP's `error`), so clamp
// them to a short opaque slug before they reach either sink.
export function normalizeOauthErrorReason(input: unknown): string {
  if (typeof input !== `string`) return OAUTH_ERROR_FALLBACK
  const slug = input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, `_`)
    .replace(/^_+|_+$/g, ``)
    .slice(0, 48)
  return slug || OAUTH_ERROR_FALLBACK
}

// Human copy for a reason slug, shown on the web error/login pages. The native
// clients keep their own translations of the same slugs.
export function oauthErrorMessage(reason: unknown): string {
  switch (normalizeOauthErrorReason(reason)) {
    case `access_denied`:
      return `Sign-in was cancelled.`
    case `state_missing`:
    case `state_invalid`:
    case `state_mismatch`:
    case `state_not_found`:
    case `please_restart_the_process`:
      return `That sign-in link expired. Please start again.`
    case `no_session`:
    case `session_cookie_missing`:
      return `Sign-in didn't complete on the server. Please try again.`
    default:
      return `Couldn't complete sign-in. Please try again.`
  }
}
