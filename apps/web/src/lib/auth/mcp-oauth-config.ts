import type { McpOptions } from "@better-auth/mcp"

// The MCP OAuth authorization server's configuration (better-auth 1.7,
// `@better-auth/mcp` on `@better-auth/oauth-provider`), kept beside the auth
// config so the real-DB flow test builds the SAME plugin. Every choice here
// keeps what the 1.6 `mcp` plugin did for connected clients:
//
// - OPAQUE access tokens (`disableJwtPlugin`): stored hashed, looked up by
//   lib/emcp/scope.ts, gone the moment a client is revoked (a JWT would
//   outlive its revocation by its whole lifetime).
// - Open Dynamic Client Registration: MCP clients (Claude Code & co.)
//   register themselves on first connect, as they did on /mcp/register;
//   a client naming no `application_type` with a loopback or custom-scheme
//   callback registers as native (1.6 accepted those callbacks; the 1.7
//   default "web" refuses them).
// - The grants 1.6 offered: authorization code + refresh (no
//   client_credentials).
// - Both protected resources (`/api/mcp`, `/api/mcp/apps`) open to every
//   client: 1.6 had no per-client resource binding.
// - The same lifetimes: a one-day access token, a 90-day refresh window.

export const MCP_ACCESS_TOKEN_TTL_SECONDS = 60 * 60 * 24
export const MCP_REFRESH_TOKEN_TTL_SECONDS = 60 * 60 * 24 * 90

// Better Auth's own fallback base when BETTER_AUTH_URL is unset (tests, a
// bare boot smoke); every real deployment sets it.
const DEFAULT_BASE_URL = `http://localhost:3000`

function baseUrl(env: NodeJS.ProcessEnv = process.env): string {
  return (env.BETTER_AUTH_URL || DEFAULT_BASE_URL).replace(/\/$/, ``)
}

/** The RFC 8707 resource of the builtin MCP endpoint. */
export function mcpResource(env: NodeJS.ProcessEnv = process.env): string {
  return `${baseUrl(env)}/api/mcp`
}

/** #VAPP-182: the apps endpoint's resource. */
export function mcpAppsResource(env: NodeJS.ProcessEnv = process.env): string {
  return `${baseUrl(env)}/api/mcp/apps`
}

/** The issuer oauth-provider stamps (`iss`, metadata): Better Auth's base URL. */
export function mcpIssuer(env: NodeJS.ProcessEnv = process.env): string {
  return `${baseUrl(env)}/api/auth`
}

export function mcpOAuthOptions(env: NodeJS.ProcessEnv = process.env): McpOptions {
  return {
    loginPage: `/auth/login`,
    // Scope-selection consent screen (team/board multi-select → mcp_grants).
    // The /api/auth/$ route forces prompt=consent on every authorize request
    // so no client skips it.
    consentPage: `/auth/consent`,
    resource: mcpResource(env),
    resources: [{ identifier: mcpAppsResource(env), name: `Exponential apps` }],
    enforcePerClientResources: false,
    disableJwtPlugin: true,
    accessTokenExpiresIn: MCP_ACCESS_TOKEN_TTL_SECONDS,
    refreshTokenExpiresIn: MCP_REFRESH_TOKEN_TTL_SECONDS,
    allowDynamicClientRegistration: true,
    allowUnauthenticatedClientRegistration: true,
    clientRegistrationDefaultApplicationType: `infer`,
    grantTypes: [`authorization_code`, `refresh_token`],
  }
}

/** RFC 9728 metadata of one of our protected resources: the shape the 1.6
 * plugin served (resource, its authorization server, header bearer tokens,
 * the scopes), naming the issuer the authorization server now reports. */
export function protectedResourceMetadata(resource: string, env: NodeJS.ProcessEnv = process.env) {
  return {
    resource,
    authorization_servers: [mcpIssuer(env)],
    scopes_supported: [`openid`, `profile`, `email`, `offline_access`],
    bearer_methods_supported: [`header`],
  }
}
