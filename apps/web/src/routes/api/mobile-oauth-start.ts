import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"

// Custom Tabs only emit GETs, but Better Auth's /sign-in/oauth2 and
// /sign-in/social are POST-only. Bridge: client opens this GET endpoint,
// we invoke the POST server-side, then forward Better Auth's response
// (state cookies + redirect to the IdP) to the browser.

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

  const callbackURL = `${originForRequest(request)}/api/mobile-oauth-return`

  const response = social
    ? await auth.api.signInSocial({
        body: { provider: social as never, callbackURL },
        headers: request.headers,
        asResponse: true,
      })
    : await auth.api.signInWithOAuth2({
        body: { providerId: providerId!, callbackURL },
        headers: request.headers,
        asResponse: true,
      })

  // Better Auth returns 200 JSON `{ url, redirect: true }` instead of a 302.
  // Translate to a real redirect so the Custom Tab follows it. State cookies
  // set on the response carry over because we forward all headers.
  const data = (await response.clone().json()) as
    | { url?: string; redirect?: boolean }
    | undefined

  const headers = new Headers(response.headers)
  if (data?.url) {
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
