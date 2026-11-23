// #AXC-12 (VAPP-299): fixtures/express-demo.json, the recorded Express
// response every guide app replays (guides/agent `GET /express`), compiles
// through the vendored core: the chunks, read in order by one wrapped
// ExpressDecoder, give exactly `expected` (no issues), and the surface they
// build reduces against the core catalog without an issue. A catalog or
// compiler change that breaks the demo fails here (and in eui-core
// tests/express_demo.rs).
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import demo from "../../fixtures/express-demo.json" with { type: "json" }
import { ExpressDecoder, mergeDecoded, messagesFromMcpResult, EXPRESS_MIME_TYPE } from "../host"
import type { Decoded } from "../host"
import { reduceSurface } from "../reducer"
import type { FlatComponent } from "../types"
import { compileExpress, initExpress } from "."

initExpress(readFileSync(new URL(`../../wasm/express/a2ui_express_bg.wasm`, import.meta.url)))

const BLOCK = /<a2ui>\n?([\s\S]*?)<\/a2ui>/

describe(`express-demo.json`, () => {
  const full = demo.chunks.map((c) => c.text).join(``)

  test(`the stream reads into exactly the expected messages and prose`, () => {
    const dec = new ExpressDecoder({ wrapped: true, surfaceId: demo.surfaceId })
    const out: Decoded = { messages: [], issues: [] }
    for (const c of demo.chunks) mergeDecoded(out, dec.push(c.text))
    mergeDecoded(out, dec.end())
    expect(out.issues).toEqual([])
    expect(out.messages).toEqual(demo.expected.messages)
    expect((out.text ?? []).join(``)).toBe(demo.expected.text)
  })

  test(`the whole block compiles to the same surface (one document)`, () => {
    expect(compileExpress(BLOCK.exec(full)![1]!, demo.surfaceId)).toEqual(compileExpress(BLOCK.exec(full)![0]!, demo.surfaceId))
    const mcp = messagesFromMcpResult({ content: [{ type: `resource`, resource: { uri: `a2ui://surface/main`, mimeType: EXPRESS_MIME_TYPE, text: BLOCK.exec(full)![1] } }] })
    expect(mcp.issues).toEqual([])
    expect(mcp.messages.length).toBeGreaterThan(0)
  })

  test(`the surface is existing core components only, reduced without an issue`, () => {
    const components = new Map<string, FlatComponent>()
    let catalogId = ``
    for (const m of demo.expected.messages as unknown as Record<string, { catalogId?: string; components?: FlatComponent[] }>[]) {
      const body = m.createSurface ?? m.updateComponents
      if (m.createSurface) catalogId = m.createSurface.catalogId!
      for (const c of body?.components ?? []) components.set(c.id, c)
    }
    const { issues } = reduceSurface([...components.values()], { catalogId })
    expect(issues).toEqual([])
    expect(new Set([...components.values()].map((c) => c.component))).toEqual(new Set([`Stack`, `Heading`, `Badge`, `Grid`, `StatTile`, `Card`, `Table`, `Button`]))
  })

  test(`a real stream: prose, then a block spanning many chunks, then prose`, () => {
    expect(demo.chunks.length).toBeGreaterThan(100)
    expect(demo.chunks.every((c) => c.delayMs > 0 && c.text.length > 0)).toBe(true)
    expect(full.indexOf(`<a2ui>`)).toBeGreaterThan(0)
    expect(full.trimEnd().endsWith(`</a2ui>`)).toBe(false)
  })
})
