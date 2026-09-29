// The OAuth client behind team MCP servers, run by the SERVER (ported from
// the desktop's former crates/coding/src/mcp_oauth.rs): discovery (RFC 9728
// protected-resource metadata → RFC 8414 / OIDC authorization-server
// metadata), the client-id decision (CIMD when the instance is a public
// https app base and the provider advertises it, else RFC 7591 dynamic
// registration cached per (issuer, redirect_uri, registration endpoint) in
// `mcp_oauth_clients`),
// PKCE S256 (always — Linear accepts nothing else), the authorize URL, the
// code exchange and the refresh (a rotated refresh token replaces the old).
//
// Provider quirks baked in rather than documented away: Sentry serves its
// PRM ONLY at the path-suffixed well-known and 500s when a CIMD document is
// unreachable (a non-public app base MUST take DCR); Notion's single scope
// is `default` (passed through as-is); Linear is S256-only. Never log a
// token, a code or a verifier — errors carry the provider's error text only.
//
// Every endpoint a metadata document names must be https (http only while
// the MCP server itself is on the loopback, i.e. a dev box), and an AS
// metadata document must name the issuer it was fetched for (RFC 8414
// §3.3) — a PRM or metadata answer can never point the browser at a
// `javascript:` URL or the registration cache at another issuer's slot.
import { createHash, randomBytes } from "node:crypto"
import { and, eq } from "drizzle-orm"
import { mcpOauthClients, type McpOauthClient } from "@/db/schema"
import type { Context } from "@/lib/trpc"
import { isPublicHttpsBase } from "@/lib/mcp-oauth/urls"
import { clientSecretAad, decryptSecret, encryptSecret } from "@/lib/mcp-oauth/crypto"
import { McpHttpError, mcpFetch } from "@/lib/mcp-oauth/net"

export type Db = Context[`db`]
/** A handle that can only read (the db or an open transaction). */
export type DbReader = Pick<Db, `select`>

export const CIMD_PATH = `/api/mcp-oauth/client.json`
const CLIENT_NAME = `Exponential`

export class McpOauthError extends Error {}

export interface Discovery {
  issuer: string
  authorizationEndpoint: string
  tokenEndpoint: string
  registrationEndpoint: string | null
  /** `client_id_metadata_document_supported` (the CIMD signal). */
  cimdSupported: boolean
  /** The PRM's `scopes_supported` (the AS's when the PRM has none). */
  scopesSupported: string[]
  /** Whether a protected-resource metadata document answered at all. */
  prmFound: boolean
}

/** The well-known candidates for `suffix` at `url`: the path-suffixed form
 * FIRST (RFC 9728 §3.1 / RFC 8414 §3.1 — Sentry answers only there), then
 * the root. */
export function wellKnownCandidates(url: string, suffix: string): string[] {
  const parsed = new URL(url)
  const path = parsed.pathname.replace(/\/+$/, ``)
  const origin = parsed.origin
  const out: string[] = []
  if (path.length > 0) out.push(`${origin}/.well-known/${suffix}${path}`)
  out.push(`${origin}/.well-known/${suffix}`)
  return out
}

/** GET a JSON document; null for a non-2xx or non-JSON answer (discovery
 * probes several URLs and only cares which one speaks). Transport failures
 * throw (no point probing the next one). */
async function fetchJson(url: string): Promise<Record<string, unknown> | null> {
  const response = await mcpFetch(url, { headers: { accept: `application/json` } })
  if (!response.ok) return null
  try {
    const doc: unknown = await response.json()
    return doc && typeof doc === `object` && !Array.isArray(doc)
      ? (doc as Record<string, unknown>)
      : null
  } catch {
    return null
  }
}

function stringList(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === `string`)
    : []
}

const trimSlash = (value: string) => value.replace(/\/+$/, ``)

/** Whether `serverUrl` is a loopback http MCP server (a dev box) — the one
 * case its OAuth endpoints may be plain http too. */
function isLoopbackHttp(serverUrl: string): boolean {
  try {
    const url = new URL(serverUrl)
    return (
      url.protocol === `http:` &&
      (url.hostname === `localhost` || url.hostname === `127.0.0.1`)
    )
  } catch {
    return false
  }
}

/** `value` as an endpoint the sign-in may use for `serverUrl`: an absolute
 * https URL (http only when the MCP server itself is loopback http), else
 * McpOauthError naming `what`. Returns the parsed URL's string form. */
export function assertEndpointUrl(value: string, serverUrl: string, what: string): string {
  let url: URL
  try {
    url = new URL(value)
  } catch {
    throw new McpOauthError(`the provider's ${what} is not an absolute URL`)
  }
  if (url.protocol === `https:`) return url.toString()
  if (url.protocol === `http:` && isLoopbackHttp(serverUrl)) return url.toString()
  throw new McpOauthError(`the provider's ${what} must be an https:// URL`)
}

/** RFC 9728 + RFC 8414 (then OIDC) discovery for `serverUrl`. */
export async function discover(serverUrl: string): Promise<Discovery> {
  let issuer = new URL(serverUrl).origin
  let scopes: string[] = []
  let prmFound = false
  for (const candidate of wellKnownCandidates(serverUrl, `oauth-protected-resource`)) {
    const doc = await fetchJson(candidate)
    if (!doc) continue
    const servers = stringList(doc.authorization_servers)
    if (servers[0]) {
      issuer = trimSlash(assertEndpointUrl(servers[0], serverUrl, `authorization server`))
    }
    scopes = stringList(doc.scopes_supported)
    prmFound = true
    break
  }
  const candidates = [
    ...wellKnownCandidates(issuer, `oauth-authorization-server`),
    ...wellKnownCandidates(issuer, `openid-configuration`),
  ]
  let mismatched: string | null = null
  for (const candidate of candidates) {
    const doc = await fetchJson(candidate)
    if (!doc) continue
    const authorizationEndpoint = doc.authorization_endpoint
    const tokenEndpoint = doc.token_endpoint
    if (typeof authorizationEndpoint !== `string` || typeof tokenEndpoint !== `string`) {
      continue
    }
    // RFC 8414 §3.3 / OIDC Discovery §4.3: the document MUST name the
    // issuer it was fetched for; anything else (a root document answering
    // for a path issuer, a spoofed one) is not this issuer's metadata.
    if (typeof doc.issuer !== `string` || trimSlash(doc.issuer) !== issuer) {
      mismatched ??= typeof doc.issuer === `string` ? doc.issuer.slice(0, 200) : `none`
      continue
    }
    if (scopes.length === 0) scopes = stringList(doc.scopes_supported)
    return {
      issuer,
      authorizationEndpoint: assertEndpointUrl(
        authorizationEndpoint,
        serverUrl,
        `authorization endpoint`
      ),
      tokenEndpoint: assertEndpointUrl(tokenEndpoint, serverUrl, `token endpoint`),
      registrationEndpoint:
        typeof doc.registration_endpoint === `string`
          ? assertEndpointUrl(doc.registration_endpoint, serverUrl, `registration endpoint`)
          : null,
      cimdSupported: doc.client_id_metadata_document_supported === true,
      scopesSupported: scopes,
      prmFound,
    }
  }
  if (mismatched !== null) {
    throw new McpOauthError(
      `the authorization server metadata names issuer ${mismatched}, not ${issuer}`
    )
  }
  throw new McpOauthError(
    `no OAuth authorization server found for this MCP server (no metadata at ${issuer})`
  )
}

export function cimdClientId(appBase: string): string {
  return `${trimSlash(appBase)}${CIMD_PATH}`
}

/** `error_description` / `error` of an OAuth error body, else a stub. */
function oauthErrorText(doc: unknown): string {
  const rec = (doc && typeof doc === `object` ? doc : {}) as Record<string, unknown>
  const text = rec.error_description ?? rec.error
  return typeof text === `string` ? text.slice(0, 300) : `no error body`
}

export interface OauthClient {
  clientId: string
  clientSecret: string | null
  /** `none` | `client_secret_post` | `client_secret_basic`. */
  authMethod: string
}

/** Which client to use with `discovery`: CIMD when the provider supports it
 * AND `appBase` is public https; else the cached dynamic registration for
 * (issuer, redirectUri, registration endpoint), registering one on a miss;
 * else the provider needs a pre-registered client we cannot supply. */
export async function chooseClient(
  db: Db,
  discovery: Discovery,
  appBase: string,
  redirectUri: string
): Promise<OauthClient> {
  if (discovery.cimdSupported && isPublicHttpsBase(appBase)) {
    return { clientId: cimdClientId(appBase), clientSecret: null, authMethod: `none` }
  }
  const registrationEndpoint = discovery.registrationEndpoint
  if (!registrationEndpoint) {
    throw new McpOauthError(
      `this provider requires a pre-registered OAuth client, which Exponential cannot supply`
    )
  }
  const [cachedRow] = await db
    .select()
    .from(mcpOauthClients)
    .where(
      and(
        eq(mcpOauthClients.issuer, discovery.issuer),
        eq(mcpOauthClients.redirectUri, redirectUri),
        eq(mcpOauthClients.registrationEndpoint, registrationEndpoint)
      )
    )
    .limit(1)
  const cached = cachedRow ? clientFromRow(cachedRow) : null
  if (cached) return cached
  const response = await mcpFetch(registrationEndpoint, {
    method: `POST`,
    headers: { "content-type": `application/json`, accept: `application/json` },
    body: JSON.stringify({
      client_name: CLIENT_NAME,
      redirect_uris: [redirectUri],
      grant_types: [`authorization_code`, `refresh_token`],
      response_types: [`code`],
      token_endpoint_auth_method: `none`,
    }),
  })
  const doc = (await response.json().catch(() => null)) as Record<string, unknown> | null
  const clientId = typeof doc?.client_id === `string` ? doc.client_id : ``
  if (!response.ok || clientId.length === 0) {
    throw new McpOauthError(
      `client registration failed (HTTP ${response.status}): ${oauthErrorText(doc)}`
    )
  }
  const clientSecret =
    typeof doc?.client_secret === `string` && doc.client_secret.length > 0
      ? doc.client_secret
      : null
  const authMethod =
    typeof doc?.token_endpoint_auth_method === `string`
      ? doc.token_endpoint_auth_method.slice(0, 32)
      : clientSecret
        ? `client_secret_post`
        : `none`
  const clientSecretCiphertext = clientSecret
    ? encryptSecret(clientSecret, clientSecretAad(discovery.issuer, registrationEndpoint))
    : null
  await db
    .insert(mcpOauthClients)
    .values({
      issuer: discovery.issuer,
      redirectUri,
      registrationEndpoint,
      clientId,
      clientSecretCiphertext,
      tokenEndpointAuthMethod: authMethod,
    })
    .onConflictDoUpdate({
      target: [
        mcpOauthClients.issuer,
        mcpOauthClients.redirectUri,
        mcpOauthClients.registrationEndpoint,
      ],
      set: { clientId, clientSecretCiphertext, tokenEndpointAuthMethod: authMethod },
    })
  return { clientId, clientSecret, authMethod }
}

/** A cached registration as a client, or null when a confidential client's
 * secret no longer decrypts (rotated BETTER_AUTH_SECRET, a row copied from
 * another issuer/endpoint) — useless, so the caller registers afresh. */
function clientFromRow(row: McpOauthClient): OauthClient | null {
  const clientSecret = decryptSecret(
    row.clientSecretCiphertext,
    clientSecretAad(row.issuer, row.registrationEndpoint)
  )
  if (row.clientSecretCiphertext && clientSecret === null) return null
  return { clientId: row.clientId, clientSecret, authMethod: row.tokenEndpointAuthMethod }
}

/** The client the TOKENS belong to, for a refresh: CIMD ids need no row;
 * a DCR id is looked up by (issuer, redirect, client id). */
export async function clientForRefresh(
  db: DbReader,
  issuer: string | null,
  clientId: string,
  redirectUri: string
): Promise<OauthClient> {
  if (issuer) {
    const [row] = await db
      .select()
      .from(mcpOauthClients)
      .where(
        and(
          eq(mcpOauthClients.issuer, issuer),
          eq(mcpOauthClients.redirectUri, redirectUri),
          eq(mcpOauthClients.clientId, clientId)
        )
      )
      .limit(1)
    const cached = row ? clientFromRow(row) : null
    if (cached) return cached
  }
  return { clientId, clientSecret: null, authMethod: `none` }
}

/** A fresh PKCE S256 pair (43-char base64url verifier, RFC 7636). */
export function generatePkce(): { verifier: string; challenge: string } {
  const verifier = randomBytes(32).toString(`base64url`)
  const challenge = createHash(`sha256`).update(verifier).digest(`base64url`)
  return { verifier, challenge }
}

/** The authorize URL: code flow, the client id, the redirect, PKCE S256,
 * the state, the scope when there is one, and RFC 8707 `resource` = the
 * server URL. The endpoint's scheme is re-checked here (https, http only for
 * a loopback server): this URL is what a member's browser navigates to. */
export function authorizeUrl(args: {
  authorizationEndpoint: string
  clientId: string
  redirectUri: string
  codeChallenge: string
  state: string
  scope: string | null
  resource: string
}): string {
  const url = new URL(
    assertEndpointUrl(args.authorizationEndpoint, args.resource, `authorization endpoint`)
  )
  url.searchParams.set(`response_type`, `code`)
  url.searchParams.set(`client_id`, args.clientId)
  url.searchParams.set(`redirect_uri`, args.redirectUri)
  url.searchParams.set(`code_challenge`, args.codeChallenge)
  url.searchParams.set(`code_challenge_method`, `S256`)
  url.searchParams.set(`state`, args.state)
  if (args.scope && args.scope.trim().length > 0) {
    url.searchParams.set(`scope`, args.scope)
  }
  url.searchParams.set(`resource`, args.resource)
  return url.toString()
}

/** The row's scopes joined by space, else the discovered
 * `scopes_supported`, else none. */
export function scopeFor(rowScopes: string[], discovery: Pick<Discovery, `scopesSupported`>): string | null {
  const picked = rowScopes.length > 0 ? rowScopes : discovery.scopesSupported
  const joined = picked
    .map((scope) => scope.trim())
    .filter(Boolean)
    .join(` `)
  return joined.length > 0 ? joined : null
}

export interface TokenSet {
  accessToken: string
  refreshToken: string | null
  tokenType: string
  expiresAt: Date | null
}

function parseTokenResponse(
  doc: Record<string, unknown>,
  now: Date,
  previousRefresh: string | null
): TokenSet {
  const accessToken = typeof doc.access_token === `string` ? doc.access_token : ``
  if (accessToken.length === 0) {
    throw new McpOauthError(`token response had no access token: ${oauthErrorText(doc)}`)
  }
  const rawExpiry = doc.expires_in
  const expiresIn =
    typeof rawExpiry === `number`
      ? rawExpiry
      : typeof rawExpiry === `string`
        ? Number.parseInt(rawExpiry, 10)
        : NaN
  return {
    accessToken,
    refreshToken:
      typeof doc.refresh_token === `string` && doc.refresh_token.length > 0
        ? doc.refresh_token
        : previousRefresh,
    tokenType: typeof doc.token_type === `string` ? doc.token_type : `Bearer`,
    expiresAt:
      Number.isFinite(expiresIn) && expiresIn > 0
        ? new Date(now.getTime() + expiresIn * 1000)
        : null,
  }
}

async function postToken(
  tokenEndpoint: string,
  client: OauthClient,
  params: Record<string, string>
): Promise<Record<string, unknown>> {
  const body = new URLSearchParams(params)
  const headers: Record<string, string> = {
    "content-type": `application/x-www-form-urlencoded`,
    accept: `application/json`,
  }
  if (client.clientSecret && client.authMethod === `client_secret_basic`) {
    const basic = `${encodeURIComponent(client.clientId)}:${encodeURIComponent(client.clientSecret)}`
    headers.authorization = `Basic ${Buffer.from(basic).toString(`base64`)}`
  } else {
    body.set(`client_id`, client.clientId)
    if (client.clientSecret) body.set(`client_secret`, client.clientSecret)
  }
  let response: Response
  try {
    response = await mcpFetch(tokenEndpoint, { method: `POST`, headers, body: body.toString() })
  } catch (e) {
    throw new McpOauthError(
      `token request failed: ${e instanceof McpHttpError ? e.message : `request failed`}`
    )
  }
  const doc = (await response.json().catch(() => null)) as Record<string, unknown> | null
  if (!response.ok) {
    throw new McpOauthError(
      `token request refused (HTTP ${response.status}): ${oauthErrorText(doc)}`
    )
  }
  return doc ?? {}
}

/** Exchange the authorization `code` (PKCE verifier, no secret unless the
 * DCR client is confidential). */
export async function exchangeCode(args: {
  tokenEndpoint: string
  client: OauthClient
  code: string
  redirectUri: string
  codeVerifier: string
  resource: string
  now?: Date
}): Promise<TokenSet> {
  const doc = await postToken(args.tokenEndpoint, args.client, {
    grant_type: `authorization_code`,
    code: args.code,
    redirect_uri: args.redirectUri,
    code_verifier: args.codeVerifier,
    resource: args.resource,
  })
  return parseTokenResponse(doc, args.now ?? new Date(), null)
}

/** Rotate with the refresh token (`resource` = the server URL). The old
 * refresh token is kept when the provider returns none. */
export async function refreshTokens(args: {
  tokenEndpoint: string
  client: OauthClient
  refreshToken: string
  resource: string
  now?: Date
}): Promise<TokenSet> {
  const doc = await postToken(args.tokenEndpoint, args.client, {
    grant_type: `refresh_token`,
    refresh_token: args.refreshToken,
    resource: args.resource,
  })
  return parseTokenResponse(doc, args.now ?? new Date(), args.refreshToken)
}
