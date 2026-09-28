// EXP-1111: `POST /api/cli/install-token/redeem` — body `{"token":"expi_…"}`
// → 200 `{"access_token","token_type":"Bearer"}`, the SAME credential the
// device-code flow's `/api/auth/device/token` returns on approval: a Better
// Auth session minted through `internalAdapter.createSession` (so the session
// hooks — placeholder claim included — run as for any sign-in) and accepted
// everywhere via the bearer plugin. Every failure is ONE shape, 400
// `{"error":"invalid_token"}` (never 404 — the nitro dev bridge mangles
// 404-status responses), bar the per-IP ceiling's 429.
import { db } from "@/db/connection"
import { auth } from "@/lib/auth"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"
import {
  claimInstallToken,
  installTokenRedeemLimiter,
} from "@/lib/auth/cli-install-token"

const NO_STORE = { "Cache-Control": `no-store`, Pragma: `no-cache` }

export interface RedeemDeps {
  claim: (token: unknown) => Promise<string | null>
  createSession: (
    userId: string,
    meta: { ipAddress: string; userAgent: string }
  ) => Promise<{ token: string } | null>
  allow: (ip: string) => boolean
}

const defaultDeps: RedeemDeps = {
  claim: (token) => claimInstallToken(db, token),
  createSession: async (userId, meta) => {
    const ctx = await auth.$context
    return ctx.internalAdapter.createSession(userId, false, meta)
  },
  allow: (ip) => installTokenRedeemLimiter.tryTake(ip).ok,
}

function invalidToken(): Response {
  return Response.json(
    { error: `invalid_token` },
    { status: 400, headers: NO_STORE }
  )
}

export async function redeemInstallTokenRequest(
  request: Request,
  deps: RedeemDeps = defaultDeps
): Promise<Response> {
  const ip = clientIpFromRequest(request)
  if (!deps.allow(ip)) {
    return Response.json(
      { error: `slow_down` },
      { status: 429, headers: NO_STORE }
    )
  }
  let body: unknown
  try {
    body = await request.json()
  } catch {
    return invalidToken()
  }
  const token = (body as { token?: unknown } | null)?.token
  const userId = await deps.claim(token)
  if (!userId) return invalidToken()
  const session = await deps.createSession(userId, {
    ipAddress: ip === `unknown` ? `` : ip,
    userAgent: request.headers.get(`user-agent`) ?? ``,
  })
  if (!session) {
    return Response.json(
      { error: `server_error` },
      { status: 500, headers: NO_STORE }
    )
  }
  return Response.json(
    { access_token: session.token, token_type: `Bearer` },
    { headers: NO_STORE }
  )
}
