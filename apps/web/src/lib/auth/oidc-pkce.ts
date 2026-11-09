// Generic OIDC providers (OIDC_PROVIDERS) use PKCE exactly when their
// discovery document advertises `S256`, decided PER FLOW:
//
// - Sign-in (`/sign-in/social`, `/link-social`, the native
//   mobile-oauth-start): a before hook resolves the IdP's support (cached,
//   re-fetched with a timeout when unknown) and records the decision in the
//   OAuth state's server context (`addOAuthServerContext`), which better-auth
//   seals into the flow's state.
// - The generic providers are wrapped so `createAuthorizationURL` sends a
//   challenge, and `validateAuthorizationCode` sends the verifier, only when
//   THAT flow's state says so (`getOAuthState`). A discovery doc changing
//   mid-flow, or another instance deciding differently, never splits a flow.
//
// Boot never waits on an IdP: the first check runs in the background, and an
// unreachable or unknown IdP means no PKCE, re-checked on the next sign-in.
import type { BetterAuthPlugin } from "better-auth"
import { addOAuthServerContext, createAuthMiddleware, getOAuthState } from "better-auth/api"

/** RFC 8414 / OIDC discovery: does the IdP accept `code_challenge_method=S256`? */
export function supportsS256(discovery: unknown): boolean {
  if (!discovery || typeof discovery !== `object`) return false
  const methods = (discovery as { code_challenge_methods_supported?: unknown }).code_challenge_methods_supported
  return Array.isArray(methods) && methods.includes(`S256`)
}

/** The server-context key carrying a flow's decision through the IdP redirect. */
export const OIDC_PKCE_STATE_KEY = `oidcPkce`

/** A reachable IdP's answer is trusted this long; an unreachable one is re-asked on the next sign-in. */
const CHECKED_TTL_MS = 60 * 60 * 1000
const DISCOVERY_TIMEOUT_MS = 5000

type Checked = { s256: boolean; until: number }

export type OidcPkceProvider = { id: string; discoveryUrl: string }

export function createS256Cache(
  fetchDiscovery: (url: string) => Promise<unknown> = defaultFetchDiscovery,
  now: () => number = Date.now
) {
  const checked = new Map<string, Checked>()
  const inFlight = new Map<string, Promise<boolean>>()
  return {
    /** The IdP's support, fetched when not known; never throws (unreachable = false). */
    resolve(p: OidcPkceProvider): Promise<boolean> {
      const hit = checked.get(p.id)
      if (hit && hit.until > now()) return Promise.resolve(hit.s256)
      const pending = inFlight.get(p.id)
      if (pending) return pending
      const attempt = fetchDiscovery(p.discoveryUrl)
        .then((doc) => {
          const s256 = supportsS256(doc)
          checked.set(p.id, { s256, until: now() + CHECKED_TTL_MS })
          return s256
        })
        .catch(() => {
          checked.delete(p.id)
          return false
        })
        .finally(() => inFlight.delete(p.id))
      inFlight.set(p.id, attempt)
      return attempt
    },
  }
}

async function defaultFetchDiscovery(url: string): Promise<unknown> {
  const res = await fetch(url, { signal: AbortSignal.timeout(DISCOVERY_TIMEOUT_MS) })
  if (!res.ok) throw new Error(`discovery ${res.status}`)
  return res.json()
}

/** This flow's decision, as sealed into its OAuth state at sign-in. */
async function flowUsesPkce(): Promise<boolean> {
  try {
    const state = await getOAuthState()
    const context = state?.serverContext as Record<string, unknown> | undefined
    return context?.[OIDC_PKCE_STATE_KEY] === true
  } catch {
    return false
  }
}

type WrappableProvider = {
  id: string
  createAuthorizationURL: (data: { codeVerifier: string }) => unknown
  validateAuthorizationCode: (data: { codeVerifier: string }) => unknown
}

const wrapped = new WeakSet<object>()

function wrapProvider(provider: WrappableProvider): void {
  if (wrapped.has(provider)) return
  wrapped.add(provider)
  const authorize = provider.createAuthorizationURL.bind(provider)
  const exchange = provider.validateAuthorizationCode.bind(provider)
  provider.createAuthorizationURL = async (data) =>
    authorize({ ...data, codeVerifier: (await flowUsesPkce()) ? data.codeVerifier : (undefined as never) })
  provider.validateAuthorizationCode = async (data) =>
    exchange({ ...data, codeVerifier: (await flowUsesPkce()) ? data.codeVerifier : (undefined as never) })
}

function requestedProviderId(ctx: {
  body?: Record<string, unknown>
  query?: Record<string, unknown>
  params?: Record<string, unknown>
}): string | undefined {
  const id = ctx.body?.provider ?? ctx.body?.providerId ?? ctx.query?.provider ?? ctx.params?.id
  return typeof id === `string` ? id : undefined
}

/**
 * Must come AFTER `genericOAuth` in the plugin list: its before hook re-adds a
 * provider whose boot discovery failed, and this one wraps it afterwards.
 * The generic providers must keep `pkce` unset (the plugin then passes the
 * verifier this wrapper hands it, or none).
 */
export function oidcPkcePlugin(
  providers: OidcPkceProvider[],
  cache: ReturnType<typeof createS256Cache> = createS256Cache()
): BetterAuthPlugin {
  const byId = new Map(providers.map((p) => [p.id, p]))
  return {
    id: `oidc-pkce`,
    init: () => {
      // Warm the cache without holding boot on any IdP.
      for (const p of providers) void cache.resolve(p)
    },
    hooks: {
      before: [
        {
          matcher: (context) => byId.has(requestedProviderId(context as never) ?? ``),
          handler: createAuthMiddleware(async (ctx) => {
            for (const provider of ctx.context.socialProviders) {
              if (byId.has(provider.id)) wrapProvider(provider as unknown as WrappableProvider)
            }
            // Callbacks read the decision their sign-in sealed; only a flow's
            // start decides.
            if (ctx.path.includes(`/callback`)) return
            const p = byId.get(requestedProviderId(ctx as never)!)!
            await addOAuthServerContext({ [OIDC_PKCE_STATE_KEY]: await cache.resolve(p) })
          }),
        },
      ],
    },
  }
}
