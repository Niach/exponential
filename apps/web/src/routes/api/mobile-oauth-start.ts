import { createFileRoute } from "@tanstack/react-router"
import { randomBytes } from "crypto"
import { eq } from "drizzle-orm"
import { auth } from "@/lib/auth"
import { db } from "@/db/connection"
import { sessions } from "@/db/auth-schema"
import {
  isValidCodeChallenge,
  stateCookieSecureAttribute,
} from "@/lib/auth/mobile-oauth-code"
import { redeemSignInLinkTicket } from "@/lib/auth/sign-in-link-ticket"

// Custom Tabs only emit GETs, but Better Auth's /sign-in/oauth2 and
// /sign-in/social are POST-only. Bridge: client opens this GET endpoint,
// we invoke the POST server-side, then forward Better Auth's response
// (state cookies + redirect to the IdP) to the browser.

const STATE_COOKIE_NAME = `exp_mobile_oauth_state`

function originForRequest(request: Request): string {
  const url = new URL(request.url)
  return `${url.protocol}//${url.host}`
}

async function handle({ request }: { request: Request }) {
  const url = new URL(request.url)
  const providerId = url.searchParams.get(`providerId`)
  const social = url.searchParams.get(`provider`)

  if (!providerId && !social) {
    return new Response(`Missing providerId or provider`, { status: 400 })
  }
  // EXP-857: `provider=browser` is the generic handoff — no IdP hop, the
  // web login page itself is the destination (passkeys and one-time codes
  // for the desktop app, and the mobile fallback when the on-device
  // ceremony is unavailable). The page ends on /api/mobile-oauth-return
  // via its `redirect`, which is where the state cookie set below is
  // checked, so the completion is exactly the OAuth one.
  const browserHandoff = social === `browser`

  // PKCE (REV-13, required since EXP-543): the client presents an S256
  // code_challenge here and the return page mints a one-time code instead of
  // leaking the raw session token into the deep link. There is no
  // challenge-less flow — a start without one is a 400.
  const codeChallenge = url.searchParams.get(`code_challenge`)
  const codeChallengeMethod = url.searchParams.get(`code_challenge_method`)
  if (codeChallenge === null || !isValidCodeChallenge(codeChallenge)) {
    return new Response(`Missing or invalid code_challenge`, { status: 400 })
  }
  if (codeChallengeMethod !== null && codeChallengeMethod !== `S256`) {
    return new Response(`Unsupported code_challenge_method`, { status: 400 })
  }

  // EXP-1126 LINK mode: a signed-in native attaches a provider to its
  // account. The ticket (tRPC users.mintSignInLinkTicket) names the app's
  // session and the provider; Better Auth's link-social runs on that session
  // and the callback lands on the return route with `?linked=<provider>`,
  // which deep-links `exponential://oauth-return?linked=…` (no credential:
  // the link callback creates no browser session). Any refusal here rides
  // the return route's `?error=` branch so the auth sheet always completes.
  const linkTicket = url.searchParams.get(`link`)
  const requestedProvider = social ?? providerId!
  const linkReturnURL = `${originForRequest(request)}/api/mobile-oauth-return?linked=${encodeURIComponent(requestedProvider)}`
  const linkFailure = (reason: string) =>
    Response.redirect(`${linkReturnURL}&error=${encodeURIComponent(reason)}`, 302)

  let linkHeaders: Headers | null = null
  if (linkTicket !== null) {
    if (browserHandoff) return linkFailure(`link_ticket_invalid`)
    const redeemed = redeemSignInLinkTicket(linkTicket)
    if (!redeemed || redeemed.provider !== requestedProvider) {
      return linkFailure(`link_ticket_invalid`)
    }
    const [row] = await db
      .select({ token: sessions.token, expiresAt: sessions.expiresAt })
      .from(sessions)
      .where(eq(sessions.id, redeemed.sessionId))
      .limit(1)
    if (!row || row.expiresAt.getTime() <= Date.now()) {
      return linkFailure(`link_ticket_invalid`)
    }
    // The browser's own cookies are irrelevant (and possibly another
    // account's): the bearer plugin resolves the app's session from the
    // Authorization header; the rest of the request headers ride along for
    // Better Auth's origin/user-agent handling.
    linkHeaders = new Headers(request.headers)
    linkHeaders.delete(`cookie`)
    linkHeaders.set(`authorization`, `Bearer ${row.token}`)
  }

  const callbackURL = linkHeaders
    ? linkReturnURL
    : `${originForRequest(request)}/api/mobile-oauth-return`
  // Failures must come back through the SAME endpoint (REV2-53): Better Auth
  // otherwise lands provider denials (the user cancelling at Google is the
  // most common failure of all) on its own https error page, which no native
  // completion channel recognises — the auth sheet just sits there. Better
  // Auth appends its reason as `?error=`; the return route turns that into the
  // `exponential://oauth-return?error=…` handoff.
  const errorCallbackURL = callbackURL

  const response = linkHeaders
    ? social
      ? await auth.api.linkSocialAccount({
          body: { provider: social as never, callbackURL, errorCallbackURL },
          headers: linkHeaders,
          asResponse: true,
        })
      : await auth.api.oAuth2LinkAccount({
          body: { providerId: providerId!, callbackURL, errorCallbackURL },
          headers: linkHeaders,
          asResponse: true,
        })
    : browserHandoff
    ? Response.json({
        url: `/auth/login?redirect=${encodeURIComponent(`/api/mobile-oauth-return`)}`,
        redirect: true,
      })
    : social
    ? await auth.api.signInSocial({
        body: { provider: social as never, callbackURL, errorCallbackURL },
        headers: request.headers,
        asResponse: true,
      })
    : await auth.api.signInWithOAuth2({
        body: { providerId: providerId!, callbackURL, errorCallbackURL },
        headers: request.headers,
        asResponse: true,
      })

  // Better Auth returns 200 JSON `{ url, redirect: true }` instead of a 302.
  // Translate to a real redirect so the Custom Tab follows it. State cookies
  // set on the response carry over because we forward all headers.
  const data = (await response.clone().json().catch(() => undefined)) as
    | { url?: string; redirect?: boolean }
    | undefined

  if (!data?.url) {
    const raw = await response
      .clone()
      .text()
      .catch(() => `<no body>`)
    console.error(
      `[mobile-oauth-start] Better Auth did not return a redirect url: status=${response.status} body=${raw.slice(0, 500)}`
    )
    // Link mode has a completion channel of its own: a refused link (dead
    // session, unknown provider, 401) must still close the auth sheet.
    if (linkHeaders) return linkFailure(`unable_to_link_account`)
  }

  const headers = new Headers(response.headers)
  if (data?.url) {
    // CSRF defense: drop a short-lived cookie so /api/mobile-oauth-return can
    // reject calls that didn't originate here. We intentionally do NOT touch
    // the `state` query param on data.url — Better Auth puts its own state
    // there and verifies it against `__Secure-better-auth.state` on the
    // OAuth callback. Overwriting it breaks Google's signInSocial flow (the
    // callback handler fails CSRF, falls back to the default redirect, and
    // the user lands on the web app instead of being deep-linked back).
    // The PKCE challenge rides in the same cookie as `<state>.<challenge>`
    // — `.` is unambiguous (hex and base64url contain no dot) — so the
    // return page mints a code deep link bound to that challenge.
    const state = randomBytes(32).toString(`hex`)
    const cookieValue = `${state}.${codeChallenge}`
    // `Secure` only on an https deployment — see stateCookieSecureAttribute.
    const secure = stateCookieSecureAttribute(request)
    headers.append(
      `Set-Cookie`,
      `${STATE_COOKIE_NAME}=${cookieValue}; Path=/; Max-Age=1800; HttpOnly${secure}; SameSite=Lax`
    )
    headers.set(`Location`, data.url)
    headers.delete(`Content-Type`)
    headers.delete(`Content-Length`)
    return new Response(null, { status: 302, headers })
  }

  return response
}

export const Route = createFileRoute(`/api/mobile-oauth-start`)({
  server: {
    handlers: {
      GET: handle,
    },
  },
})
