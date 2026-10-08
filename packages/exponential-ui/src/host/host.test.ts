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
} from "."
import type { ClientMessage, Decoded, FunctionDecision, FunctionPolicy, MediaOptions, UrlPolicy, VappPackage } from "."
import { CORE_CATALOG_ID } from "../catalog"

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
  test(`the 14 built-ins are the catalog's`, () => expect(BUILTIN_FUNCTIONS.length).toBe(14))
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
})
