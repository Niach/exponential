import { readFileSync, statSync } from "node:fs"
import { join } from "node:path"
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"

// EXP-1183 — MCP Apps (the stable `io.modelcontextprotocol/ui` extension):
// five tools name a `ui://` view in `_meta.ui.resourceUri`, and a host that
// renders apps (OpenClaw's dashboard, Claude, ChatGPT…) reads the view and
// mounts it in a sandbox beside the call, fed the call's result. Hosts
// without the extension ignore the field; the tools answer exactly as before.
//
// The views are ONE built document (`packages/mcp-apps`, emitted into the web
// public dir before the web build, like the widget); each resource is that
// document with its view stamped into the `exp-view` meta tag. The issue
// detail has no resource of its own: `exponential_issues_get` is always
// loaded and its budget has no room for the binding, so the issues view
// opens an issue in place (the host forwards its tools/call).

export const MCP_APP_MIME_TYPE = `text/html;profile=mcp-app`

export const MCP_APP_RESOURCE_URIS = {
  issues: `ui://exponential/issues`,
  run: `ui://exponential/run`,
  runs: `ui://exponential/runs`,
  inbox: `ui://exponential/inbox`,
  devices: `ui://exponential/devices`,
} as const

export type McpAppView = keyof typeof MCP_APP_RESOURCE_URIS

const VIEW_TITLES: Record<McpAppView, string> = {
  issues: `Exponential issues`,
  run: `Exponential run`,
  runs: `Exponential runs`,
  inbox: `Exponential inbox`,
  devices: `Exponential devices`,
}

/** A tool's `_meta` binding to its view. `exponential_issues_list` also
 *  advertises a global entrypoint (OpenAI's plugin extension, rendered by
 *  OpenClaw): the issue list opens without a model call, with `{}` = the
 *  caller's open issues. */
export function mcpAppToolMeta(view: McpAppView): Record<string, unknown> {
  return {
    ui: { resourceUri: MCP_APP_RESOURCE_URIS[view] },
    ...(view === `issues`
      ? { "openai/ui": { entrypoints: [{ type: `global` }] } }
      : {}),
  }
}

// Production runs from the repo root with the build in `.output/public`; the
// dev server runs in apps/web over `public/`.
const BUILT_APP_CANDIDATES = [
  join(`.output`, `public`, `mcp-apps`, `app.html`),
  join(`public`, `mcp-apps`, `app.html`),
  join(`apps`, `web`, `public`, `mcp-apps`, `app.html`),
]

let cached: { path: string; mtimeMs: number; html: string } | null = null

function readBuiltApp(): string | null {
  for (const candidate of BUILT_APP_CANDIDATES) {
    const path = join(process.cwd(), candidate)
    let mtimeMs: number
    try {
      mtimeMs = statSync(path).mtimeMs
    } catch {
      continue
    }
    if (cached?.path === path && cached.mtimeMs === mtimeMs) return cached.html
    const html = readFileSync(path, `utf8`)
    cached = { path, mtimeMs, html }
    return html
  }
  return null
}

const NOT_BUILT_HTML = `<!doctype html><html lang="en"><head><meta charset="UTF-8" /><meta name="exp-view" content="issues" /></head><body style="font:14px system-ui;color:#a1a1aa;padding:16px">The Exponential views are not built on this server (bun run build:mcp-apps).</body></html>`

/** The view's document: the built app with its view stamped in. */
export function mcpAppHtml(view: McpAppView, built = readBuiltApp()): string {
  return (built ?? NOT_BUILT_HTML).replace(
    /<meta name="exp-view" content="[a-z]+"\s*\/?>/,
    `<meta name="exp-view" content="${view}" />`
  )
}

/** What a view may load beyond its own document (MCP Apps `_meta.ui.csp`):
 *  pictures from the app's signed attachment URLs and Inter from Google Fonts
 *  (the web app's own font source). No connect or frame domains: data only
 *  ever arrives through the host's tools/call. */
export function mcpAppCsp(appOrigin: string) {
  return {
    resourceDomains: [
      appOrigin,
      `https://fonts.googleapis.com`,
      `https://fonts.gstatic.com`,
    ],
  }
}

/** Registers the views. Outside `registerExponentialTools` on purpose:
 *  the context budget (context-budget.ts) measures TOOL definitions against a
 *  stand-in server that only implements `registerTool`. `appOrigin` is where
 *  the signed attachment URLs point (`appBaseUrl()`, else the request's). */
export function registerExponentialApps(
  server: McpServer,
  appOrigin: string
): void {
  for (const view of Object.keys(MCP_APP_RESOURCE_URIS) as McpAppView[]) {
    const uri = MCP_APP_RESOURCE_URIS[view]
    server.registerResource(
      `exponential-${view}-view`,
      uri,
      {
        title: VIEW_TITLES[view],
        mimeType: MCP_APP_MIME_TYPE,
      },
      async () => ({
        contents: [
          {
            uri,
            mimeType: MCP_APP_MIME_TYPE,
            text: mcpAppHtml(view),
            _meta: { ui: { prefersBorder: true, csp: mcpAppCsp(appOrigin) } },
          },
        ],
      })
    )
  }
}
