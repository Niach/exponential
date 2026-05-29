import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"

const FAILED_REDIRECT = `/auth/login?error=mobile_oauth_failed`
const APP_DEEP_LINK = `exp://oauth-return`
const STATE_COOKIE_NAME = `exp_mobile_oauth_state`
const CLEAR_STATE_COOKIE = `${STATE_COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax`

function readCookie(cookieHeader: string, name: string): string | null {
  const entry = cookieHeader
    .split(`;`)
    .map((s) => s.trim())
    .find((c) => c.startsWith(`${name}=`))
  if (!entry) return null
  return decodeURIComponent(entry.slice(name.length + 1))
}

export const Route = createFileRoute(`/api/mobile-oauth-return`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const cookieHeader = request.headers.get(`cookie`) ?? ``
        // Anti-CSRF for the deep-link hop: the cookie was set by
        // /api/mobile-oauth-start, so absence means this URL was visited
        // out-of-band. Better Auth's own state cookie already protected the
        // Google → /api/auth/callback/google leg, and Better Auth doesn't
        // propagate `state` to callbackURL, so we don't compare it here.
        const stateCookie = readCookie(cookieHeader, STATE_COOKIE_NAME)
        if (!stateCookie) {
          console.warn(
            `[mobile-oauth-return] missing ${STATE_COOKIE_NAME} cookie — rejecting`
          )
          return new Response(`Invalid OAuth state`, {
            status: 400,
            headers: { "Set-Cookie": CLEAR_STATE_COOKIE },
          })
        }

        const session = await auth.api.getSession({ headers: request.headers })
        if (!session?.session) {
          console.warn(`[mobile-oauth-return] no session — falling back to ${FAILED_REDIRECT}`)
          return new Response(null, {
            status: 302,
            headers: {
              Location: new URL(FAILED_REDIRECT, request.url).toString(),
              "Set-Cookie": CLEAR_STATE_COOKIE,
            },
          })
        }

        const ctx = await auth.$context
        const cookieName = ctx.authCookies.sessionToken.name
        const token = readCookie(cookieHeader, cookieName)

        if (!token) {
          console.warn(
            `[mobile-oauth-return] session present but session-cookie '${cookieName}' missing — falling back`
          )
          return new Response(null, {
            status: 302,
            headers: {
              Location: new URL(FAILED_REDIRECT, request.url).toString(),
              "Set-Cookie": CLEAR_STATE_COOKIE,
            },
          })
        }

        const target = `${APP_DEEP_LINK}#token=${encodeURIComponent(token)}`
        // Use raw Response — Response.redirect() may reject non-http schemes.
        return new Response(null, {
          status: 302,
          headers: {
            Location: target,
            "Set-Cookie": CLEAR_STATE_COOKIE,
          },
        })
      },
    },
  },
})
