// @vitest-environment node
// better-auth 1.7: the sign-in surface against the REAL migrated Postgres and
// the PRODUCTION auth config (lib/auth/index.ts), driven over HTTP through the
// /api/auth route (its 1.6 aliases and the MCP authorize guard included):
// email + password sign-in and the bearer token, session refresh, a passkey
// registered and used by a software authenticator, MCP OAuth end to end
// (dynamic registration, login bounce, consent, code, refresh, revocation),
// the 1.6 rows migration 0267 carries over (hashed tokens, sealed secrets),
// and generic OIDC sign-in on the web and through the native handoff against
// a mock identity provider that only knows the 1.6 callback URI.
import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { createServer, type Server } from "node:http"
import { createHash, generateKeyPairSync, randomBytes, sign as signData, type KeyObject } from "node:crypto"
import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { SignJWT, exportJWK, generateKeyPair } from "jose"
import { probeRealDb } from "@/test/real-db"

const BASE = `http://localhost:3000`
const REDIRECT = `http://127.0.0.1:53682/callback`
const id8 = () => crypto.randomUUID().slice(0, 8)
const b64url = (b: Buffer | Uint8Array | string) => Buffer.from(b).toString(`base64url`)
const sha256 = (b: Buffer | string) => createHash(`sha256`).update(b).digest()

// ── The mock OIDC provider (started before the auth config reads its env) ──
const idp = await (async () => {
  const { privateKey, publicKey } = await generateKeyPair(`RS256`)
  const jwk = { ...(await exportJWK(publicKey)), kid: `k1`, alg: `RS256`, use: `sig` }
  const seen = { redirectUris: [] as string[], nonce: null as string | null }
  const profile = { sub: `idp-${id8()}`, email: `oidc-${id8()}@idp.test`, name: `Ida Provider` }
  const server: Server = createServer(async (req, res) => {
    const url = new URL(req.url ?? `/`, `http://127.0.0.1`)
    const json = (body: unknown, status = 200) => {
      res.writeHead(status, { "content-type": `application/json` })
      res.end(JSON.stringify(body))
    }
    if (url.pathname === `/.well-known/openid-configuration`) {
      return json({
        issuer: origin,
        authorization_endpoint: `${origin}/authorize`,
        token_endpoint: `${origin}/token`,
        userinfo_endpoint: `${origin}/userinfo`,
        jwks_uri: `${origin}/jwks`,
        response_types_supported: [`code`],
        subject_types_supported: [`public`],
        id_token_signing_alg_values_supported: [`RS256`],
      })
    }
    if (url.pathname === `/jwks`) return json({ keys: [jwk] })
    if (url.pathname === `/userinfo`) return json({ ...profile, email_verified: true })
    if (url.pathname === `/token`) {
      let body = ``
      for await (const chunk of req) body += chunk
      const form = new URLSearchParams(body)
      seen.redirectUris.push(form.get(`redirect_uri`) ?? ``)
      // A real IdP refuses any callback but the registered (1.6) one.
      if (form.get(`redirect_uri`) !== `${BASE}/api/auth/oauth2/callback/mockidp`) {
        return json({ error: `invalid_grant`, error_description: `redirect_uri mismatch` }, 400)
      }
      // A real IdP echoes the authorize request's nonce (the test hands it over).
      const idToken = await new SignJWT({ email: profile.email, email_verified: true, name: profile.name, ...(seen.nonce ? { nonce: seen.nonce } : {}) })
        .setProtectedHeader({ alg: `RS256`, kid: `k1` })
        .setIssuer(origin)
        .setSubject(profile.sub)
        .setAudience(`mock-client`)
        .setIssuedAt()
        .setExpirationTime(`5m`)
        .sign(privateKey)
      return json({ access_token: `idp-at`, token_type: `Bearer`, expires_in: 300, id_token: idToken })
    }
    res.writeHead(404).end()
  })
  await new Promise<void>((resolve) => server.listen(0, `127.0.0.1`, resolve))
  const origin = `http://127.0.0.1:${(server.address() as { port: number }).port}`
  return { server, origin, seen, profile }
})()

process.env.BETTER_AUTH_URL = BASE
process.env.BETTER_AUTH_SECRET ||= `real-db-auth-flows-secret-0123456789abcdef`
process.env.AUTH_RATE_LIMIT_ENABLED = `false`
process.env.AUTH_SIGNUP_ENABLED = `true`
process.env.OIDC_PROVIDERS = JSON.stringify([
  { id: `mockidp`, name: `Mock IdP`, clientId: `mock-client`, clientSecret: `mock-secret`, discoveryUrl: `${idp.origin}/.well-known/openid-configuration` },
])

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'oauth2_clients') AS ok
`)
if (!ready.ok) console.info(`[auth-flows.real-db] skipped: ${ready.reason}`)

const { db } = await import(`@/db/connection`)
const { sql } = await import(`drizzle-orm`)
const { auth } = await import(`@/lib/auth`)
const { Route: AuthRoute } = await import(`@/routes/api/auth/$`)
const { Route: MobileStart } = await import(`@/routes/api/mobile-oauth-start`)
const { Route: MobileReturn } = await import(`@/routes/api/mobile-oauth-return`)
const { mcpGrantsRouter } = await import(`@/lib/trpc/mcp-grants`)
const { resolveMcpTokenAccess } = await import(`@/lib/emcp/scope`)
const { revokeClient } = await import(`@/lib/emcp/revoke`)
const { oauthResumeUrlFor } = await import(`./oauth-resume`)
const { hashOAuthToken } = await import(`./oauth-token-hash`)
const { fillLegacyMcpClientSecrets, sealerFor } = await import(`./legacy-mcp-oauth`)

type Handler = (ctx: { request: Request }) => Promise<Response>
const handlers = (route: unknown) => (route as { options: { server: { handlers: { GET: Handler; POST?: Handler } } } }).options.server.handlers

/** A browser's cookie store, enough for Better Auth's cookies. */
class Jar {
  private readonly cookies = new Map<string, string>()
  take(res: Response): Response {
    for (const line of res.headers.getSetCookie()) {
      const [pair] = line.split(`;`)
      const at = pair!.indexOf(`=`)
      const name = pair!.slice(0, at).trim()
      const value = pair!.slice(at + 1)
      if (!value || /max-age=0/i.test(line)) this.cookies.delete(name)
      else this.cookies.set(name, value)
    }
    return res
  }
  drop(name: string) {
    this.cookies.delete(name)
  }
  get header() {
    return [...this.cookies].map(([k, v]) => `${k}=${v}`).join(`; `)
  }
}

async function call(
  method: `GET` | `POST`,
  path: string,
  opts: { jar?: Jar; json?: unknown; form?: Record<string, string>; headers?: Record<string, string>; route?: unknown } = {}
): Promise<Response> {
  const headers = new Headers({ origin: BASE, ...opts.headers })
  if (opts.jar?.header) headers.set(`cookie`, opts.jar.header)
  let body: string | undefined
  if (opts.json !== undefined) {
    headers.set(`content-type`, `application/json`)
    body = JSON.stringify(opts.json)
  } else if (opts.form) {
    headers.set(`content-type`, `application/x-www-form-urlencoded`)
    body = new URLSearchParams(opts.form).toString()
  }
  const request = new Request(new URL(path, BASE), { method, headers, body, redirect: `manual` })
  const h = handlers(opts.route ?? AuthRoute)
  const res = await (method === `GET` ? h.GET : h.POST!)({ request })
  opts.jar?.take(res)
  return res
}

const made = { emails: [] as string[], clients: [] as string[] }

async function signedUp(): Promise<{ jar: Jar; email: string; password: string; userId: string; token: string }> {
  const email = `flows-${id8()}@auth-flows.test`
  const password = `pw-${randomBytes(9).toString(`hex`)}`
  made.emails.push(email)
  const jar = new Jar()
  const res = await call(`POST`, `/api/auth/sign-up/email`, { jar, json: { email, password, name: `Flow Tester` } })
  expect(res.status).toBe(200)
  const body = (await res.json()) as { user: { id: string }; token: string }
  return { jar, email, password, userId: body.user.id, token: body.token }
}

async function sessionOf(jar: Jar): Promise<{ user: { id: string; email: string } } | null> {
  const res = await call(`GET`, `/api/auth/get-session`, { jar })
  return (await res.json()) as { user: { id: string; email: string } } | null
}

afterAll(async () => {
  idp.server.close()
  if (!ready.ok) return
  for (const email of [...made.emails, idp.profile.email]) {
    await db.execute(sql`DELETE FROM users WHERE email = ${email}`)
  }
  for (const clientId of made.clients) {
    await db.execute(sql`DELETE FROM oauth2_clients WHERE client_id = ${clientId}`)
    await db.execute(sql`DELETE FROM oauth_applications WHERE client_id = ${clientId}`)
  }
})

describe.skipIf(!ready.ok)(`sign-in and sessions (better-auth 1.7, production config)`, () => {
  it(`signs up, signs in with the password, refuses a wrong one, and honours the bearer token`, async () => {
    const { email, password, userId } = await signedUp()
    const jar = new Jar()
    const ok = await call(`POST`, `/api/auth/sign-in/email`, { jar, json: { email, password } })
    expect(ok.status).toBe(200)
    expect((await sessionOf(jar))?.user.id).toBe(userId)
    const bearer = ok.headers.get(`set-auth-token`)
    expect(bearer).toBeTruthy()
    const viaBearer = await call(`GET`, `/api/auth/get-session`, { headers: { authorization: `Bearer ${bearer}` } })
    expect(((await viaBearer.json()) as { user: { id: string } }).user.id).toBe(userId)
    const wrong = await call(`POST`, `/api/auth/sign-in/email`, { json: { email, password: `${password}x` } })
    expect(wrong.status).toBe(401)
  })

  it(`refreshes a session older than updateAge and ends an expired one`, async () => {
    const { jar, userId } = await signedUp()
    // Past the 1-day updateAge, one day left: the next read slides it to 60 days.
    await db.execute(sql`
      UPDATE auth_sessions SET expires_at = now() + interval '1 day', updated_at = now() - interval '2 days'
      WHERE user_id = ${userId}
    `)
    jar.drop(`better-auth.session_data`) // the 5-minute cookie cache would answer instead
    expect((await sessionOf(jar))?.user.id).toBe(userId)
    const { rows } = await db.execute<{ days: number }>(sql`
      SELECT extract(epoch FROM expires_at - now()) / 86400 AS days FROM auth_sessions WHERE user_id = ${userId}
    `)
    expect(Number(rows[0]!.days)).toBeGreaterThan(59)
    await db.execute(sql`UPDATE auth_sessions SET expires_at = now() - interval '1 minute' WHERE user_id = ${userId}`)
    jar.drop(`better-auth.session_data`)
    expect(await sessionOf(jar)).toBeNull()
  })

  it(`registers a passkey and signs in with it`, async () => {
    const { jar, email } = await signedUp()
    const authenticator = softwareAuthenticator()
    const optionsRes = await call(`GET`, `/api/auth/passkey/generate-register-options`, { jar })
    expect(optionsRes.status).toBe(200)
    const options = (await optionsRes.json()) as { challenge: string; rp: { id: string }; user: { id: string } }
    expect(options.rp.id).toBe(`localhost`)
    const registered = await call(`POST`, `/api/auth/passkey/verify-registration`, {
      jar,
      json: { response: authenticator.register(options.challenge), name: `Test key` },
    })
    expect(registered.status).toBe(200)

    const fresh = new Jar()
    const authOptionsRes = await call(`GET`, `/api/auth/passkey/generate-authenticate-options`, { jar: fresh })
    expect(authOptionsRes.status).toBe(200)
    const authOptions = (await authOptionsRes.json()) as { challenge: string }
    const signedIn = await call(`POST`, `/api/auth/passkey/verify-authentication`, {
      jar: fresh,
      json: { response: authenticator.assert(authOptions.challenge, options.user.id) },
    })
    expect(signedIn.status).toBe(200)
    expect((await sessionOf(fresh))?.user.email).toBe(email)
  })
})

describe.skipIf(!ready.ok)(`MCP OAuth (oauth-provider, the 1.6 paths aliased)`, () => {
  const pkce = () => {
    const verifier = b64url(randomBytes(32))
    return { verifier, challenge: b64url(sha256(verifier)) }
  }

  async function register(): Promise<string> {
    const res = await call(`POST`, `/api/auth/mcp/register`, {
      json: {
        client_name: `Claude Code`,
        redirect_uris: [REDIRECT],
        grant_types: [`authorization_code`, `refresh_token`],
        response_types: [`code`],
        token_endpoint_auth_method: `none`,
      },
    })
    expect(res.status).toBeLessThan(300)
    const { client_id } = (await res.json()) as { client_id: string }
    made.clients.push(client_id)
    return client_id
  }

  /** Authorize → login bounce → resume → consent; returns the consent URL. */
  async function toConsent(jar: Jar, clientId: string, challenge: string, resource = `${BASE}/api/mcp`): Promise<URL> {
    const query = new URLSearchParams({
      response_type: `code`,
      client_id: clientId,
      redirect_uri: REDIRECT,
      state: `st-1`,
      code_challenge: challenge,
      code_challenge_method: `S256`,
      scope: `openid profile email offline_access`,
      resource,
    })
    const bounce = await call(`GET`, `/api/auth/mcp/authorize?${query}`)
    expect(bounce.status).toBe(302)
    const login = new URL(bounce.headers.get(`location`)!, BASE)
    expect(login.pathname).toBe(`/auth/login`)
    expect(login.searchParams.get(`sig`)).toBeTruthy()
    const resume = oauthResumeUrlFor(login.search)!
    const consent = await call(`GET`, resume, { jar })
    expect(consent.status).toBe(302)
    const consentUrl = new URL(consent.headers.get(`location`)!, BASE)
    expect(consentUrl.pathname).toBe(`/auth/consent`)
    return consentUrl
  }

  async function consentAs(jar: Jar, consentUrl: URL, clientId: string): Promise<URL> {
    const headers = new Headers({ cookie: jar.header, origin: BASE })
    const session = await auth.api.getSession({ headers })
    const caller = mcpGrantsRouter.createCaller({ session, db, request: new Request(BASE, { headers }) } as never)
    const { redirectURI } = await caller.grantAndConsent({ clientId, oauthQuery: consentUrl.search, accept: true, allTeams: true })
    return new URL(redirectURI)
  }

  const token = (form: Record<string, string>, path = `/api/auth/mcp/token`, headers?: Record<string, string>) =>
    call(`POST`, path, { form, headers })

  it(`registers, consents, exchanges, refreshes and revokes`, async () => {
    const { jar, userId } = await signedUp()
    const clientId = await register()
    const { verifier, challenge } = pkce()
    const consentUrl = await toConsent(jar, clientId, challenge)
    const callback = await consentAs(jar, consentUrl, clientId)
    expect(callback.origin + callback.pathname).toBe(REDIRECT)
    expect(callback.searchParams.get(`state`)).toBe(`st-1`)

    const exchanged = await token({
      grant_type: `authorization_code`,
      code: callback.searchParams.get(`code`)!,
      redirect_uri: REDIRECT,
      client_id: clientId,
      code_verifier: verifier,
    })
    expect(exchanged.status).toBe(200)
    const first = (await exchanged.json()) as { access_token: string; refresh_token: string; expires_in: number }
    expect(first.refresh_token).toBeTruthy()
    expect(first.expires_in).toBe(60 * 60 * 24)
    // Opaque, and stored only as its hash.
    expect(first.access_token.split(`.`)).toHaveLength(1)
    const stored = await db.execute<{ n: number }>(sql`SELECT count(*)::int AS n FROM oauth2_access_tokens WHERE token = ${first.access_token}`)
    expect(stored.rows[0]!.n).toBe(0)

    const access = await resolveMcpTokenAccess(first.access_token)
    expect(access).toMatchObject({ userId, clientId })
    expect(access!.access.full).toBe(true)

    const refreshed = await token(
      { grant_type: `refresh_token`, refresh_token: first.refresh_token, client_id: clientId },
      `/api/auth/oauth2/token`
    )
    expect(refreshed.status).toBe(200)
    const second = (await refreshed.json()) as { access_token: string; refresh_token: string }
    expect(second.access_token).not.toBe(first.access_token)
    expect(await resolveMcpTokenAccess(second.access_token)).toMatchObject({ userId, clientId })

    expect(await revokeClient(db, userId, clientId)).toBeGreaterThan(0)
    expect(await resolveMcpTokenAccess(second.access_token)).toBeNull()
    const dead = await token({ grant_type: `refresh_token`, refresh_token: second.refresh_token, client_id: clientId })
    expect(dead.ok).toBe(false)
  })

  it(`keeps the apps resource on the consent page's signed query`, async () => {
    const { jar } = await signedUp()
    const clientId = await register()
    const consentUrl = await toConsent(jar, clientId, pkce().challenge, `${BASE}/api/mcp/apps`)
    expect(consentUrl.searchParams.get(`resource`)).toBe(`${BASE}/api/mcp/apps`)
  })

  it(`refuses an unknown client before the provider sees it`, async () => {
    const res = await call(`GET`, `/api/auth/oauth2/authorize?client_id=nope&response_type=code&redirect_uri=${encodeURIComponent(REDIRECT)}`)
    expect(res.status).toBe(400)
    expect(await res.text()).toContain(`isn't registered here`)
  })

  it(`carries the 1.6 rows over: hashed tokens keep working, a sealed secret refreshes`, async () => {
    const { userId } = await signedUp()
    const pub = `legacy-pub-${id8()}`
    const conf = `legacy-conf-${id8()}`
    made.clients.push(pub, conf)
    const at = `legacy-at-${randomBytes(12).toString(`hex`)}`
    const rt = `legacy-rt-${randomBytes(12).toString(`hex`)}`
    const rt2 = `legacy-rt2-${randomBytes(12).toString(`hex`)}`
    await db.execute(sql`
      INSERT INTO oauth_applications (id, name, client_id, client_secret, redirect_urls, type, disabled, user_id, created_at, updated_at) VALUES
        (${pub}, 'Claude Code', ${pub}, '', ${REDIRECT}, 'public', false, NULL, now(), now()),
        (${conf}, 'Server agent', ${conf}, 'conf-secret', ${REDIRECT}, 'web', false, NULL, now(), now())
    `)
    await db.execute(sql`
      INSERT INTO oauth_access_tokens (id, access_token, refresh_token, access_token_expires_at, refresh_token_expires_at, client_id, user_id, scopes, created_at, updated_at) VALUES
        (${`lat-${id8()}`}, ${at}, ${rt}, now() + interval '1 hour', now() + interval '30 days', ${pub}, ${userId}, 'openid profile email offline_access', now(), now()),
        (${`lat-${id8()}`}, ${`legacy-at2-${id8()}`}, ${rt2}, now() + interval '1 hour', now() + interval '30 days', ${conf}, ${userId}, 'openid offline_access', now(), now())
    `)
    await db.execute(sql`
      INSERT INTO oauth_consents (id, client_id, user_id, scopes, consent_given, created_at, updated_at)
      VALUES (${`lc-${id8()}`}, ${pub}, ${userId}, 'openid profile email offline_access', true, now(), now())
    `)

    // Migration 0267's copy, verbatim (idempotent: ON CONFLICT DO NOTHING).
    for (const statement of legacyCopyStatements()) await db.execute(sql.raw(statement))
    await db.execute(sql`
      INSERT INTO mcp_grants (user_id, client_id, kind, all_teams) VALUES (${userId}, ${pub}, 'builtin', true), (${userId}, ${conf}, 'builtin', true)
    `)

    const { rows } = await db.execute<{ token: string }>(sql`SELECT token FROM oauth2_refresh_tokens WHERE client_id = ${pub}`)
    expect(rows[0]!.token).toBe(hashOAuthToken(rt))
    expect(await resolveMcpTokenAccess(at)).toMatchObject({ userId, clientId: pub })

    const viaAlias = await token({ grant_type: `refresh_token`, refresh_token: rt, client_id: pub })
    expect(viaAlias.status).toBe(200)
    const renewed = (await viaAlias.json()) as { access_token: string }
    expect(await resolveMcpTokenAccess(renewed.access_token)).toMatchObject({ userId, clientId: pub })

    const ctx = await auth.$context
    const seal = sealerFor(ctx.secretConfig)
    expect(await fillLegacyMcpClientSecrets(db, seal)).toBeGreaterThanOrEqual(1)
    expect(await fillLegacyMcpClientSecrets(db, seal)).toBe(0)
    const basic = (secret: string) => ({ authorization: `Basic ${Buffer.from(`${conf}:${secret}`).toString(`base64`)}` })
    const wrong = await token({ grant_type: `refresh_token`, refresh_token: rt2 }, `/api/auth/mcp/token`, basic(`not-it`))
    expect(wrong.ok).toBe(false)
    const right = await token({ grant_type: `refresh_token`, refresh_token: rt2 }, `/api/auth/mcp/token`, basic(`conf-secret`))
    expect(right.status).toBe(200)
  })
})

describe.skipIf(!ready.ok)(`generic OIDC sign-in (the 1.6 callback URI kept)`, () => {
  it(`signs in on the web through the 1.6 callback path`, async () => {
    const jar = new Jar()
    const start = await call(`POST`, `/api/auth/sign-in/social`, { jar, json: { provider: `mockidp`, callbackURL: `/after` } })
    expect(start.status).toBe(200)
    const { url } = (await start.json()) as { url: string }
    const authorize = new URL(url)
    expect(authorize.origin).toBe(idp.origin)
    expect(authorize.searchParams.get(`redirect_uri`)).toBe(`${BASE}/api/auth/oauth2/callback/mockidp`)
    expect(authorize.searchParams.get(`code_challenge`)).toBeNull()
    idp.seen.nonce = authorize.searchParams.get(`nonce`)
    const back = await call(`GET`, `/api/auth/oauth2/callback/mockidp?code=c1&state=${authorize.searchParams.get(`state`)}`, { jar })
    expect(back.status).toBe(302)
    expect(new URL(back.headers.get(`location`)!, BASE).pathname).toBe(`/after`)
    expect((await sessionOf(jar))?.user.email).toBe(idp.profile.email)
  })

  it(`signs a native in through mobile-oauth-start / -return`, async () => {
    const jar = new Jar()
    const verifier = b64url(randomBytes(32))
    const challenge = b64url(sha256(verifier))
    const start = await call(`GET`, `/api/mobile-oauth-start?providerId=mockidp&code_challenge=${challenge}&code_challenge_method=S256`, {
      jar,
      route: MobileStart,
    })
    expect(start.status).toBe(302)
    const authorize = new URL(start.headers.get(`location`)!)
    expect(authorize.searchParams.get(`redirect_uri`)).toBe(`${BASE}/api/auth/oauth2/callback/mockidp`)
    idp.seen.nonce = authorize.searchParams.get(`nonce`)
    const back = await call(`GET`, `/api/auth/oauth2/callback/mockidp?code=c2&state=${authorize.searchParams.get(`state`)}`, { jar })
    expect(back.status).toBe(302)
    const returnUrl = new URL(back.headers.get(`location`)!, BASE)
    expect(returnUrl.pathname).toBe(`/api/mobile-oauth-return`)
    const handoff = await call(`GET`, `${returnUrl.pathname}${returnUrl.search}`, { jar, route: MobileReturn })
    const location = handoff.headers.get(`location`) ?? (await handoff.text())
    expect(location).toContain(`exponential://oauth-return`)
    expect(location).toContain(`code=`)
    expect(location).not.toContain(`error=`)
  })
})

/** Migration 0267's statements that copy the 1.6 clients, consents and tokens. */
function legacyCopyStatements(): string[] {
  const file = join(dirname(fileURLToPath(import.meta.url)), `..`, `..`, `db`, `out`, `0267_mcp_oauth_provider.sql`)
  return readFileSync(file, `utf8`)
    .split(`--> statement-breakpoint`)
    .map((s) => s.replace(/^\s*--.*$/gm, ``).trim())
    .filter((s) => /^INSERT INTO "oauth2_(clients|consents|refresh_tokens|access_tokens)"/.test(s))
}

// ── A software WebAuthn authenticator (ES256, "none" attestation) ──────────

function cbor(value: unknown): Buffer {
  const head = (major: number, n: number) => {
    if (n < 24) return Buffer.from([(major << 5) | n])
    if (n < 256) return Buffer.from([(major << 5) | 24, n])
    if (n < 65536) return Buffer.from([(major << 5) | 25, n >> 8, n & 255])
    const b = Buffer.alloc(5)
    b[0] = (major << 5) | 26
    b.writeUInt32BE(n, 1)
    return b
  }
  if (typeof value === `number`) return value >= 0 ? head(0, value) : head(1, -1 - value)
  if (typeof value === `string`) return Buffer.concat([head(3, Buffer.byteLength(value)), Buffer.from(value)])
  if (value instanceof Uint8Array) return Buffer.concat([head(2, value.length), Buffer.from(value)])
  if (value instanceof Map) {
    return Buffer.concat([head(5, value.size), ...[...value].flatMap(([k, v]) => [cbor(k), cbor(v)])])
  }
  throw new Error(`cbor: unsupported ${typeof value}`)
}

function softwareAuthenticator() {
  const { privateKey, publicKey } = generateKeyPairSync(`ec`, { namedCurve: `P-256` }) as { privateKey: KeyObject; publicKey: KeyObject }
  const jwk = publicKey.export({ format: `jwk` }) as { x: string; y: string }
  const credentialId = randomBytes(16)
  const rpIdHash = sha256(`localhost`)
  const clientData = (type: string, challenge: string) =>
    Buffer.from(JSON.stringify({ type, challenge, origin: BASE, crossOrigin: false }))
  return {
    register(challenge: string) {
      const coseKey = cbor(new Map<number, unknown>([[1, 2], [3, -7], [-1, 1], [-2, Buffer.from(jwk.x, `base64url`)], [-3, Buffer.from(jwk.y, `base64url`)]]))
      const idLength = Buffer.alloc(2)
      idLength.writeUInt16BE(credentialId.length)
      const authData = Buffer.concat([rpIdHash, Buffer.from([0x45]), Buffer.alloc(4), Buffer.alloc(16), idLength, credentialId, coseKey])
      const attestationObject = cbor(new Map<string, unknown>([[`fmt`, `none`], [`attStmt`, new Map()], [`authData`, authData]]))
      return {
        id: b64url(credentialId),
        rawId: b64url(credentialId),
        type: `public-key`,
        authenticatorAttachment: `platform`,
        clientExtensionResults: {},
        response: {
          clientDataJSON: b64url(clientData(`webauthn.create`, challenge)),
          attestationObject: b64url(attestationObject),
          transports: [`internal`],
        },
      }
    },
    assert(challenge: string, userHandle: string) {
      const counter = Buffer.alloc(4)
      counter.writeUInt32BE(1)
      const authData = Buffer.concat([rpIdHash, Buffer.from([0x05]), counter])
      const clientDataJSON = clientData(`webauthn.get`, challenge)
      const signature = signData(`sha256`, Buffer.concat([authData, sha256(clientDataJSON)]), privateKey)
      return {
        id: b64url(credentialId),
        rawId: b64url(credentialId),
        type: `public-key`,
        authenticatorAttachment: `platform`,
        clientExtensionResults: {},
        response: {
          clientDataJSON: b64url(clientDataJSON),
          authenticatorData: b64url(authData),
          signature: b64url(signature),
          userHandle,
        },
      }
    },
  }
}

beforeAll(() => {
  if (ready.ok) expect(process.env.BETTER_AUTH_URL).toBe(BASE)
})
