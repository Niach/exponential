import { createFileRoute } from "@tanstack/react-router"
import {
  exchangeMobileOauthCode,
  isValidCodeVerifier,
} from "@/lib/auth/mobile-oauth-code"

// PKCE code → session-token exchange for the mobile OAuth handoff (REV-13).
// The `exponential://oauth-return?code=…#code=…` deep link carries only a
// single-use short-TTL code; the native app redeems it here with the
// code_verifier it kept in memory, and the token only ever travels over this
// TLS response. Unauthenticated by design — the code+verifier ARE the
// credentials; no cookies or CORS involved (native clients only). Failures
// are always 400 `invalid_grant` (never 404 — the nitro dev bridge mangles
// 404-status responses into connect HTML pages).

function invalidGrant(): Response {
  return Response.json(
    { error: `invalid_grant` },
    { status: 400, headers: { "Cache-Control": `no-store` } }
  )
}

export const Route = createFileRoute(`/api/mobile-oauth-exchange`)({
  server: {
    handlers: {
      POST: async ({ request }) => {
        let body: unknown
        try {
          body = await request.json()
        } catch {
          return invalidGrant()
        }
        const { code, code_verifier: codeVerifier } = (body ?? {}) as {
          code?: unknown
          code_verifier?: unknown
        }
        if (typeof code !== `string` || code.length === 0) {
          return invalidGrant()
        }
        if (typeof codeVerifier !== `string` || !isValidCodeVerifier(codeVerifier)) {
          return invalidGrant()
        }

        const token = exchangeMobileOauthCode(code, codeVerifier)
        if (!token) {
          return invalidGrant()
        }
        return Response.json(
          { token },
          { headers: { "Cache-Control": `no-store` } }
        )
      },
    },
  },
})
