// An MCP server (JSON-RPC over HTTP, Node 24: `node server.ts`) whose one
// tool lets a model show UI. The tool's description IS the catalog prompt;
// every answer is validated before it reaches a client, and a valid surface
// goes back as A2UI over MCP: an `application/json+a2ui` resource.
import { createServer } from "node:http"
import { CORE_CATALOG_ID, ICON_NAMES, catalogPrompt, coreSchema, reduceSurface } from "@exponential-at/ui"
import type { FlatComponent } from "@exponential-at/ui"

const tool = {
  name: `show_ui`,
  description: `Show the user a UI. ${catalogPrompt()}`, // the catalog ({terse: true} = smaller)
  inputSchema: { type: `object`, properties: { components: { type: `array`, items: { type: `object` } } }, required: [`components`] },
}

function showUi(components: FlatComponent[]) {
  // The reducer never throws: unknown components become placeholders, bad props issues.
  const { issues } = reduceSurface(components, { catalogId: CORE_CATALOG_ID })
  if (issues.length) return { isError: true, content: [{ type: `text`, text: issues.map((i) => `${i.id}: ${i.message}`).join(`\n`) }] } // the model retries
  const messages = [
    { version: `v0.9`, createSurface: { surfaceId: `main`, catalogId: CORE_CATALOG_ID } },
    { version: `v0.9`, updateComponents: { surfaceId: `main`, components } },
  ]
  const text = messages.map((m) => JSON.stringify(m)).join(`\n`) // JSONL
  return { content: [{ type: `resource`, resource: { uri: `a2ui://surface/main`, mimeType: `application/json+a2ui`, text } }] }
}

function handle(method: string, params: any): unknown {
  if (method === `initialize`) return { protocolVersion: `2025-06-18`, capabilities: { tools: {} }, serverInfo: { name: `ui-agent`, version: `1.0.0` } }
  if (method === `tools/list`) return { tools: [tool] }
  if (method === `tools/call` && params?.name === `show_ui`) return showUi(params.arguments?.components ?? [])
  if (method === `tools/call` && params?.name === `a2ui_event`) return { content: [{ type: `text`, text: `ok` }] } // actions come back here
  return {}
}

createServer(async (req, res) => {
  // The JSON Schema of the catalog, for structured outputs or a validator.
  if (req.method === `GET` && req.url === `/catalog.schema.json`) return void res.end(JSON.stringify(coreSchema(ICON_NAMES)))
  let body = ``
  for await (const chunk of req) body += chunk
  const { id, method, params } = JSON.parse(body || `{}`)
  if (id === undefined) return void res.writeHead(202).end() // a notification
  res.setHeader(`content-type`, `application/json`)
  res.end(JSON.stringify({ jsonrpc: `2.0`, id, result: handle(method, params) }))
}).listen(Number(process.env.PORT ?? 4300), () => console.log(`MCP on http://localhost:${process.env.PORT ?? 4300}`))
