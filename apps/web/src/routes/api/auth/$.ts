import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import { withProxyAttestedClientIp } from "@/lib/auth/client-ip"
import { guardMcpAuthorize } from "@/lib/auth/mcp-authorize-guard"
import { withCurrentAuthPath } from "@/lib/auth/legacy-auth-paths"

// The Creem plugin registers session-facing endpoints we never call —
// checkout/seat/plan/cancel changes all go through the team-scoped tRPC
// billing router, and the portal through billing.createPortalSession
// (EXP-315). Several of them are also unsafe to leave reachable: they act on
// an arbitrary body-supplied Creem id for any signed-in user
// (`create-portal` mints another customer's portal link,
// `cancel-subscription` cancels any subscription id). Only the webhook —
// the one plugin route we actually use — passes through.
function isBlockedCreemEndpoint(pathname: string): boolean {
  return (
    pathname.startsWith(`/api/auth/creem/`) &&
    pathname !== `/api/auth/creem/webhook`
  )
}

// The 1.6 paths (MCP OAuth under /mcp/*, OIDC callbacks under
// /oauth2/callback/*) are re-addressed to their 1.7 homes first
// (lib/auth/legacy-auth-paths.ts). MCP OAuth authorize requests then get a
// pre-flight (stale-client error page, forced prompt=consent) before
// better-auth handles them. EXP-503: both
// handlers hand better-auth a request whose x-forwarded-for is collapsed to
// the proxy-attested last hop, so its per-IP rate-limit buckets key on an
// address the client cannot rotate or spoof.
async function handleGet(incoming: Request) {
  const request = withCurrentAuthPath(incoming)
  const pathname = new URL(request.url).pathname
  if (isBlockedCreemEndpoint(pathname)) {
    return new Response(`Not found`, { status: 404 })
  }
  if (pathname === `/api/auth/oauth2/authorize`) {
    const guarded = await guardMcpAuthorize(withProxyAttestedClientIp(request))
    if (`response` in guarded) return guarded.response
    return auth.handler(guarded.request)
  }
  return auth.handler(withProxyAttestedClientIp(request))
}

async function handlePost(incoming: Request) {
  const request = withCurrentAuthPath(incoming)
  if (isBlockedCreemEndpoint(new URL(request.url).pathname)) {
    return new Response(`Not found`, { status: 404 })
  }
  return auth.handler(withProxyAttestedClientIp(request))
}

export const Route = createFileRoute(`/api/auth/$`)({
  server: {
    handlers: {
      GET: ({ request }) => handleGet(request),
      POST: ({ request }) => handlePost(request),
    },
  },
})
