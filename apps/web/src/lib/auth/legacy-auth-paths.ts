// better-auth 1.7 moved two families of endpoints; clients that learned the
// 1.6 paths keep working through these aliases (routes/api/auth/$.ts):
//
// - MCP OAuth: /api/auth/mcp/{authorize,token,register,userinfo} →
//   /api/auth/oauth2/*. MCP clients cache the authorization-server metadata
//   (and with it the token endpoint they refresh against) from their first
//   connect.
// - Generic OIDC callbacks: /api/auth/oauth2/callback/:id → /api/auth/callback/:id.
//   Every self-hosted IdP has the 1.6 URI registered, and the generic
//   providers still send it (lib/auth/index.ts `legacyOidcCallbackUrl`).

const MCP_ALIASES: Record<string, string> = {
  "/api/auth/mcp/authorize": `/api/auth/oauth2/authorize`,
  "/api/auth/mcp/token": `/api/auth/oauth2/token`,
  "/api/auth/mcp/register": `/api/auth/oauth2/register`,
  "/api/auth/mcp/userinfo": `/api/auth/oauth2/userinfo`,
}

const LEGACY_OIDC_CALLBACK = /^\/api\/auth\/oauth2\/callback\/([^/]+)\/?$/

/** Pure: the 1.7 path a 1.6 path now lives at, or null for any other path. */
export function currentAuthPath(pathname: string): string | null {
  const mcp = MCP_ALIASES[pathname.replace(/\/$/, ``)]
  if (mcp) return mcp
  const callback = LEGACY_OIDC_CALLBACK.exec(pathname)
  return callback ? `/api/auth/callback/${callback[1]}` : null
}

/** The request re-addressed to its 1.7 path (method, headers and body kept),
 * or the request itself. */
export function withCurrentAuthPath(request: Request): Request {
  const url = new URL(request.url)
  const current = currentAuthPath(url.pathname)
  if (!current) return request
  url.pathname = current
  return new Request(url, request)
}
