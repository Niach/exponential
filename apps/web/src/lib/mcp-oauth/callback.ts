// EXP-792: the hosted redirect target of a member's MCP OAuth sign-in.
// Reachable without a session (the provider redirects whatever browser
// consented) and keyed on `state`: 32 random bytes minted by
// mcpServers.connect, single-use (the flow leaves `pending` the moment a
// request claims it) and 10-minute bound. It then REQUIRES the Better Auth
// session of the member who started the flow (a same-origin top-level GET,
// so the Lax cookie arrives): a consent completed in any other browser or
// account (a phished authorize URL) fails the flow instead of landing the
// provider account on the starter's credential. The code is exchanged HERE, server-side, with the PKCE
// verifier the flow row holds encrypted; the token set lands in the
// member's `mcp_credentials` row. The code and tokens are never logged or
// echoed. An unknown/expired state answers the same plain page with no
// detail. A flow with a `return_to` 302s back into the app with
// `?mcp=connected|failed&server=<id>[&error=]`; without one the page says
// what happened.
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { auth } from "@/lib/auth"
import { mcpOauthFlows, mcpServers } from "@/db/schema"
import {
  TokenBucketLimiter,
  clientIpFromRequest,
  envInt,
} from "@/lib/widget/rate-limit"
import { claimFlow, completeFlow, flowAcceptsCode } from "@/lib/mcp-oauth/flows"
import { returnUrl, safeReturnTo } from "@/lib/mcp-oauth/urls"
import { mcpOauthPageResponse } from "@/lib/mcp-oauth/page"

// Per-IP bucket: one consent redirect per sign-in, so 60/h with a burst of
// 20 is generous for a human and tight for a state-guessing loop (each
// probe costs a DB round trip before anything can be refused).
let callbackIpLimiter: TokenBucketLimiter | null = null
export function getMcpOauthCallbackLimiter(): TokenBucketLimiter {
  callbackIpLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`MCP_OAUTH_CALLBACK_RATE_LIMIT_IP_BURST`, 20),
    refillPerHour: envInt(`MCP_OAUTH_CALLBACK_RATE_LIMIT_PER_IP_HOURLY`, 60),
  })
  return callbackIpLimiter
}

/** Test seam: drop the module bucket. */
export function resetMcpOauthCallbackLimiter(): void {
  callbackIpLimiter = null
}

const MAX_STATE = 64
const MAX_CODE = 4096
const MAX_ERROR = 500

function expiredPage(): Response {
  return mcpOauthPageResponse({
    ok: false,
    title: `Link expired`,
    body: `This sign-in link has expired.`,
  })
}

function wrongAccountPage(): Response {
  return mcpOauthPageResponse({
    ok: false,
    title: `Sign-in failed`,
    body: `To connect an MCP server you must be signed in to Exponential, in this browser, as the member who started the sign-in. Sign in as that member and choose Connect again in Settings › MCP servers.`,
  })
}

async function sessionUserId(request: Request): Promise<string | null> {
  try {
    const session = await auth.api.getSession({ headers: request.headers })
    return session?.user?.id ?? null
  } catch {
    return null
  }
}

function redirectTo(location: string): Response {
  return new Response(null, {
    status: 302,
    headers: { location, "cache-control": `no-store` },
  })
}

export async function handleMcpOauthCallback(
  request: Request
): Promise<Response> {
  const limit = getMcpOauthCallbackLimiter().tryTake(
    clientIpFromRequest(request)
  )
  if (!limit.ok) {
    return new Response(`Too many requests`, {
      status: 429,
      headers: {
        "retry-after": String(limit.retryAfterSeconds),
        "cache-control": `no-store`,
      },
    })
  }

  const url = new URL(request.url)
  const state = url.searchParams.get(`state`) ?? ``
  if (state.length === 0 || state.length > MAX_STATE) return expiredPage()

  const [flow] = await db
    .select()
    .from(mcpOauthFlows)
    .where(eq(mcpOauthFlows.state, state))
    .limit(1)
  if (!flow || !flowAcceptsCode(flow)) return expiredPage()

  // Only the member who started this flow may complete it, in a browser
  // signed in as them. Anything else burns the flow (the code is never
  // exchanged).
  if ((await sessionUserId(request)) !== flow.userId) {
    await claimFlow(db, flow.id, {
      ok: false,
      error: `completed outside the starting member's Exponential session`,
    })
    return wrongAccountPage()
  }

  const [server] = await db
    .select({ name: mcpServers.name })
    .from(mcpServers)
    .where(eq(mcpServers.id, flow.serverId))
    .limit(1)
  const name = server?.name ?? `the MCP server`
  const returnTo = safeReturnTo(flow.returnTo)

  const fail = (error: string): Response => {
    if (returnTo) {
      return redirectTo(
        returnUrl(returnTo, { mcp: `failed`, server: flow.serverId, error })
      )
    }
    return mcpOauthPageResponse({
      ok: false,
      title: `Sign-in failed`,
      body: `Connecting ${name} failed: ${error}`,
    })
  }

  // The provider refused (or the user cancelled): the flow fails with the
  // provider's error code; nothing to exchange.
  const providerError = url.searchParams.get(`error`)
  const code = url.searchParams.get(`code`) ?? ``
  if (providerError || code.length === 0 || code.length > MAX_CODE) {
    const description = url.searchParams.get(`error_description`)
    const error = (
      providerError
        ? description
          ? `${providerError}: ${description}`
          : providerError
        : `no code`
    ).slice(0, MAX_ERROR)
    if (!(await claimFlow(db, flow.id, { ok: false, error }))) return expiredPage()
    return fail(error)
  }

  // Claim BEFORE the exchange so a redelivered redirect (browser back,
  // double load) finds the flow gone and cannot spend the code twice.
  if (!(await claimFlow(db, flow.id, { ok: true }))) return expiredPage()
  try {
    await completeFlow(db, flow, code)
  } catch (e) {
    const error = (e instanceof Error ? e.message : `token exchange failed`).slice(
      0,
      MAX_ERROR
    )
    await db
      .update(mcpOauthFlows)
      .set({ status: `failed`, error })
      .where(eq(mcpOauthFlows.id, flow.id))
    return fail(error)
  }

  if (returnTo) {
    return redirectTo(returnUrl(returnTo, { mcp: `connected`, server: flow.serverId }))
  }
  return mcpOauthPageResponse({
    ok: true,
    title: `Connected`,
    body: `Connected to ${name}. You can close this tab.`,
  })
}
