import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { VIEW_NAMES, viewResourceUri } from "@exp/mcp-views/contract"
import { loadViewHtml, viewToolMeta } from "./views"

// EXP-1153: the ui:// resources are the BUILT views; a missing build must
// fail loudly at read time, never hand a host an empty iframe.
describe(`MCP App views`, () => {
  it(`links a tool to its view the MCP Apps way`, () => {
    const meta = viewToolMeta(`issue`)
    expect(meta.ui).toEqual({
      resourceUri: `ui://exponential/issue`,
      visibility: [`model`, `app`],
    })
    expect(meta[`openai/ui`]).toBeUndefined()
    // The thread-panel entrypoint rides only on the tool that accepts `{}`.
    expect(viewToolMeta(`board`, { threadEntrypoint: true })[`openai/ui`]).toEqual({
      entrypoints: [{ type: `thread` }],
    })
  })

  it(`names one resource per view`, () => {
    expect(VIEW_NAMES.map(viewResourceUri)).toEqual([
      `ui://exponential/board`,
      `ui://exponential/issue`,
    ])
  })

  it(`reads a built view as one self-contained document`, () => {
    let html: string
    try {
      html = loadViewHtml(`board`)
    } catch (e) {
      // Not built in this checkout: the error must say how to fix it.
      expect(String(e)).toContain(`build:mcp-views`)
      return
    }
    expect(html.startsWith(`<!doctype html>`)).toBe(true)
    expect(html).toContain(`<div id="root">`)
    expect(html).toContain(`ui/initialize`)
    // Nothing external: the sandbox CSP allows no origins.
    expect(html).not.toMatch(/<(script|link)[^>]+(src|href)="https?:/)
  })

  it(`refuses a view that was never built`, () => {
    expect(() => loadViewHtml(`nope` as never)).toThrow(/build:mcp-views/)
  })
})
