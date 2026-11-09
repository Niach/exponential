import { createHash } from "node:crypto"

/** How `@better-auth/oauth-provider` stores opaque access tokens, refresh
 * tokens and authorization codes (`storeTokens: "hashed"`, its default):
 * SHA-256 of the UTF-8 token, base64url without padding. The MCP token
 * lookup (lib/emcp/scope.ts) and the 1.6 import (legacy-mcp-oauth.ts) hash
 * exactly as the provider does; the real-DB flow test pins the equality. */
export function hashOAuthToken(token: string): string {
  return createHash(`sha256`).update(token, `utf8`).digest(`base64url`)
}
