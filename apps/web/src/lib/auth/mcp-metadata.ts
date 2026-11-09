import { oauthProviderAuthServerMetadata } from "@better-auth/oauth-provider"
import { auth } from "@/lib/auth"
import { protectedResourceMetadata } from "./mcp-oauth-config"

// The discovery documents MCP clients read (RFC 8414, RFC 9728), with the
// CORS headers the 1.6 plugin served them with (browser-based clients).
const CORS = {
  "Access-Control-Allow-Origin": `*`,
  "Access-Control-Allow-Methods": `GET, OPTIONS`,
  "Access-Control-Allow-Headers": `Content-Type, Authorization`,
  "Access-Control-Max-Age": `86400`,
}

/** RFC 8414 authorization-server metadata: the issuer is Better Auth's base
 * URL (`<origin>/api/auth`), served at the root well-known path (where 1.6
 * clients look) and at the path-inserted one (`…/oauth-authorization-server/
 * api/auth`, where clients resolving the new issuer look). */
export const authorizationServerMetadata = oauthProviderAuthServerMetadata(auth, { headers: CORS })

/** RFC 9728 metadata of one protected resource. */
export function protectedResourceResponse(resource: string): Response {
  return new Response(JSON.stringify(protectedResourceMetadata(resource)), {
    status: 200,
    headers: { "Content-Type": `application/json`, "Cache-Control": `no-store`, ...CORS },
  })
}
