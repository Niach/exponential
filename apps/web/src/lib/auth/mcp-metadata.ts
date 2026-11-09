import { oauthProviderAuthServerMetadata } from "@better-auth/oauth-provider"
import { auth } from "@/lib/auth"
import { METADATA_CORS as CORS } from "./mcp-oauth-config"

export { protectedResourceResponse } from "./mcp-oauth-config"

/** RFC 8414 authorization-server metadata: the issuer is Better Auth's base
 * URL (`<origin>/api/auth`), served at the root well-known path (where 1.6
 * clients look) and at the path-inserted one (`…/oauth-authorization-server/
 * api/auth`, where clients resolving the new issuer look). */
export const authorizationServerMetadata = oauthProviderAuthServerMetadata(auth, { headers: CORS })
