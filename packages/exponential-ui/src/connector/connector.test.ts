// VAPP-91: the connector against a fake Exponential instance (discovery,
// registration, PKCE, the token exchange, MCP tool calls, polled `exp:`
// sources) and a declarative vapp run on it.

import { describe, expect, test } from "bun:test"
import { ExponentialConnector, createVappHost, pkcePair } from "."
import { CORE_CATALOG_ID } from "../catalog"
import type { VappPackage } from "../host"

const BASE = `https://exp.example`

function fakeInstance() {
  const calls: { url: string; body?: string; auth?: string | null }[] = []
  let issues = [{ id: `i1`, identifier: `ACME-1`, title: `First` }]
  const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input)
    calls.push({ url, body: init?.body ? String(init.body) : undefined, auth: new Headers(init?.headers).get(`authorization`) })
    const json = (v: unknown) => new Response(JSON.stringify(v), { status: 200, headers: { "content-type": `application/json` } })
    if (url.endsWith(`/.well-known/oauth-authorization-server`))
      return json({ issuer: BASE, authorization_endpoint: `${BASE}/api/auth/mcp/authorize`, token_endpoint: `${BASE}/api/auth/mcp/token`, registration_endpoint: `${BASE}/api/auth/mcp/register` })
    if (url.endsWith(`/register`)) return json({ client_id: `client-1` })
    if (url.endsWith(`/token`)) return json({ access_token: `at-1`, refresh_token: `rt-1`, expires_in: 3600 })
    if (url.endsWith(`/api/mcp`)) {
      const rpc = JSON.parse(String(init?.body))
      const payload = rpc.params.name === `exponential_issues_list` ? issues : []
      return new Response(`event: message\ndata: ${JSON.stringify({ jsonrpc: `2.0`, id: rpc.id, result: { content: [{ type: `text`, text: JSON.stringify(payload) }] } })}\n\n`, { headers: { "content-type": `text/event-stream` } })
    }
    return new Response(`nope`, { status: 404 })
  }) as typeof fetch
  return { calls, fetchFn, setIssues: (next: typeof issues) => (issues = next) }
}

describe(`ExponentialConnector`, () => {
  test(`PKCE: a 43-char verifier and its S256 challenge`, async () => {
    const { verifier, challenge } = await pkcePair()
    expect(verifier).toMatch(/^[A-Za-z0-9_-]{43}$/)
    expect(challenge).toMatch(/^[A-Za-z0-9_-]{43}$/)
    expect(challenge).not.toBe(verifier)
  })

  test(`discovery → registration → authorize url → exchange → an MCP call as the person`, async () => {
    const instance = fakeInstance()
    const saved: unknown[] = []
    const c = new ExponentialConnector({ baseUrl: BASE, redirectUri: `https://host.example/cb`, fetch: instance.fetchFn, onTokens: (t) => saved.push(t) })
    const { url, verifier, state } = await c.authorizeUrl()
    const u = new URL(url)
    expect(u.origin + u.pathname).toBe(`${BASE}/api/auth/mcp/authorize`)
    expect(u.searchParams.get(`client_id`)).toBe(`client-1`)
    expect(u.searchParams.get(`code_challenge_method`)).toBe(`S256`)
    expect(u.searchParams.get(`state`)).toBe(state)
    await c.exchange(`code-1`, verifier)
    expect(c.tokens?.accessToken).toBe(`at-1`)
    expect(saved).toHaveLength(1)
    const tokenCall = instance.calls.find((x) => x.url.endsWith(`/token`))!
    expect(new URLSearchParams(tokenCall.body).get(`code_verifier`)).toBe(verifier)
    expect(await c.callTool(`exponential_issues_list`, { limit: 5 })).toEqual([{ id: `i1`, identifier: `ACME-1`, title: `First` }])
    expect(instance.calls.at(-1)!.auth).toBe(`Bearer at-1`)
  })

  test(`a declarative vapp runs on the connector's exp: sources`, async () => {
    const instance = fakeInstance()
    const connector = new ExponentialConnector({ baseUrl: BASE, redirectUri: `x`, fetch: instance.fetchFn, tokens: { accessToken: `at-1` }, pollSeconds: 0 })
    const pkg: VappPackage = {
      id: `acme.triage`,
      name: `Triage`,
      version: `1.0.0`,
      catalogId: CORE_CATALOG_ID,
      functions: [`exponential.mcp`],
      templates: {
        board: {
          components: [
            { id: `root`, component: `List`, children: { componentId: `row`, path: `/issues/rows` } },
            { id: `row`, component: `Text`, text: { path: `title` } },
          ],
          bindings: [{ path: `/issues`, source: `exp:issues?board=b1&limit=10` }],
        },
      },
    }
    const vapp = createVappHost({ package: pkg, sources: connector.sources(), functions: connector.functions() })
    expect(vapp.issues).toEqual([])
    vapp.open(`triage`)
    await new Promise((r) => setTimeout(r, 10))
    expect(vapp.host.surface(`triage`)!.data).toEqual({ issues: { rows: [{ id: `i1`, identifier: `ACME-1`, title: `First` }], count: 1 } })
    const rpc = JSON.parse(instance.calls.find((x) => x.url.endsWith(`/api/mcp`))!.body!)
    expect(rpc.params).toEqual({ name: `exponential_issues_list`, arguments: { limit: 10, boardId: `b1` } })
    // the package lists exponential.mcp, nothing else
    expect(vapp.host.decide(`triage`, `exponential.mcp`)).toBe(`allow`)
  })
})
