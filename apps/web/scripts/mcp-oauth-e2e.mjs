#!/usr/bin/env node
// End-to-end MCP OAuth against a RUNNING web server, as an MCP client does it:
// the official MCP SDK's client auth (what Claude Code builds on) for
// discovery, dynamic client registration, PKCE, token exchange and refresh,
// and a real browser (Playwright) for the human part: the login page and the
// /auth/consent scope picker. The client's whole state (registration,
// tokens, cached discovery) persists in one JSON file, so a client connected
// to one build can be resumed against the next (better-auth 1.6 → 1.7: the
// cached 1.6 token endpoint must keep refreshing).
//
//   node scripts/mcp-oauth-e2e.mjs <step> --base <url> --state <file> [--db <url>] [--bearer <token>]
//
//   --bearer   an instance without password login (staging): the browser
//              carries this session token as `Authorization: Bearer` to the
//              instance (never to other origins), so no login page is shown
//
//   setup    sign a test user up (password) and give them a team (needs --db)
//   connect  fresh client: register, log in, consent, list tools
//   reauth   the SAVED registration authorizes again (tokens dropped): log in,
//            consent, list tools
//   reuse    saved tokens only (no browser allowed): list tools
//   refresh  refresh with the saved refresh token (cached discovery), list tools
//   revoke   revoke the client the way Settings does (tRPC), then expect a 401
//            and a client that has to re-authorize
//
// Run with node (Playwright's request client fails under Bun).
import { readFileSync, writeFileSync, existsSync } from "node:fs"
import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js"
import { auth, UnauthorizedError } from "@modelcontextprotocol/sdk/client/auth.js"
import { chromium } from "@playwright/test"

const args = process.argv.slice(2)
const step = args[0]
const opt = (name) => {
  const i = args.indexOf(`--${name}`)
  return i >= 0 ? args[i + 1] : undefined
}
const base = (opt(`base`) ?? ``).replace(/\/$/, ``)
const statePath = opt(`state`)
if (!step || !base || !statePath) {
  console.error(`usage: mcp-oauth-e2e.mjs <setup|connect|reuse|refresh|revoke> --base <url> --state <file> [--db <url>]`)
  process.exit(2)
}
const bearer = opt(`bearer`)
const REDIRECT = `http://127.0.0.1:53682/callback`
const mcpUrl = `${base}/api/mcp`

const load = () => (existsSync(statePath) ? JSON.parse(readFileSync(statePath, `utf8`)) : {})
const save = (s) => writeFileSync(statePath, JSON.stringify(s, null, 2))
const log = (...a) => console.log(`[e2e:${step}]`, ...a)
const fail = (msg) => {
  console.error(`[e2e:${step}] FAIL: ${msg}`)
  process.exit(1)
}

/** The SDK's OAuthClientProvider over the state file. `browser` = what to do
 *  when the SDK sends the user to authorize (null: not allowed). */
function provider(state, browser) {
  return {
    get redirectUrl() {
      return REDIRECT
    },
    get clientMetadata() {
      return {
        client_name: `E2E MCP client`,
        redirect_uris: [REDIRECT],
        grant_types: [`authorization_code`, `refresh_token`],
        response_types: [`code`],
        token_endpoint_auth_method: `none`,
      }
    },
    // Like Claude Code: a `state` on every authorization request.
    state: () => (state.oauthState = Math.random().toString(36).slice(2)),
    clientInformation: () => state.client,
    saveClientInformation: (c) => {
      state.client = c
      save(state)
    },
    tokens: () => state.tokens,
    saveTokens: (t) => {
      state.tokens = t
      save(state)
    },
    codeVerifier: () => state.verifier,
    saveCodeVerifier: (v) => {
      state.verifier = v
      save(state)
    },
    discoveryState: () => state.discovery,
    saveDiscoveryState: (d) => {
      state.discovery = d
      save(state)
    },
    invalidateCredentials: (scope) => {
      if (scope === `all` || scope === `tokens`) delete state.tokens
      if (scope === `all` || scope === `client`) delete state.client
      if (scope === `all` || scope === `discovery`) delete state.discovery
      save(state)
    },
    redirectToAuthorization: async (url) => {
      state.authorizeCalls = (state.authorizeCalls ?? 0) + 1
      save(state)
      if (!browser) throw new Error(`the client was sent to authorize: ${url}`)
      state.pendingCode = await browser(url)
      save(state)
    },
  }
}

/** With --bearer: the instance's requests (and only those) carry the token. */
async function withBearer(context) {
  if (!bearer) return
  const origin = new URL(base).origin
  await context.route(`${origin}/**`, (route) =>
    route.continue({ headers: { ...route.request().headers(), authorization: `Bearer ${bearer}` } })
  )
}

/** Log in and consent in a real browser; returns the authorization code. */
async function browserConsent(state, url) {
  const browserApp = await chromium.launch()
  try {
    const context = await browserApp.newContext({ ignoreHTTPSErrors: true })
    await withBearer(context)
    const page = await context.newPage()
    let code = null
    await page.route(`${REDIRECT}**`, (route) => {
      const u = new URL(route.request().url())
      code = u.searchParams.get(`code`)
      state.callbackIss = u.searchParams.get(`iss`)
      if (u.searchParams.get(`state`) !== state.oauthState) fail(`the callback's state is not the request's`)
      return route.fulfill({ status: 200, body: `ok` })
    })
    await page.goto(url.toString())
    if (!bearer) {
      await page.waitForURL(/\/auth\/login/, { timeout: 30_000 })
      log(`login page: ${new URL(page.url()).pathname}`)
      await page.getByRole(`button`, { name: `Continue with email` }).click()
      await page.locator(`#email`).fill(state.user.email)
      const password = page.locator(`#password`)
      if (!(await password.isVisible().catch(() => false))) {
        await page.getByRole(`button`, { name: /password/i }).first().click()
      }
      await password.fill(state.user.password)
      await page.locator(`button[type="submit"]`).first().click()
    }
    await page.waitForURL(/\/auth\/consent/, { timeout: 30_000 })
    const consentUrl = new URL(page.url())
    log(`consent page: ${consentUrl.pathname} (params: ${[...consentUrl.searchParams.keys()].join(`,`)})`)
    // The default selection is "Everything"; Allow enables once the client
    // and the scope tree have loaded.
    const allow = page.getByRole(`button`, { name: `Allow access` })
    await page.waitForFunction(
      () => [...document.querySelectorAll(`button`)].some((b) => b.textContent?.trim() === `Allow access` && !b.disabled),
      null,
      { timeout: 30_000 }
    )
    if (process.env.E2E_SHOTS) await page.screenshot({ path: `${process.env.E2E_SHOTS}/consent-${base.replace(/\W+/g, `-`)}.png` })
    await allow.click()
    for (let i = 0; i < 100 && !code; i++) await page.waitForTimeout(100)
    if (!code) {
      if (process.env.E2E_SHOTS) await page.screenshot({ path: `${process.env.E2E_SHOTS}/no-code.png` })
      fail(`no authorization code reached the client's callback (the browser is on ${page.url()})`)
    }
    // Keep the browser session's cookies for the revoke step (Settings).
    state.cookies = await context.cookies()
    save(state)
    return code
  } finally {
    await browserApp.close()
  }
}

async function listTools(state, browser = null) {
  const transport = new StreamableHTTPClientTransport(new URL(mcpUrl), { authProvider: provider(state, browser) })
  const client = new Client({ name: `e2e`, version: `1.0.0` })
  await client.connect(transport)
  const { tools } = await client.listTools()
  await client.close()
  return tools
}

async function setup() {
  const db = opt(`db`)
  if (!db) fail(`setup needs --db`)
  const id = Math.random().toString(36).slice(2, 8)
  const user = { email: `e2e-${id}@example.com`, password: `e2e-password-${id}!`, name: `E2E ${id}` }
  const res = await fetch(`${base}/api/auth/sign-up/email`, {
    method: `POST`,
    headers: { "content-type": `application/json`, origin: base },
    body: JSON.stringify(user),
  })
  if (!res.ok) fail(`sign-up: ${res.status} ${await res.text()}`)
  const { user: created } = await res.json()
  const { default: pg } = await import(`pg`)
  const pool = new pg.Pool({ connectionString: db })
  const team = await pool.query(`INSERT INTO teams (name, slug) VALUES ($1, $2) RETURNING id`, [`E2E ${id}`, `e2e-${id}`])
  await pool.query(`INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'owner')`, [team.rows[0].id, created.id])
  await pool.end()
  save({ user: { ...user, id: created.id }, teamId: team.rows[0].id })
  log(`user ${user.email}, team ${team.rows[0].id}`)
}

async function reauth() {
  const prior = load()
  if (!prior.client) fail(`no saved registration to re-authorize`)
  const state = { user: prior.user, teamId: prior.teamId, client: prior.client, discovery: prior.discovery }
  save(state)
  await authorize(state)
}

async function connect() {
  const prior = load()
  const state = { user: prior.user, teamId: prior.teamId }
  save(state)
  await authorize(state)
}

async function authorize(state) {
  const browser = (url) => browserConsent(state, url)
  try {
    await listTools(state, browser)
    fail(`an unauthenticated client listed tools`)
  } catch (e) {
    if (!(e instanceof UnauthorizedError)) throw e
  }
  const result = await auth(provider(state, null), { serverUrl: mcpUrl, authorizationCode: state.pendingCode })
  if (result !== `AUTHORIZED`) fail(`token exchange: ${result}`)
  const tools = await listTools(state)
  log(`registered ${state.client.client_id}; issuer in callback: ${state.callbackIss ?? `(none)`}`)
  log(`token endpoint (cached): ${state.discovery?.authorizationServerMetadata?.token_endpoint}`)
  log(`tools/list: ${tools.length} tools; refresh token: ${state.tokens.refresh_token ? `yes` : `no`}`)
  if (tools.length === 0) fail(`no tools`)
}

async function reuse() {
  const state = load()
  const authorizeCalls = state.authorizeCalls ?? 0
  const tools = await listTools(state)
  log(`tools/list with the saved access token: ${tools.length} tools`)
  if ((state.authorizeCalls ?? 0) !== authorizeCalls) fail(`the client had to re-authorize`)
}

async function refresh() {
  const state = load()
  const before = state.tokens
  log(`refreshing at the cached token endpoint: ${state.discovery?.authorizationServerMetadata?.token_endpoint}`)
  const result = await auth(provider(state, null), { serverUrl: mcpUrl })
  if (result !== `AUTHORIZED`) fail(`refresh: ${result}`)
  if (state.tokens.access_token === before.access_token) fail(`refresh returned the same access token`)
  const tools = await listTools(state)
  log(`refreshed (new access + refresh token: ${state.tokens.refresh_token !== before.refresh_token}); tools/list: ${tools.length} tools`)
}

async function revoke() {
  const state = load()
  const browserApp = await chromium.launch()
  try {
    const context = await browserApp.newContext()
    if (state.cookies?.length) await context.addCookies(state.cookies)
    const res = await context.request.post(`${base}/api/trpc/mcpClients.revoke`, {
      headers: { "content-type": `application/json`, origin: base, ...(bearer ? { authorization: `Bearer ${bearer}` } : {}) },
      data: { clientId: state.client.client_id, teamId: state.teamId },
    })
    log(`Settings revoke: ${res.status()}`)
    if (!res.ok()) fail(`revoke: ${res.status()} ${await res.text()}`)
  } finally {
    await browserApp.close()
  }
  const raw = await fetch(mcpUrl, {
    method: `POST`,
    headers: {
      authorization: `Bearer ${state.tokens.access_token}`,
      "content-type": `application/json`,
      accept: `application/json, text/event-stream`,
    },
    body: JSON.stringify({ jsonrpc: `2.0`, id: 1, method: `tools/list` }),
  })
  log(`revoked access token on /api/mcp: ${raw.status}`)
  if (raw.status !== 401) fail(`a revoked token answered ${raw.status}`)
  const refreshed = await fetch(state.discovery.authorizationServerMetadata.token_endpoint, {
    method: `POST`,
    headers: { "content-type": `application/x-www-form-urlencoded` },
    body: new URLSearchParams({ grant_type: `refresh_token`, refresh_token: state.tokens.refresh_token, client_id: state.client.client_id }),
  })
  log(`revoked refresh token at the token endpoint: ${refreshed.status}`)
  if (refreshed.ok) fail(`a revoked refresh token still refreshed`)
}

const steps = { setup, connect, reauth, reuse, refresh, revoke }
if (!steps[step]) fail(`unknown step ${step}`)
await steps[step]()
log(`ok`)
