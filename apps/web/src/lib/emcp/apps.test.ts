import { describe, expect, it } from "vitest"
import {
  MCP_APP_MIME_TYPE,
  MCP_APP_RESOURCE_URIS,
  mcpAppDocument,
  mcpAppHtml,
  mcpAppToolMeta,
  registerExponentialApps,
} from "./apps"

describe(`MCP Apps (EXP-1183)`, () => {
  it(`stamps the view into the built document`, () => {
    const built = `<html><head><meta name="exp-view" content="issues" /></head></html>`
    expect(mcpAppHtml(`devices`, built)).toContain(
      `<meta name="exp-view" content="devices" />`
    )
    expect(mcpAppHtml(`devices`, built)).not.toContain(`content="issues"`)
  })

  it(`points the stylesheet's /fonts/ urls at the app's origin (#EXP-1766)`, () => {
    const built = `<html><head><meta name="exp-view" content="issues" /><style>@font-face{src:url(/fonts/JetBrainsMono-Regular.woff2) format("woff2")}@font-face{src:url("/fonts/JetBrainsMono-Bold.woff2")}</style></head></html>`
    const html = mcpAppHtml(`issues`, built, `https://app.example.com/`)
    expect(html).toContain(`url(https://app.example.com/fonts/JetBrainsMono-Regular.woff2)`)
    expect(html).toContain(`url("https://app.example.com/fonts/JetBrainsMono-Bold.woff2")`)
    expect(html).not.toContain(`url(/fonts/`)
    // Without an origin the document is untouched.
    expect(mcpAppHtml(`issues`, built)).toContain(`url(/fonts/JetBrainsMono-Regular.woff2)`)
  })

  it(`#VAPP-292: the genUI view is its own document, told the instance origin`, () => {
    expect(mcpAppDocument(`genui`)).toBe(`genui.html`)
    expect(mcpAppDocument(`issues`)).toBe(`app.html`)
    const built = `<html><head><meta name="exp-view" content="genui" /><meta name="exp-origin" content="" /></head></html>`
    const html = mcpAppHtml(`genui`, built, `https://app.example.com/`)
    expect(html).toContain(`<meta name="exp-origin" content="https://app.example.com" />`)
    // Nothing but a URL's own characters lands in the attribute.
    expect(mcpAppHtml(`genui`, built, `https://a.example"><script>`)).toContain(`content="https://a.examplescript"`)
    expect(mcpAppToolMeta(`genui`)).toEqual({ ui: { resourceUri: `ui://exponential/genui` } })
  })

  it(`serves a placeholder document when the views are not built`, () => {
    expect(mcpAppHtml(`issues`, null)).toContain(`not built`)
  })

  it(`binds tools to their view; only the list advertises an entrypoint`, () => {
    expect(mcpAppToolMeta(`issues`)).toEqual({
      ui: { resourceUri: `ui://exponential/issues` },
      "openai/ui": { entrypoints: [{ type: `global` }] },
    })
    for (const view of [`inbox`, `devices`] as const) {
      expect(mcpAppToolMeta(view)).toEqual({
        ui: { resourceUri: `ui://exponential/${view}` },
      })
    }
  })

  it(`registers every view as an mcp-app resource`, async () => {
    const registered: Array<{ uri: string; mimeType?: string; read: () => Promise<{ contents: Array<{ mimeType: string; text: string }> }> }> = []
    registerExponentialApps({
      registerResource: (_name: string, uri: string, config: { mimeType?: string }, read: () => Promise<never>) => {
        registered.push({ uri, mimeType: config.mimeType, read })
      },
    } as never, `https://app.example`)
    expect(registered.map((r) => r.uri)).toEqual(
      Object.values(MCP_APP_RESOURCE_URIS)
    )
    for (const resource of registered) {
      expect(resource.mimeType).toBe(MCP_APP_MIME_TYPE)
      const { contents } = await resource.read()
      expect(contents[0].mimeType).toBe(MCP_APP_MIME_TYPE)
      expect(contents[0].text).toContain(`<meta name="exp-view"`)
      expect(JSON.stringify(contents[0])).toContain(`"resourceDomains":["https://app.example"`)
    }
  })
})
