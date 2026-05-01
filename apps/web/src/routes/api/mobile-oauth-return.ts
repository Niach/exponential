import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"

const FAILED_REDIRECT = `/auth/login?error=mobile_oauth_failed`
const APP_DEEP_LINK = `exp://oauth-return`

export const Route = createFileRoute(`/api/mobile-oauth-return`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const session = await auth.api.getSession({ headers: request.headers })
        if (!session?.session) {
          return Response.redirect(new URL(FAILED_REDIRECT, request.url), 302)
        }

        const ctx = await auth.$context
        const cookieName = ctx.authCookies.sessionToken.name
        const cookieHeader = request.headers.get(`cookie`) ?? ``
        const entry = cookieHeader
          .split(`;`)
          .map((s) => s.trim())
          .find((c) => c.startsWith(`${cookieName}=`))

        if (!entry) {
          return Response.redirect(new URL(FAILED_REDIRECT, request.url), 302)
        }

        const token = decodeURIComponent(entry.slice(cookieName.length + 1))
        const target = `${APP_DEEP_LINK}#token=${encodeURIComponent(token)}`
        // Use raw Response — Response.redirect() may reject non-http schemes.
        return new Response(null, {
          status: 302,
          headers: { Location: target },
        })
      },
    },
  },
})
