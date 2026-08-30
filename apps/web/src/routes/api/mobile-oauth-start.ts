import { createFileRoute } from "@tanstack/react-router"
import { randomBytes } from "crypto"
import { auth } from "@/lib/auth"
import {
  isValidCodeChallenge,
  stateCookieSecureAttribute,
} from "@/lib/auth/mobile-oauth-code"

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

  const callbackURL = `${originForRequest(request)}/api/mobile-oauth-return`
  // Failures must come back through the SAME endpoint (REV2-53): Better Auth
  // otherwise lands provider denials (the user cancelling at Google is the
  // most common failure of all) on its own https error page, which no native
  // completion channel recognises — the auth sheet just sits there. Better
  // Auth appends its reason as `?error=`; the return route turns that into the
  // `exponential://oauth-return?error=…` handoff.
  const errorCallbackURL = callbackURL

  const response = social
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
      `${STATE_COOKIE_NAME}=${cookieValue}; Path=/; Max-Age=600; HttpOnly${secure}; SameSite=Lax`
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
