// The custom URL scheme the native clients (iOS, Android, desktop) register
// for server → app handoffs. EXP-41 flipped this from `exp://` to
// `exponential://` as a HARD cutover — no `exp://` alias remains registered,
// minted, or parsed anywhere. The server mints exactly two deep links, built
// here so no route carries its own scheme literal. Import-light on purpose:
// client-rendered routes (e.g. the GitHub claim page) import this module too.
export const DEEP_LINK_SCHEME = `exponential`

// `exponential://oauth-return?token=…#token=…` — the OAuth session handoff.
// The token rides in BOTH the query AND the fragment (EXP-21): when a browser
// hands a custom scheme to the OS it drops the URL #fragment (a client-only
// construct), so Linux xdg handlers only see the query — while iOS's
// ASWebAuthenticationSession keeps the whole URL and reads the fragment.
// Every native client must keep parsing both forms.
export function oauthReturnDeepLink(token: string): string {
  const enc = encodeURIComponent(token)
  return `${DEEP_LINK_SCHEME}://oauth-return?token=${enc}#token=${enc}`
}

// `exponential://github-connected` (no payload) — fired after the GitHub App
// install / OAuth-claim flow to hand the user back to the native app.
export function githubConnectedDeepLink(): string {
  return `${DEEP_LINK_SCHEME}://github-connected`
}
