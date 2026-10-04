import { authClient } from "@/lib/auth/client"

// SLOP-7: the client side of the ONE GitHub flow.
//
// `/integrations/github` is the guided page (Connect GitHub → Install the
// app → Pick a repository). Web surfaces open it as a POPUP over the page
// they are on — the issue, the board form, the settings section — and
// re-probe when focus comes back; the page closes itself once the person is
// done. Natives open the same page in the system browser with `return=app`.

/** The guided page's path (same origin), with the team/board context the
 * repository step needs. */
export function githubConnectPagePath(opts: {
  teamId?: string | null
  boardId?: string | null
  returnTo?: `app` | `popup`
  /** The same-origin path the person came from: when the flow ends outside
   * a popup (a blocked popup, a tab GitHub's redirect landed in) the page
   * goes straight back there instead of showing a done card. */
  from?: string | null
} = {}): string {
  const params = new URLSearchParams()
  if (opts.teamId) params.set(`team`, opts.teamId)
  if (opts.boardId) params.set(`board`, opts.boardId)
  if (opts.returnTo) params.set(`return`, opts.returnTo)
  if (opts.from && isSameOriginPath(opts.from)) params.set(`from`, opts.from)
  const query = params.toString()
  return `/integrations/github${query ? `?${query}` : ``}`
}

// The hop must open synchronously inside a click handler or popup blockers
// eat it. window.open with a reused window NAME returns the existing popup
// without raising it (the button looked broken while the popup sat behind
// the app), so we keep the handle and focus() it; a popup-blocked `null` is
// reported (returns false) so callers can render an inline hint.
let githubPopup: Window | null = null

export function openGithubPopup(url: string | null | undefined): boolean {
  if (!url) return false
  if (githubPopup && !githubPopup.closed) {
    githubPopup.focus()
    return true
  }
  githubPopup = window.open(url, `gh-install`, `popup,width=980,height=860`)
  if (!githubPopup) return false
  githubPopup.focus()
  return true
}

/** Open the guided page as a popup for a team (and a board whose repository
 * the pick should set). */
export function openGithubConnect(opts: {
  teamId?: string | null
  boardId?: string | null
}): boolean {
  const from =
    typeof window === `undefined`
      ? null
      : `${window.location.pathname}${window.location.search}`
  return openGithubPopup(
    githubConnectPagePath({ ...opts, returnTo: `popup`, from })
  )
}

/** A path on this origin: starts with ONE slash (`//host` would leave), no
 * backslash or control character (browsers read `/\host` and `/<TAB>/host`
 * as `//host`), and it still resolves to this origin. `origin` defaults to
 * the page's own; tests pass one. */
export function isSameOriginPath(
  value: unknown,
  origin: string = typeof window === `undefined`
    ? `http://same-origin.invalid`
    : window.location.origin
): value is string {
  if (typeof value !== `string` || !/^\/(?!\/)/.test(value)) return false
  if (/[\\\u0000-\u001f\u007f]/.test(value)) return false
  try {
    return new URL(value, origin).origin === new URL(origin).origin
  } catch {
    return false
  }
}

/** Pure: a signed-out native arrival (`?return=app`) that has not been
 * through GitHub's install redirect (no `installation_id`). Compat shim for
 * natives that open the connect URL in a session-less browser
 * (routes/integrations/github.tsx); retired with it. */
export function isSignedOutConnectStart(
  returnTo: string | undefined,
  locationSearch: string
): boolean {
  return (
    returnTo === `app` &&
    !new URLSearchParams(locationSearch).has(`installation_id`)
  )
}

// The message callers show when openGithubPopup returns false with a URL at
// hand — one string so every surface says the same thing.
export const POPUP_BLOCKED_MESSAGE = `Your browser blocked the GitHub window. Allow popups for this site and try again.`

/**
 * Link GitHub to the signed-in account: Better Auth's `linkSocial` sends the
 * browser to GitHub's authorize screen and back to `callbackURL` (its own
 * `/api/auth/callback/github` in between). Resolves with an error message
 * when the start itself failed (misconfigured provider, 429); on success
 * the browser is already navigating away.
 */
export async function startGithubLink(
  callbackURL: string,
  errorCallbackURL: string
): Promise<string | null> {
  try {
    const { error } = await authClient.linkSocial({
      provider: `github`,
      callbackURL,
      errorCallbackURL,
    })
    if (error) return error.message ?? `Couldn't start connecting GitHub.`
    return null
  } catch {
    return `Couldn't start connecting GitHub.`
  }
}
