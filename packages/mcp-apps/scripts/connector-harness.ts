// #VAPP-292 — a stand-in Exponential MCP server for proving the genUI view in
// a REAL MCP Apps host (the ext-apps basic-host, or Claude through a tunnel)
// without the database: the same tool names, `_meta.ui` bindings, resource
// and answers' shapes as apps/web lib/emcp, over an in-memory team running
// the Standup sample app (samples/apps/standup.app.json).
//
//   bun run build                                   # the two documents
//   bun scripts/connector-harness.ts                # http://localhost:3001/mcp
//
// Then point the basic host at it (SERVERS='["http://localhost:3001/mcp"]')
// and call exponential_apps {op: "get", slug: "standup"} or exponential_show.

import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { randomUUID } from "node:crypto"
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js"
import { z } from "zod"
import { inlineShowResult } from "../src/genui/result"

const PORT = Number(process.env.PORT ?? 3001)
const ORIGIN = process.env.ORIGIN ?? `http://localhost:${PORT}`
const WEB_PUBLIC = fileURLToPath(new URL(`../../../apps/web/public/`, import.meta.url))
const standup = JSON.parse(readFileSync(fileURLToPath(new URL(`../../../samples/apps/standup.app.json`, import.meta.url)), `utf8`))

const TEAM = `5b0c2f7e-0000-4000-8000-000000000001`
const ME = `user-danny`
const MEMBERS = [
  { id: ME, memberId: `m1`, name: `Danny`, email: `danny@exponential.at`, image: null, role: `owner`, placeholder: false },
  { id: `user-ana`, memberId: `m2`, name: `Ana`, email: `ana@example.com`, image: null, role: `member`, placeholder: false },
]
const APP = { id: `5b0c2f7e-0000-4000-8000-0000000000aa`, slug: `standup`, name: `Standup`, teamId: TEAM, icon: `users`, color: `#f59e0b`, headVersion: 1, publishedVersion: 1, grantedTools: [] as string[] }
const day = (offset: number) => new Date(Date.now() + offset * 86_400_000).toISOString().slice(0, 10)
const RECORDS: Array<Record<string, unknown>> = [
  { id: randomUUID(), kind: `update`, data: { date: day(0), yesterday: `Wired the genUI view into MCP Apps`, today: `Screenshots in the basic host`, blockers: null }, createdBy: `user-ana`, authorId: `user-ana`, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
  { id: randomUUID(), kind: `update`, data: { date: day(-1), yesterday: `Audited OAuth + DCR for claude.ai`, today: `The inline show card`, blockers: `Waiting on the tunnel` }, createdBy: ME, authorId: ME, createdAt: new Date(Date.now() - 86_400_000).toISOString(), updatedAt: new Date(Date.now() - 86_400_000).toISOString() },
]

const ok = (data: unknown) => ({ content: [{ type: `text` as const, text: JSON.stringify(data, null, 2) }] })
const err = (message: string) => ({ isError: true, content: [{ type: `text` as const, text: message }] })
const GENUI = { ui: { resourceUri: `ui://exponential/genui` } }
const noun = { op: z.string() }

function genuiHtml(): string {
  const html = readFileSync(`${WEB_PUBLIC}mcp-apps/genui.html`, `utf8`)
  // The same stamping as apps/web lib/emcp/apps.ts `mcpAppHtml`.
  return html
    .replace(/<meta name="exp-origin" content="[^"]*"\s*\/?>/, `<meta name="exp-origin" content="${ORIGIN}" />`)
    .replace(/url\((["']?)\/fonts\//g, (_, quote: string) => `url(${quote}${ORIGIN}/fonts/`)
}

function server(): McpServer {
  const s = new McpServer({ name: `exponential`, version: `0.1.0` })
  s.registerResource(`exponential-genui-view`, `ui://exponential/genui`, { title: `Exponential`, mimeType: `text/html;profile=mcp-app` }, async () => ({
    contents: [{ uri: `ui://exponential/genui`, mimeType: `text/html;profile=mcp-app`, text: genuiHtml(), _meta: { ui: { prefersBorder: true, csp: { resourceDomains: [ORIGIN, `https://fonts.googleapis.com`, `https://fonts.gstatic.com`] } } } }],
  }))
  s.registerTool(`exponential_show`, { description: `ONE card in your chat (outside a session: drawn inline).`, inputSchema: { surface: z.any().optional(), title: z.string().optional() }, _meta: GENUI }, async ({ surface, title }) =>
    surface ? ok(inlineShowResult({ kind: `surface`, surface, title: title ?? null })) : err(`Pass surface.`)
  )
  s.registerTool(`exponential_apps`, { description: `Exponential apps: list, get, records_list, records_upsert, records_delete.`, inputSchema: z.object(noun).passthrough(), _meta: GENUI }, async (args: Record<string, unknown>) => {
    switch (args.op) {
      case `list`:
        return ok([APP])
      case `get`:
        if (args.slug !== APP.slug) return err(`App not found`)
        return ok({ ...APP, url: `${ORIGIN}/t/acme/apps/standup`, version: 1, package: standup.package, linkableScreens: [], viewer: { id: ME, access: `write` } })
      case `records_list`:
        return ok({ records: RECORDS.filter((r) => !args.kind || r.kind === args.kind) })
      case `records_upsert`: {
        const now = new Date().toISOString()
        if (typeof args.id === `string`) {
          const row = RECORDS.find((r) => r.id === args.id)
          if (!row) return err(`Record not found`)
          row.data = { ...(row.data as object), ...(args.data as object) }
          row.updatedAt = now
          return ok({ ok: true, id: row.id, created: false })
        }
        const id = randomUUID()
        RECORDS.unshift({ id, kind: args.kind, data: args.data, createdBy: ME, authorId: ME, createdAt: now, updatedAt: now })
        return ok({ ok: true, id, created: true })
      }
      case `records_delete`: {
        const at = RECORDS.findIndex((r) => r.id === args.id)
        if (at >= 0) RECORDS.splice(at, 1)
        return ok({ ok: true, id: args.id, deleted: true })
      }
      default:
        return ok({ ok: true })
    }
  })
  s.registerTool(`exponential_people`, { description: `A team's members.`, inputSchema: z.object(noun).passthrough() }, async () => ok(MEMBERS))
  s.registerTool(`exponential_teams`, { description: `Teams.`, inputSchema: z.object(noun).passthrough() }, async () => ok({ id: TEAM, slug: `acme`, name: `Acme`, timeZone: `Europe/Vienna` }))
  return s
}

const CORS = { "access-control-allow-origin": `*`, "access-control-allow-headers": `*`, "access-control-allow-methods": `GET, POST, DELETE, OPTIONS`, "access-control-expose-headers": `mcp-session-id` }

Bun.serve({
  port: PORT,
  async fetch(request) {
    const url = new URL(request.url)
    if (request.method === `OPTIONS`) return new Response(null, { headers: CORS })
    if (url.pathname.startsWith(`/fonts/`)) {
      const file = Bun.file(`${WEB_PUBLIC}${url.pathname.slice(1)}`)
      return (await file.exists()) ? new Response(file, { headers: CORS }) : new Response(`not found`, { status: 404 })
    }
    if (url.pathname !== `/mcp`) return new Response(`not found`, { status: 404 })
    // Stateless, like /api/mcp: a fresh server + transport per request.
    const transport = new WebStandardStreamableHTTPServerTransport({ sessionIdGenerator: undefined, enableJsonResponse: true })
    await server().connect(transport)
    const response = await transport.handleRequest(request)
    const headers = new Headers(response.headers)
    for (const [k, v] of Object.entries(CORS)) headers.set(k, v)
    return new Response(response.body, { status: response.status, headers })
  },
})
console.log(`connector harness: ${ORIGIN}/mcp`)
