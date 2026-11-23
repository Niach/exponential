// #AXC-12 (VAPP-295/296): the vendored Express compiler is the rev eui-core
// pins; the three sample surfaces round-trip through it with zero diffs and
// their node counts (56/12/251, as natively); the host-transport.json
// Express cases replay; the transports read Express and hand its prose to
// the host's onText.
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import roundtrip from "../../fixtures/express-roundtrip.json" with { type: "json" }
import transport from "../../fixtures/host-transport.json" with { type: "json" }
import hostContract from "../../catalog/host.json" with { type: "json" }
import { reduceSurface } from "../reducer"
import type { FlatComponent, UiNode } from "../types"
import {
  EXPRESS_DEFAULT_SURFACE_ID,
  EXPRESS_MIME_TYPE,
  EXPRESS_SSE_EVENT,
  EXPRESS_STRUCTURED_KEY,
  EXPRESS_WS_KEY,
  ExponentialHost,
  ExpressDecoder,
  FrameDecoder,
  INFERENCE_FORMATS,
  INFERENCE_FORMATS_KEY,
  JsonlStreamTransport,
  McpTransport,
  SseDecoder,
  SseTransport,
  inferenceFormatOf,
  mergeDecoded,
  messagesFromMcpResult,
  rendererCapabilities,
} from "../host"
import type { Decoded } from "../host"
import { compileExpress, decompileExpress, expressPrompt, initExpress, unwrapExpress } from "."

const wasmUrl = new URL(`../../wasm/express/a2ui_express_bg.wasm`, import.meta.url)
initExpress(readFileSync(wasmUrl))

describe(`express wasm`, () => {
  test(`is built from the a2ui-express rev eui-core pins (else run scripts/build-express-wasm.sh)`, () => {
    const toml = readFileSync(new URL(`../../../../apps/desktop/crates/eui-core/Cargo.toml`, import.meta.url), `utf8`)
    const rev = /^a2ui-express = .*rev = "([0-9a-f]+)"/m.exec(toml)?.[1]
    expect(rev).toBeDefined()
    expect(readFileSync(new URL(`../../wasm/express/SOURCE_HASH`, import.meta.url), `utf8`).trim()).toBe(rev!)
  })

  // The full build (compile, stream, decompile, the prompt) is ~600 KB raw,
  // ~245 KB gzipped; a growth past this means a new rev grew on purpose.
  test(`stays inside the 640 KB raw budget`, () => {
    expect(readFileSync(wasmUrl).byteLength).toBeLessThanOrEqual(640_000)
  })
})

describe(`express-roundtrip.json (decompile → compile, zero diffs)`, () => {
  const count = (n: UiNode): number => 1 + n.children.reduce((a, c) => a + count(c), 0) + Object.values(n.slots ?? {}).reduce((a, c) => a + count(c), 0)
  const nodes = (messages: unknown[]) => {
    const create = (messages[0] as { createSurface: { components: FlatComponent[]; catalogId: string } }).createSurface
    const result = reduceSurface(create.components, { catalogId: create.catalogId, expand: false })
    expect(result.issues).toEqual([])
    // A template node is lifted out of the tree (it renders per item).
    return count(result.root) + Object.values(result.templates ?? {}).reduce((a, t) => a + count(t), 0)
  }
  const want: Record<string, number> = { dashboard: 56, "demo-surface": 12, "kitchen-sink": 251, "express-extras": 6 }
  for (const c of roundtrip.cases as { name: string; nodes: number; messages: unknown[] }[]) {
    test(`${c.name}: ${want[c.name]} nodes`, () => {
      expect(c.nodes).toBe(want[c.name]!)
      expect(nodes(c.messages)).toBe(c.nodes)
      const back = compileExpress(decompileExpress(c.messages), `main`)
      expect(back).toEqual(c.messages)
      expect(nodes(back)).toBe(c.nodes)
    })
  }

  test(`the <a2ui> tags are optional`, () => {
    const c = roundtrip.cases[1]!
    const express = decompileExpress(c.messages)
    expect(compileExpress(`<a2ui>\n${express}\n</a2ui>`, `main`)).toEqual(compileExpress(express, `main`))
  })

  test(`the compact prompt names the catalog's components`, () => {
    const prompt = expressPrompt()
    expect(prompt).toContain(`Button`)
    expect(prompt.length).toBeGreaterThan(10_000)
  })
})

const all = (dec: { push(c: string): Decoded; end(): Decoded }, chunks: string[]) => {
  const out: Decoded = { messages: [], issues: [] }
  for (const d of [...chunks.map((c) => dec.push(c)), dec.end()]) mergeDecoded(out, d)
  return out
}
const at = (c: { surfaceId?: string }) => (c.surfaceId ? { surfaceId: c.surfaceId } : {})

describe(`host-transport.json express`, () => {
  test(`the contract keys are catalog/host.json inference`, () => {
    const inf = hostContract.inference
    expect(INFERENCE_FORMATS).toEqual(inf.formats as never)
    expect(INFERENCE_FORMATS_KEY).toBe(inf.capabilitiesKey)
    expect([EXPRESS_MIME_TYPE, EXPRESS_STRUCTURED_KEY, EXPRESS_SSE_EVENT, EXPRESS_WS_KEY, EXPRESS_DEFAULT_SURFACE_ID]).toEqual([
      inf.express.mimeType,
      inf.express.structuredContentKey,
      inf.express.sseEvent,
      inf.express.wsFrameKey,
      inf.express.defaultSurfaceId,
    ])
  })
  for (const c of transport.express) test(`express: ${c.name}`, () => expect(all(new ExpressDecoder({ wrapped: c.wrapped, ...at(c) }), c.chunks)).toEqual(c.expected as Decoded))
  // #VAPP-300: the open block after every read.
  const opens = (dec: { push(c: string): Decoded; end(): Decoded }, chunks: string[]) => [...chunks.map((c) => dec.push(c)), dec.end()].map((d) => d.open ?? [])
  for (const c of transport.expressOpen as { name: string; decoder: string; wrapped?: boolean; surfaceId?: string; chunks: string[]; expected: string[][] }[])
    test(`open: ${c.name}`, () => expect(opens(c.decoder === `sse` ? new SseDecoder(at(c)) : c.decoder === `ws` ? new FrameDecoder(at(c)) : new ExpressDecoder({ wrapped: c.wrapped, ...at(c) }), c.chunks)).toEqual(c.expected))
  for (const c of transport.expressSse) test(`sse: ${c.name}`, () => expect(all(new SseDecoder(at(c as { surfaceId?: string })), c.chunks)).toEqual(c.expected as Decoded))
  for (const c of transport.expressWs) test(`ws: ${c.name}`, () => expect(all(new FrameDecoder(at(c as { surfaceId?: string })), c.frames)).toEqual(c.expected as Decoded))
  for (const c of transport.expressMcp) test(`mcp: ${c.name}`, () => expect(messagesFromMcpResult(c.result, at(c as { surfaceId?: string }))).toEqual(c.expected as Decoded))
  for (const c of transport.expressContentTypes) test(`contentType: ${c.name}`, () => expect(inferenceFormatOf(c.contentType)).toBe(c.expected as never))
  for (const c of transport.expressCapabilities)
    test(`capabilities: ${c.name}`, () => {
      const formats = (c as { inferenceFormats?: string[] }).inferenceFormats
      expect(rendererCapabilities(c.extensionIds, formats ? { inferenceFormats: formats } : {})).toEqual(c.expected as never)
    })
})

const body = `root = Stack(gap="sm", children=[title, ok])\ntitle = Text(text="Release 0.18.84")\nok = Button(label="Promote", variant="default")\n`
const response = `Here is the release.\n<a2ui>\n${body}</a2ui>\nWant me to promote it?`

/** A streamed fetch response in small chunks. */
function streamed(text: string, type: string): typeof fetch {
  return (async () => {
    const bytes = new TextEncoder().encode(text)
    const stream = new ReadableStream<Uint8Array>({
      start(c) {
        for (let i = 0; i < bytes.length; i += 5) c.enqueue(bytes.slice(i, i + 5))
        c.close()
      },
    })
    return new Response(stream, { headers: { "content-type": type } })
  }) as unknown as typeof fetch
}

async function run(host: ExponentialHost, until: () => boolean) {
  host.connect()
  for (let i = 0; i < 200 && !until(); i++) await new Promise((r) => setTimeout(r, 5))
}

describe(`transports read Express (onText = the prose)`, () => {
  test(`an application/a2ui+express HTTP body`, async () => {
    const text: string[] = []
    const host = new ExponentialHost({ transport: new JsonlStreamTransport({ url: `https://agent.test/s`, fetch: streamed(response, `${EXPRESS_MIME_TYPE}; charset=utf-8`) }), onText: (t) => text.push(t) })
    await run(host, () => host.status === `closed`)
    expect(host.surfaceIds()).toEqual([`main`])
    expect(text.join(``)).toBe(`Here is the release.\n\nWant me to promote it?`)
  })

  test(`a2ui-express SSE events`, async () => {
    const sse = `event: ${EXPRESS_SSE_EVENT}\ndata: Here is the release.\ndata: <a2ui>\ndata: ${body.trimEnd().split(`\n`).join(`\ndata: `)}\ndata: </a2ui>\ndata: Done.\n\n`
    const text: string[] = []
    const host = new ExponentialHost({ transport: new SseTransport({ url: `https://agent.test/s`, surfaceId: `s9`, fetch: streamed(sse, `text/event-stream`) }), onText: (t) => text.push(t) })
    await run(host, () => host.status === `closed`)
    expect(host.surfaceIds()).toEqual([`s9`])
    expect(text.join(``)).toBe(`Here is the release.\n\nDone.`)
  })

  test(`an Express MCP resource`, async () => {
    let id = 0
    const mcp = (async (_url: string, init: { body: string }) => {
      const req = JSON.parse(init.body) as { id: number; method: string }
      id = req.id
      const result = req.method === `tools/call` ? { structuredContent: { [EXPRESS_STRUCTURED_KEY]: body } } : {}
      return new Response(JSON.stringify({ jsonrpc: `2.0`, id, result }), { headers: { "content-type": `application/json` } })
    }) as unknown as typeof fetch
    const host = new ExponentialHost({ transport: new McpTransport({ url: `https://agent.test/mcp`, tool: `show`, fetch: mcp }) })
    await run(host, () => host.surfaceIds().length > 0)
    expect(host.surfaceIds()).toEqual([`main`])
  })

  test(`the host advertises both formats unless told otherwise`, () => {
    expect(new ExponentialHost().rendererCapabilities()[`1.0`].inferenceFormats).toEqual([`json`, `express`])
    expect(new ExponentialHost({ inferenceFormats: [`json`] }).rendererCapabilities()[`1.0`].inferenceFormats).toEqual([`json`])
  })
})

// #VAPP-300: a streaming surface draws no Unknown placeholder for a child
// that has not arrived yet; once its block closes, a child still missing is
// the placeholder (and its issue goes to the agent) as on any JSON surface.
describe(`a streaming Express block renders a child not streamed yet as nothing`, () => {
  const unknowns = (n: UiNode | null | undefined): string[] => (n ? [...(n.component === `Unknown` ? [n.id] : []), ...n.children.flatMap(unknowns), ...Object.values(n.slots ?? {}).flatMap(unknowns)] : [])
  /** A fetch whose body the test releases chunk by chunk. */
  const gated = (type: string) => {
    let push!: (text: string | null) => void
    const stream = new ReadableStream<Uint8Array>({
      start(c) {
        push = (text) => (text === null ? c.close() : c.enqueue(new TextEncoder().encode(text)))
      },
    })
    const fetchBody = (async () => new Response(stream, { headers: { "content-type": type } })) as unknown as typeof globalThis.fetch
    return { fetch: fetchBody, push: (text: string | null) => push(text) }
  }
  const tick = () => new Promise((r) => setTimeout(r, 5))
  const timeline = [`Here is the release.\n<a2ui>\nroot = Stack(gap="sm", children=[title, later, ok])\n`, `title = Text(text="Release 0.18.84")\n`, `ok = Button(label="Promote", variant="default")\n`]

  async function host(close: string) {
    const wire = gated(EXPRESS_MIME_TYPE)
    const sent: unknown[] = []
    const transport = new JsonlStreamTransport({ url: `https://agent.test/s`, fetch: wire.fetch })
    transport.send = async (m) => void sent.push(m)
    const h = new ExponentialHost({ transport })
    h.connect()
    for (let i = 0; i < 20 && h.status !== `open`; i++) await tick()
    const mid: { ids: string[] | undefined; unknown: string[] }[] = []
    for (const chunk of timeline) {
      wire.push(chunk)
      await tick()
      const root = h.surface(`main`)?.root
      mid.push({ ids: root?.children.map((c) => c.id), unknown: unknowns(root) })
    }
    expect(sent).toEqual([])
    wire.push(close)
    wire.push(null)
    for (let i = 0; i < 50 && h.status !== `closed`; i++) await tick()
    return { h, mid, sent }
  }

  test(`mid-stream: no Unknown node, no issue sent`, async () => {
    const { mid } = await host(`later = Text(text="Arrived")\n</a2ui>\nDone.`)
    // The root came first: its children arrive one by one, `later` never
    // mid-stream, and nothing draws in its place.
    expect(mid).toEqual([
      { ids: [], unknown: [] },
      { ids: [`title`], unknown: [] },
      { ids: [`title`, `ok`], unknown: [] },
    ])
  })

  test(`closed with every child: no Unknown node`, async () => {
    const { h, sent } = await host(`later = Text(text="Arrived")\n</a2ui>\nDone.`)
    expect(unknowns(h.surface(`main`)?.root)).toEqual([])
    expect(sent).toEqual([])
  })

  test(`closed with a child still missing: the Unknown placeholder and its issue`, async () => {
    const { h, sent } = await host(`</a2ui>\nDone.`)
    expect(unknowns(h.surface(`main`)?.root)).toEqual([`later`])
    expect(JSON.stringify(sent)).toContain(`no component with this id`)
  })

  test(`a JSON surface keeps the placeholder at once`, () => {
    const h = new ExponentialHost()
    h.receive({ version: `1.0`, createSurface: { surfaceId: `j`, catalogId: `https://ui.exponential.at/catalogs/core/v1` } })
    h.receive({ version: `1.0`, updateComponents: { surfaceId: `j`, components: [{ id: `root`, component: `Stack`, children: [`later`] }] } })
    expect(unknowns(h.surface(`j`)?.root)).toEqual([`later`])
  })
})

// #EXP-2067: the same split eui-core's `express::unwrap` asserts
// (tests/express_unwrap.rs), over the wasm.
describe(`unwrapExpress`, () => {
  test(`prose and blocks in order`, () => {
    expect(unwrapExpress(`Here is the build:\n<a2ui>\nroot = Text(text="Green")\n</a2ui>\nShip it?`)).toEqual([
      { text: `Here is the build:` },
      { a2ui_raw: `root = Text(text="Green")` },
      { text: `Ship it?` },
    ])
  })

  test(`a fence around a block is dropped`, () => {
    expect(unwrapExpress("Look:\n```\n<a2ui>\nroot = Text(\"hi\")\n</a2ui>\n```\nafter")).toEqual([{ text: `Look:` }, { a2ui_raw: `root = Text("hi")` }, { text: `after` }])
  })

  test(`an open block is not final`, () => {
    expect(unwrapExpress(`end <a2ui>\nroot = Text("open`)).toEqual([{ text: `end` }, { a2ui_raw: `root = Text("open`, is_final: false }])
  })
})
