// VAPP-91: the host fixtures replayed by the TS reference, plus the runtime
// (ExponentialHost over the in-memory transport).

import { describe, expect, test } from "bun:test"
import transportFixture from "../../fixtures/host-transport.json" with { type: "json" }
import policyFixture from "../../fixtures/host-policy.json" with { type: "json" }
import routerFixture from "../../fixtures/host-router.json" with { type: "json" }
import hostContract from "../../catalog/host.json" with { type: "json" }
import {
  ExponentialHost,
  HostRouter,
  JsonlStreamTransport,
  PackageError,
  SseTransport,
  JsonlDecoder,
  MemoryTransport,
  SseDecoder,
  clientCapabilities,
  combineDecisions,
  decideFunction,
  decideUrl,
  mcpActionCall,
  mediaRequest,
  messagesFromMcpResult,
  parseSource,
  supportedCatalogIds,
  validatePackage,
  BUILTIN_FUNCTIONS,
  HOST_ERROR_CODES,
  FORMATTER_METHODS,
  SURFACE_COMMANDS,
  SURFACE_SETTING_KEYS,
  imageDimensions,
  MEDIA_LIMITS,
} from "."
import type { ClientMessage, Decoded, FunctionDecision, FunctionPolicy, HostIssue, MediaOptions, TransportStatus, UrlPolicy, VappPackage } from "."
import { neutralTheme } from "../themes"
import { CORE_CATALOG_ID } from "../catalog"
import { englishFormatter } from "../format"
import { A11Y_COMMANDS } from "../a11y"

function feed(dec: { push(c: string): Decoded; end(): Decoded }, chunks: string[]): Decoded {
  const out: Decoded = { messages: [], issues: [] }
  for (const c of [...chunks.map((c) => dec.push(c)), dec.end()]) {
    out.messages.push(...c.messages)
    out.issues.push(...c.issues)
  }
  return out
}

describe(`host-transport.json`, () => {
  for (const c of transportFixture.jsonl) test(`jsonl: ${c.name}`, () => expect(feed(new JsonlDecoder(), c.chunks)).toEqual(c.expected as Decoded))
  for (const c of transportFixture.sse) test(`sse: ${c.name}`, () => expect(feed(new SseDecoder(), c.chunks)).toEqual(c.expected as Decoded))
  for (const c of transportFixture.mcp) test(`mcp: ${c.name}`, () => expect(messagesFromMcpResult(c.result)).toEqual(c.expected as Decoded))
  for (const c of transportFixture.mcpAction)
    test(`mcpAction: ${c.name}`, () => expect(mcpActionCall(c.message as ClientMessage, (c as { tool?: string }).tool)).toEqual(c.expected as never))
})

describe(`host-policy.json`, () => {
  for (const c of policyFixture.functions)
    test(`function: ${c.name}`, () => expect(decideFunction((c as { policy?: FunctionPolicy }).policy, c.fn, c.registered)).toBe(c.expected as FunctionDecision))
  test(`combine: the stricter decision wins`, () => {
    for (const c of policyFixture.combine) expect(combineDecisions(c.a as FunctionDecision, c.b as FunctionDecision)).toBe(c.expected as FunctionDecision)
  })
  for (const c of policyFixture.urls) test(`url: ${c.name}`, () => expect(decideUrl((c as { policy?: UrlPolicy }).policy, c.url)).toEqual(c.expected as never))
  for (const c of policyFixture.media) test(`media: ${c.name}`, () => expect(mediaRequest(c.url, c.options as MediaOptions)).toEqual(c.expected as never))
  for (const c of policyFixture.sources) test(`source: ${c.uri}`, () => expect(parseSource(c.uri)).toEqual(c.expected as never))
  for (const c of policyFixture.negotiation)
    test(`negotiation: ${c.extensionIds.length} extension ids`, () => {
      expect(supportedCatalogIds(c.extensionIds)).toEqual(c.expected.supportedCatalogIds)
      expect(clientCapabilities(c.extensionIds)).toEqual(c.expected.clientCapabilities)
    })
  // The basic catalog's 14 plus round 1's 15 core functions (`set` incl.).
  test(`the 31 built-ins are the catalog's`, () => expect(BUILTIN_FUNCTIONS.length).toBe(31))
})

describe(`host-router.json`, () => {
  const packages = routerFixture.packages as unknown as Record<string, VappPackage>
  for (const v of routerFixture.validation) test(`validation: ${v.package}`, () => expect(validatePackage(packages[v.package])).toEqual(v.expected))
  for (const flow of routerFixture.flows as { name: string; extensionIds?: string[]; packages?: string[]; installIssues?: Record<string, unknown>; steps: { message: unknown; expected: unknown }[] }[])
    test(`flow: ${flow.name}`, () => {
      const r = new HostRouter({ extensionIds: flow.extensionIds ?? [] })
      for (const id of flow.packages ?? []) expect(r.installPackage(packages[id]!)).toEqual(flow.installIssues![id] as never)
      for (const step of flow.steps) expect(r.route(step.message)).toEqual(step.expected as never)
    })
  test(`every error code the router emits is in the contract`, () => {
    const codes = new Set<string>()
    for (const flow of routerFixture.flows) for (const s of flow.steps) for (const op of s.expected as { op: string; message?: { error?: { code: string } } }[]) if (op.message?.error) codes.add(op.message.error.code)
    for (const code of codes) expect(HOST_ERROR_CODES).toContain(code)
    expect(hostContract.ops.kinds.sort()).toEqual([`bind`, `components`, `create`, `data`, `delete`, `send`])
  })

  test(`the surface section matches the Formatter and the a11y commands`, () => {
    expect(hostContract.version).toBe(2)
    expect([...FORMATTER_METHODS].sort()).toEqual(Object.keys(englishFormatter()).filter((k) => k !== `locale`).sort())
    expect(SURFACE_COMMANDS).toEqual(Object.keys(A11Y_COMMANDS))
    for (const key of [`locale`, `timeZone`]) expect(SURFACE_SETTING_KEYS).toContain(key)
  })
})

const devicesPackage = routerFixture.packages[`acme.devices`] as unknown as VappPackage

describe(`ExponentialHost`, () => {
  test(`messages through the in-memory transport build surfaces`, () => {
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport })
    transport.feedJsonl(
      [
        JSON.stringify({ version: `v0.9`, createSurface: { surfaceId: `s1`, catalogId: CORE_CATALOG_ID } }),
        JSON.stringify({ version: `v0.9`, updateComponents: { surfaceId: `s1`, components: [{ id: `root`, component: `Text`, text: { path: `/name` } }] } }),
        JSON.stringify({ version: `v0.9`, updateDataModel: { surfaceId: `s1`, path: `/name`, value: `Ada` } }),
      ].join(`\n`)
    )
    host.connect()
    expect(host.status).toBe(`open`)
    const s = host.surface(`s1`)!
    expect(s.root?.component).toBe(`Text`)
    expect(s.data).toEqual({ name: `Ada` })
    transport.feed({ version: `v0.9`, deleteSurface: { surfaceId: `s1` } })
    expect(host.surface(`s1`)).toBeUndefined()
  })

  test(`an unsupported catalog answers UNSUPPORTED_CATALOG and raises the banner`, () => {
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport })
    host.connect()
    transport.feed({ version: `v0.9`, createSurface: { surfaceId: `s1`, catalogId: `https://ui.exponential.at/catalogs/core/v2` } })
    expect(host.unsupportedCatalog).toBe(`https://ui.exponential.at/catalogs/core/v2`)
    expect((transport.sent[0] as { error: { code: string } }).error.code).toBe(`UNSUPPORTED_CATALOG`)
  })

  test(`applyTemplate + a source resolver feed the data model; delete cancels`, () => {
    let cancelled = 0
    let emit: (v: unknown) => void = () => {}
    const host = new ExponentialHost({
      packages: [devicesPackage],
      sources: {
        exp: (source, e) => {
          expect(source.name).toBe(`devices`)
          emit = e
          e([{ id: `d1`, name: `MacBook` }])
          return () => cancelled++
        },
      },
    })
    host.receive({ version: `v0.9`, applyTemplate: { surfaceId: `d`, templateId: `list`, data: { filter: `online` } } })
    const s = host.surface(`d`)!
    expect(s.data).toEqual({ title: `Devices`, filter: `online`, devices: [{ id: `d1`, name: `MacBook` }] })
    emit([])
    expect(s.data.devices).toEqual([])
    host.receive({ version: `v0.9`, deleteSurface: { surfaceId: `d` } })
    expect(cancelled).toBe(1)
  })

  test(`a bind without a resolver reports VALIDATION_FAILED`, () => {
    const sent: ClientMessage[] = []
    const host = new ExponentialHost({ onSend: (m) => sent.push(m) })
    host.receive({ version: `v0.9`, createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG_ID } })
    host.receive({ version: `v0.9`, bindDataModel: { surfaceId: `s`, path: `/x`, source: `acme:things` } })
    expect(sent).toEqual([{ version: `v0.9`, error: { code: `VALIDATION_FAILED`, surfaceId: `s`, message: `no resolver for the source scheme acme`, path: `/x` } }])
  })

  test(`functions: registered + allowed runs, ask goes through consent, a package narrows`, async () => {
    const sent: ClientMessage[] = []
    const calls: string[] = []
    let consent = false
    const host = new ExponentialHost({
      onSend: (m) => sent.push(m),
      packages: [devicesPackage],
      sources: { exp: () => {} },
      functions: { "harness.toast": (a) => calls.push(`toast ${a.message}`), "harness.mcp": () => (calls.push(`mcp`), 42) },
      policy: { functions: { ask: [`harness.mcp`] }, onFunctionCall: () => consent },
    })
    host.receive({ version: `v0.9`, createSurface: { surfaceId: `free`, catalogId: CORE_CATALOG_ID } })
    host.receive({ version: `v0.9`, applyTemplate: { surfaceId: `pkg`, templateId: `list` } })
    const info = (surfaceId: string, name: string) => ({ surfaceId, componentId: `b`, name, args: { message: `hi` } })
    expect(await host.callFunction(info(`free`, `harness.toast`))).toEqual({ decision: `allow`, result: 1 })
    expect((await host.callFunction(info(`free`, `harness.mcp`))).decision).toBe(`deny`)
    consent = true
    expect(await host.callFunction(info(`free`, `harness.mcp`))).toEqual({ decision: `allow`, result: 42 })
    // the package lists only harness.toast
    expect((await host.callFunction(info(`pkg`, `harness.mcp`))).decision).toBe(`deny`)
    expect((await host.callFunction(info(`pkg`, `harness.toast`))).decision).toBe(`allow`)
    expect((await host.callFunction(info(`free`, `nope`))).decision).toBe(`not_found`)
    expect(calls).toEqual([`toast hi`, `mcp`, `toast hi`])
    expect(sent.map((m) => (`error` in m ? m.error.code : `action`))).toEqual([`FUNCTION_DENIED`, `FUNCTION_DENIED`, `FUNCTION_NOT_FOUND`])
  })

  test(`openUrl obeys the URL policy; actions become A2UI client messages`, async () => {
    const opened: string[] = []
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport, policy: { openUrl: (u) => opened.push(u), urls: { hosts: [`exponential.at`] } } })
    expect(host.openUrl(`https://exponential.at/x`)).toBe(true)
    expect(host.openUrl(`https://evil.example/`)).toBe(false)
    expect(host.openUrl(`javascript:alert(1)`)).toBe(false)
    expect((await host.callFunction({ surfaceId: `s`, componentId: `l`, name: `openUrl`, args: { url: `https://exponential.at/y` } })).decision).toBe(`allow`)
    expect(opened).toEqual([`https://exponential.at/x`, `https://exponential.at/y`])
    host.connect()
    host.action({ surfaceId: `s`, componentId: `btn`, name: `save`, context: { id: 1 }, payload: { value: `a` }, timestamp: `2026-10-08T00:00:00.000Z` })
    expect(transport.sent).toEqual([{ version: `v0.9`, action: { name: `save`, surfaceId: `s`, sourceComponentId: `btn`, timestamp: `2026-10-08T00:00:00.000Z`, context: { id: 1 }, payload: { value: `a` } } }])
  })

  test(`negotiation follows registered extensions`, () => {
    const host = new ExponentialHost()
    host.registerExtension({ id: `https://acme.example/catalog/v1`, name: `Acme`, extends: CORE_CATALOG_ID, components: {} })
    expect(host.supportedCatalogIds.at(-1)).toBe(`https://acme.example/catalog/v1`)
    expect(host.clientCapabilities()[`v0.9`].supportedCatalogIds).toEqual(host.supportedCatalogIds)
  })

  test(`media requests carry the auth rules`, () => {
    const host = new ExponentialHost({ policy: { media: { baseUrl: `https://app.exponential.at`, rules: [{ prefix: `https://app.exponential.at/api/attachments/`, headers: { authorization: `Bearer k` } }] } } })
    expect(host.mediaRequest(`/api/attachments/a`)).toEqual({ url: `https://app.exponential.at/api/attachments/a`, headers: { authorization: `Bearer k` } })
  })

  test(`VAPP-103: a src the media policy denies builds no request`, () => {
    const host = new ExponentialHost({ policy: { media: { baseUrl: `file:///Users/me/` } } })
    expect(host.mediaRequest(`/etc/passwd`)).toBeNull()
    expect(host.mediaRequest(`javascript:alert(1)`)).toBeNull()
    expect(imageDimensions(new Uint8Array([0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x10, 0, 0x20, 0]))).toEqual([16, 32])
    expect(MEDIA_LIMITS).toEqual(hostContract.media.limits)
  })

  test(`VAPP-103: paintError → ONE RENDER_FAILED error per surface + component + message, reset by new components`, () => {
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport })
    host.connect()
    transport.feed({ version: `v0.9`, createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG_ID } })
    const e = { surfaceId: `s`, componentId: `c`, message: `boom` }
    host.paintError(e)
    host.paintError(e)
    expect(transport.sent).toEqual([{ version: `v0.9`, error: { code: `RENDER_FAILED`, surfaceId: `s`, message: `boom`, path: `/components/c` } }])
    transport.feed({ version: `v0.9`, updateComponents: { surfaceId: `s`, components: [{ id: `root`, component: `Text`, text: `x` }] } })
    host.paintError(e)
    expect(transport.sent).toHaveLength(2)
    expect(host.issues.at(-1)?.code).toBe(`RENDER_FAILED`)
  })
})

describe(`review fixes`, () => {
  test(`createSurface.theme: a built-in id or a theme JSON lands on the store; an unusable one is an issue`, () => {
    const sent: ClientMessage[] = []
    const host = new ExponentialHost({ onSend: (m) => sent.push(m) })
    host.receive({ version: `v0.9`, createSurface: { surfaceId: `a`, catalogId: CORE_CATALOG_ID, theme: `neutral` } })
    expect(host.surface(`a`)!.theme?.id).toBe(`neutral`)
    host.receive({ version: `v0.9`, createSurface: { surfaceId: `b`, catalogId: CORE_CATALOG_ID, theme: { ...neutralTheme, id: `acme` } } })
    expect(host.surface(`b`)!.theme?.id).toBe(`acme`)
    host.receive({ version: `v0.9`, createSurface: { surfaceId: `c`, catalogId: CORE_CATALOG_ID } })
    expect(host.surface(`c`)!.theme).toBeUndefined()
    expect(sent).toEqual([])
    host.receive({ version: `v0.9`, createSurface: { surfaceId: `d`, catalogId: CORE_CATALOG_ID, theme: `nope` } })
    host.receive({ version: `v0.9`, createSurface: { surfaceId: `e`, catalogId: CORE_CATALOG_ID, theme: { id: `broken` } } })
    expect(host.surface(`d`)!.theme).toBeUndefined()
    expect(host.surface(`e`)!.theme).toBeUndefined()
    expect(sent.map((m) => (`error` in m ? [m.error.code, m.error.surfaceId, m.error.path] : null))).toEqual([
      [`VALIDATION_FAILED`, `d`, `/createSurface/theme`],
      [`VALIDATION_FAILED`, `e`, `/createSurface/theme`],
    ])
    expect(host.issues.map((i) => [i.code, i.surfaceId])).toEqual([
      [`VALIDATION_FAILED`, `d`],
      [`VALIDATION_FAILED`, `e`],
    ])
    expect(host.issues[0]!.message).toContain(`unknown built-in theme "nope"`)
  })

  test(`a throwing or rejecting consent hook is a deny with the error, never a rejected call`, async () => {
    const sent: ClientMessage[] = []
    let ran = 0
    const make = (hook: () => boolean | Promise<boolean>) =>
      new ExponentialHost({ onSend: (m) => sent.push(m), functions: { "harness.mcp": () => ++ran }, policy: { functions: { ask: [`harness.mcp`] }, onFunctionCall: hook } })
    const call = { surfaceId: `s`, componentId: `b`, name: `harness.mcp`, args: {} }
    expect(
      await make(() => {
        throw new Error(`card unmounted`)
      }).callFunction(call)
    ).toEqual({ decision: `deny`, error: `card unmounted` })
    expect(await make(() => Promise.reject(new Error(`dialog closed`))).callFunction(call)).toEqual({ decision: `deny`, error: `dialog closed` })
    expect(ran).toBe(0)
    expect(sent.map((m) => (`error` in m ? `${m.error.code}: ${m.error.message}` : ``))).toEqual([
      `FUNCTION_DENIED: harness.mcp was not allowed: the consent hook failed: card unmounted`,
      `FUNCTION_DENIED: harness.mcp was not allowed: the consent hook failed: dialog closed`,
    ])
  })

  test(`packages: an unusable one in the options throws; installPackage + TEMPLATE_NOT_FOUND reach a transport-less host`, () => {
    const broken = routerFixture.packages.broken as unknown as VappPackage
    expect(() => new ExponentialHost({ packages: [devicesPackage, broken] })).toThrow(PackageError)
    const seen: HostIssue[] = []
    const host = new ExponentialHost({ onIssue: (i) => seen.push(i) })
    expect(host.hasTransport).toBe(false)
    const issues = host.installPackage(broken)
    expect(issues.length).toBeGreaterThan(0)
    expect(host.issues.map((i) => [i.code, i.path])).toEqual(issues.map((i) => [`PACKAGE_INVALID`, i.path]))
    host.receive({ version: `v0.9`, applyTemplate: { surfaceId: `devices`, templateId: `list` } })
    expect(host.surface(`devices`)).toBeUndefined()
    expect(host.issues.at(-1)).toEqual({ code: `TEMPLATE_NOT_FOUND`, surfaceId: `devices`, message: `no installed package has the template list` })
    expect(seen).toEqual([...host.issues])
  })

  test(`the store keeps the reducer's lifted templates; a new extension re-reduces`, () => {
    const host = new ExponentialHost()
    host.receive({ version: `v0.9`, createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG_ID } })
    host.receive({
      version: `v0.9`,
      updateComponents: {
        surfaceId: `s`,
        components: [
          { id: `root`, component: `List`, children: { componentId: `row`, path: `/rows` } },
          { id: `row`, component: `Text`, text: { path: `title` } },
        ],
      },
    })
    const s = host.surface(`s`)!
    expect(Object.keys(s.templates ?? {})).toEqual([`row`])
    expect(s.templates).toBe(s.templates)
    const v = s.version
    host.registerExtension({ id: `https://acme.example/catalog/v1`, name: `Acme`, extends: CORE_CATALOG_ID, components: {} })
    expect(s.version).toBe(v + 1)
  })
})

/** A fetch whose GET answers each call with the next body (a string = a
 *  stream that ends cleanly, an Error = a failed request). */
function streamFetch(bodies: (string | Error)[]) {
  const gets: number[] = []
  const f = (async (_url: RequestInfo | URL, init?: RequestInit) => {
    if (init?.method === `POST`) return new Response(null, { status: 204 })
    gets.push(Date.now())
    const next = bodies[Math.min(gets.length - 1, bodies.length - 1)]!
    if (next instanceof Error) throw next
    return new Response(next, { status: 200 })
  }) as typeof fetch
  return { f, gets }
}

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms))
const line = (surfaceId: string) => JSON.stringify({ version: `v0.9`, createSurface: { surfaceId, catalogId: CORE_CATALOG_ID } }) + `\n`

describe(`stream transports`, () => {
  test(`a clean end of stream closes the transport: no reconnect, no re-delivery`, async () => {
    const { f, gets } = streamFetch([line(`s1`)])
    const t = new JsonlStreamTransport({ url: `https://x.test/a2ui`, reconnectMs: 5, fetch: f })
    const got: unknown[] = []
    const statuses: TransportStatus[] = []
    t.start((m) => got.push(m), (s) => statuses.push(s))
    await tick(40)
    expect(gets.length).toBe(1)
    expect(got.length).toBe(1)
    expect(statuses).toEqual([`connecting`, `open`, `closed`])
  })

  test(`an error reconnects; resumable (option or SSE retry:) reconnects after a clean end`, async () => {
    const failing = streamFetch([new Error(`ECONNRESET`), line(`s1`)])
    const statuses: TransportStatus[] = []
    new JsonlStreamTransport({ url: `u`, reconnectMs: 5, fetch: failing.f }).start(() => {}, (s) => statuses.push(s))
    await tick(40)
    expect(failing.gets.length).toBe(2)
    expect(statuses).toEqual([`connecting`, `error`, `connecting`, `open`, `closed`])

    const resumable = streamFetch([line(`s1`)])
    const a = new JsonlStreamTransport({ url: `u`, reconnectMs: 5, resumable: true, fetch: resumable.f })
    a.start(() => {}, () => {})
    await tick(40)
    a.close()
    expect(resumable.gets.length).toBeGreaterThan(1)

    const sse = streamFetch([`retry: 5\ndata: ${line(`s1`)}\n`])
    const b = new SseTransport({ url: `u`, fetch: sse.f })
    b.start(() => {}, () => {})
    await tick(40)
    b.close()
    expect(sse.gets.length).toBeGreaterThan(1)
  })

  test(`close() then start() leaves exactly one loop (a stale one asleep in its wait exits)`, async () => {
    const { f, gets } = streamFetch([new Error(`down`)])
    const t = new JsonlStreamTransport({ url: `u`, reconnectMs: 20, fetch: f })
    t.start(() => {}, () => {})
    await tick(5) // the first loop failed and sleeps in its reconnect wait
    t.close()
    t.start(() => {}, () => {})
    await tick(5)
    const before = gets.length
    expect(before).toBe(2)
    await tick(30) // one reconnect, from the new loop only
    t.close()
    expect(gets.length).toBe(3)
  })
})
