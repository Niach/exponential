import { describe, expect, it } from "vitest"
import {
  MCP_APP_MIME_TYPE,
  MCP_APP_RESOURCE_URIS,
  mcpAppHtml,
  mcpAppToolMeta,
  registerExponentialApps,
} from "./apps"

describe(`MCP Apps (EXP-1183)`, () => {
  it(`stamps the view into the built document`, () => {
    const built = `<html><head><meta name="exp-view" content="issues" /></head></html>`
    expect(mcpAppHtml(`run`, built)).toContain(
      `<meta name="exp-view" content="run" />`
    )
    expect(mcpAppHtml(`run`, built)).not.toContain(`content="issues"`)
  })

  it(`serves a placeholder document when the views are not built`, () => {
    expect(mcpAppHtml(`issues`, null)).toContain(`not built`)
  })

  it(`binds tools to their view; only the list advertises an entrypoint`, () => {
    expect(mcpAppToolMeta(`issues`)).toEqual({
      ui: { resourceUri: `ui://exponential/issues` },
      "openai/ui": { entrypoints: [{ type: `global` }] },
    })
    expect(mcpAppToolMeta(`run`)).toEqual({
      ui: { resourceUri: `ui://exponential/run` },
    })
  })

  it(`registers every view as an mcp-app resource`, async () => {
    const registered: Array<{ uri: string; mimeType?: string; read: () => Promise<{ contents: Array<{ mimeType: string; text: string }> }> }> = []
    registerExponentialApps({
      registerResource: (_name: string, uri: string, config: { mimeType?: string }, read: () => Promise<never>) => {
        registered.push({ uri, mimeType: config.mimeType, read })
      },
    } as never)
    expect(registered.map((r) => r.uri)).toEqual(
      Object.values(MCP_APP_RESOURCE_URIS)
    )
    for (const resource of registered) {
      expect(resource.mimeType).toBe(MCP_APP_MIME_TYPE)
      const { contents } = await resource.read()
      expect(contents[0].mimeType).toBe(MCP_APP_MIME_TYPE)
      expect(contents[0].text).toContain(`<meta name="exp-view"`)
    }
  })
})
