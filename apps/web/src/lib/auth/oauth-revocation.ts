import { and, eq, isNotNull, or } from "drizzle-orm"
import { accounts } from "@/db/auth-schema"
import type { db as Database } from "@/db/connection"
import { parseOidcProviders } from "@/lib/oidc-providers"
import { revokeAppleTokensBestEffort } from "./apple-revocation"

// Revoke every OAuth grant a deleted account still holds at its providers.
//
// The `accounts` row cascades away with the users row, which DISCARDS the
// tokens instead of revoking them: the provider keeps listing this app under
// "third-party apps with account access", and the stored credential stays
// valid upstream (Google is configured `accessType: 'offline'`, so the very
// first sign-in mints a long-lived refresh token). Capture the rows BEFORE the
// delete transaction, then RFC 7009 them best-effort after commit.
//
// Apple keeps its own module (App Store guideline 5.1.1(v), minted ES256
// client secret) — Apple rows are dispatched there; everything else is handled
// here: Google by its documented endpoint, generic OIDC by the
// `revocation_endpoint` advertised in the provider's discovery document.

// Google's revoke endpoint takes the bare token (no client authentication) and
// revoking either half of a grant kills the whole grant.
const GOOGLE_REVOKE_ENDPOINT = `https://oauth2.googleapis.com/revoke`

// A wedged provider must not stall the tRPC response — deletion already
// committed by the time these run.
const REVOKE_TIMEOUT_MS = 5_000

// Works over the root db or a transaction — structurally typed so it can run
// wherever the caller needs it (only `.select` is used).
type DbOrTx = Pick<typeof Database, `select`>

export interface OAuthTokenRow {
  providerId: string
  accessToken: string | null
  refreshToken: string | null
}

export interface RevocationTarget {
  endpoint: string
  // RFC 7009 §2.1 client authentication, sent in the body when the provider
  // expects it. Omitted for Google, whose endpoint is documented as taking the
  // token alone.
  clientId?: string
  clientSecret?: string
}

// Capture the user's token-bearing `accounts` rows BEFORE the delete
// transaction — the user FK cascades on delete, after which the tokens are
// gone. Password (`credential`) rows and native-idToken pairings store no
// tokens and are filtered out here, so an account with nothing to revoke costs
// one query and no requests.
export async function captureOAuthTokens(
  db: DbOrTx,
  userId: string
): Promise<OAuthTokenRow[]> {
  return db
    .select({
      providerId: accounts.providerId,
      accessToken: accounts.accessToken,
      refreshToken: accounts.refreshToken,
    })
    .from(accounts)
    .where(
      and(
        eq(accounts.userId, userId),
        or(isNotNull(accounts.accessToken), isNotNull(accounts.refreshToken))
      )
    )
}

// Prefer the refresh token: RFC 7009 §2.1 says revoking it SHOULD invalidate
// the access tokens issued from the same grant (Apple and Google both do), so
// one request retires the whole pairing.
export function pickRevocableToken(row: {
  accessToken: string | null
  refreshToken: string | null
}): { token: string; tokenTypeHint: `refresh_token` | `access_token` } | null {
  const refresh = row.refreshToken?.trim()
  if (refresh) return { token: refresh, tokenTypeHint: `refresh_token` }
  const access = row.accessToken?.trim()
  if (access) return { token: access, tokenTypeHint: `access_token` }
  return null
}

// The x-www-form-urlencoded body of an RFC 7009 revocation request. Pure so
// the field shape stays unit-testable — a wrong field name silently no-ops the
// revocation.
export function buildRevocationBody(params: {
  token: string
  tokenTypeHint: `refresh_token` | `access_token`
  clientId?: string
  clientSecret?: string
}): string {
  const body = new URLSearchParams()
  body.set(`token`, params.token)
  body.set(`token_type_hint`, params.tokenTypeHint)
  if (params.clientId) body.set(`client_id`, params.clientId)
  if (params.clientSecret) body.set(`client_secret`, params.clientSecret)
  return body.toString()
}

// discoveryUrl → the READ discovery document (`{ endpoint }`, endpoint null
// when the provider advertises none) or null when the document could not be
// read at all. Only the former is cached per process: the document is static
// config and a deletion may carry several rows for the same provider, but a
// failure is NOT config — caching it would let one 502 or timeout silently
// disable OIDC revocation for the rest of the process lifetime (every later
// deletion would short-circuit on the cached null, without a request or a
// log). Failures are evicted below so the next deletion retries.
const oidcRevocationEndpoints = new Map<
  string,
  Promise<{ endpoint: string | null } | null>
>()

async function fetchOidcRevocationEndpoint(
  discoveryUrl: string
): Promise<string | null> {
  const cached = oidcRevocationEndpoints.get(discoveryUrl)
  if (cached) return (await cached)?.endpoint ?? null
  // Never rejects: a failure resolves to null so the map never holds a rejected
  // promise, and the eviction below is ordering-safe even if `fetch` throws
  // synchronously.
  const pending = (async () => {
    try {
      const res = await fetch(discoveryUrl, {
        signal: AbortSignal.timeout(REVOKE_TIMEOUT_MS),
      })
      if (!res.ok) {
        console.error(
          `[oauth-revoke] discovery for ${discoveryUrl} responded ${res.status}`
        )
        return null
      }
      const doc = (await res.json()) as { revocation_endpoint?: unknown }
      return {
        endpoint:
          typeof doc.revocation_endpoint === `string`
            ? doc.revocation_endpoint
            : null,
      }
    } catch (err) {
      console.error(
        `[oauth-revoke] discovery fetch failed for ${discoveryUrl}:`,
        err
      )
      return null
    }
  })()
  oidcRevocationEndpoints.set(discoveryUrl, pending)
  const resolved = await pending
  if (!resolved) {
    // Identity-checked so a retry that already replaced this entry survives.
    if (oidcRevocationEndpoints.get(discoveryUrl) === pending) {
      oidcRevocationEndpoints.delete(discoveryUrl)
    }
    return null
  }
  return resolved.endpoint
}

// Where (and how) a provider's tokens are revoked. Null for providers with no
// revocation endpoint we can reach — nothing is sent for those.
export async function resolveRevocationTarget(
  providerId: string
): Promise<RevocationTarget | null> {
  // No env gate: a google row can only exist if this instance issued it, and
  // the endpoint needs no client credentials — so it stays revocable even after
  // the Google client config is rotated away.
  if (providerId === `google`) return { endpoint: GOOGLE_REVOKE_ENDPOINT }

  const provider = parseOidcProviders().find((p) => p.id === providerId)
  if (!provider) return null
  const endpoint = await fetchOidcRevocationEndpoint(provider.discoveryUrl)
  if (!endpoint) return null
  return {
    endpoint,
    clientId: provider.clientId,
    clientSecret: provider.clientSecret,
  }
}

/**
 * Best-effort revoke of every captured grant, mirroring
 * cancelCreemSubscriptionsBestEffort: never throws (a delete must never be
 * blocked by a provider being unreachable), logs loudly on failure. Apple rows
 * go through the Apple module; the rest are RFC 7009 POSTs.
 */
export async function revokeOAuthTokensBestEffort(
  rows: OAuthTokenRow[]
): Promise<void> {
  if (rows.length === 0) return

  await revokeAppleTokensBestEffort(
    rows.filter((r) => r.providerId === `apple`)
  )

  for (const row of rows) {
    if (row.providerId === `apple`) continue
    const picked = pickRevocableToken(row)
    if (!picked) continue
    let target: RevocationTarget | null = null
    try {
      target = await resolveRevocationTarget(row.providerId)
    } catch (err) {
      console.error(
        `[oauth-revoke] failed to resolve the ${row.providerId} revocation endpoint:`,
        err
      )
    }
    if (!target) continue

    try {
      const res = await fetch(target.endpoint, {
        method: `POST`,
        headers: { "content-type": `application/x-www-form-urlencoded` },
        body: buildRevocationBody({
          token: picked.token,
          tokenTypeHint: picked.tokenTypeHint,
          clientId: target.clientId,
          clientSecret: target.clientSecret,
        }),
        signal: AbortSignal.timeout(REVOKE_TIMEOUT_MS),
      })
      if (!res.ok) {
        const detail = await res.text().catch(() => ``)
        console.error(
          `[oauth-revoke] ${row.providerId} revoke failed: ${res.status} ${detail}`.trim()
        )
      }
    } catch (err) {
      console.error(
        `[oauth-revoke] ${row.providerId} revoke request threw:`,
        err
      )
    }
  }
}
