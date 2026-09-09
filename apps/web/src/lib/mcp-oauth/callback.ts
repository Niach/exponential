// EXP-792: the hosted redirect target of a device-executed MCP OAuth
// sign-in. ANONYMOUS by necessity (the provider redirects whatever browser
// consented, which may hold no Exponential session) and gated on `state`
// alone: the state is 32 random bytes minted by beginOAuth, single-use (the
// flow leaves `pending`/`authorize_url` the moment a code lands) and
// 10-minute bound. The code itself is worthless without the PKCE verifier,
// which never left the device — so this route only RELAYS it there as an
// `mcp_oauth_code` command and never logs, echoes or stores it beyond that
// row. Every failure answers a plain 200 page with no detail: an attacker
// probing states learns nothing but "expired".
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { deviceCommands, devices, mcpOauthFlows, mcpServers } from "@/db/schema"
import {
  TokenBucketLimiter,
  clientIpFromRequest,
  envInt,
} from "@/lib/widget/rate-limit"
import { nudgeDevice } from "@/lib/trpc/devices"
import { finishFlow, flowAcceptsCode } from "@/lib/mcp-oauth/flows"
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
    .select({
      id: mcpOauthFlows.id,
      status: mcpOauthFlows.status,
      createdAt: mcpOauthFlows.createdAt,
      serverId: mcpOauthFlows.serverId,
      deviceRowId: mcpOauthFlows.deviceRowId,
      userId: mcpOauthFlows.userId,
    })
    .from(mcpOauthFlows)
    .where(eq(mcpOauthFlows.state, state))
    .limit(1)
  if (!flow || !flowAcceptsCode(flow)) return expiredPage()

  // The provider refused (or the user cancelled): the flow fails with the
  // provider's error code so the web dialog can say why; no code to relay.
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
    await finishFlow(db, flow.id, { ok: false, error })
    return mcpOauthPageResponse({
      ok: false,
      title: `Sign-in refused`,
      body: `Sign-in was refused: ${error}`,
    })
  }

  // Relay the code to the device: the command row and the flow transition
  // land together so a redelivered redirect (browser back, double load)
  // finds the flow already past `authorize_url` and gets the expired page.
  await db.transaction(async (tx) => {
    const moved = await tx
      .update(mcpOauthFlows)
      .set({ status: `code_relayed` })
      .where(
        and(
          eq(mcpOauthFlows.id, flow.id),
          eq(mcpOauthFlows.status, flow.status)
        )
      )
      .returning({ id: mcpOauthFlows.id })
    if (moved.length === 0) return
    await tx.insert(deviceCommands).values({
      deviceRowId: flow.deviceRowId,
      userId: flow.userId,
      kind: `mcp_oauth_code`,
      payload: { serverId: flow.serverId, state, code },
    })
  })

  const [device] = await db
    .select({ deviceId: devices.deviceId, userId: devices.userId })
    .from(devices)
    .where(eq(devices.id, flow.deviceRowId))
    .limit(1)
  if (device) nudgeDevice(device.userId, device.deviceId)

  const [server] = await db
    .select({ name: mcpServers.name })
    .from(mcpServers)
    .where(eq(mcpServers.id, flow.serverId))
    .limit(1)
  const name = server?.name ?? `the MCP server`
  return mcpOauthPageResponse({
    ok: true,
    title: `Signed in`,
    body: `Signed in to ${name}. You can close this tab.`,
  })
}
