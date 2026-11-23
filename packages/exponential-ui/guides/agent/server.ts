// An MCP server (JSON-RPC over HTTP, Node 24: `node server.ts`) whose tools
// let a model show UI. Each tool's description IS a catalog prompt; every
// answer is validated before it reaches a client:
//
//   show_ui          A2UI JSON: an `application/a2ui+json` resource.
//   show_ui_express  Express (the compact notation): an
//                    `application/a2ui+express` resource. Called without
//                    `express`, it answers the recorded demo's block.
//
// `GET /express` replays a recorded, streamed model response (prose around
// an `<a2ui>` block: fixtures/express-demo.json) as SSE `a2ui-express`
// events, at the delays it streamed with. `?pause=<n>` holds the stream
// open after n chunks (a mid-stream screenshot).
import { readFileSync } from "node:fs"
import { createServer } from "node:http"
import type { ServerResponse } from "node:http"
import { A2UI_VERSION, CORE_CATALOG_ID, ICON_NAMES, reduceSurface } from "@exponential_app/eui"
import type { FlatComponent } from "@exponential_app/eui"
import { compileExpress, expressPrompt, initExpress } from "@exponential_app/eui/express"
import { catalogPrompt } from "@exponential_app/eui/prompt"
import { coreSchema } from "@exponential_app/eui/schema"
import demo from "@exponential_app/eui/fixtures/express-demo.json" with { type: "json" }

// The Express compiler (WASM, shipped beside the module), instantiated once.
initExpress(readFileSync(new URL(`a2ui_express_bg.wasm`, import.meta.resolve(`@exponential_app/eui/express`))))

const tools = [
  {
    name: `show_ui`,
    description: `Show the user a UI. ${catalogPrompt()}`, // the catalog ({terse: true} = smaller)
    inputSchema: { type: `object`, properties: { components: { type: `array`, items: { type: `object` } } }, required: [`components`] },
  },
  {
    name: `show_ui_express`,
    description: `Show the user a UI, written in Express. ${expressPrompt()}`, // the compact prompt
    inputSchema: { type: `object`, properties: { express: { type: `string` } } },
  },
]

function showUi(components: FlatComponent[]) {
  // The reducer never throws: unknown components become placeholders, bad props issues.
  const { issues } = reduceSurface(components, { catalogId: CORE_CATALOG_ID })
  if (issues.length) return { isError: true, content: [{ type: `text`, text: issues.map((i) => `${i.id}: ${i.message}`).join(`\n`) }] } // the model retries
  const messages = [
    { version: A2UI_VERSION, createSurface: { surfaceId: `main`, catalogId: CORE_CATALOG_ID } },
    { version: A2UI_VERSION, updateComponents: { surfaceId: `main`, components } },
  ]
  const text = messages.map((m) => JSON.stringify(m)).join(`\n`) // JSONL
  return { content: [{ type: `resource`, resource: { uri: `a2ui://surface/main`, mimeType: `application/a2ui+json`, text } }] }
}

// The recorded response, whole, and its one block's body (no tags).
const response = demo.chunks.map((c) => c.text).join(``)
const demoBlock = /<a2ui>\n?([\s\S]*?)<\/a2ui>/.exec(response)![1]!

function showUiExpress(express: string = demoBlock) {
  // Compiling IS the validation: a component or prop the catalog lacks throws.
  try {
    compileExpress(express)
  } catch (e) {
    return { isError: true, content: [{ type: `text`, text: e instanceof Error ? e.message : String(e) }] } // the model retries
  }
  return { content: [{ type: `resource`, resource: { uri: `a2ui://surface/main`, mimeType: `application/a2ui+express`, text: express } }] }
}

function handle(method: string, params: any): unknown {
  if (method === `initialize`) return { protocolVersion: `2025-06-18`, capabilities: { tools: {} }, serverInfo: { name: `ui-agent`, version: `1.0.0` } }
  if (method === `tools/list`) return { tools }
  if (method === `tools/call` && params?.name === `show_ui`) return showUi(params.arguments?.components ?? [])
  if (method === `tools/call` && params?.name === `show_ui_express`) return showUiExpress(params.arguments?.express)
  if (method === `tools/call` && params?.name === `a2ui_event`) return { content: [{ type: `text`, text: `ok` }] } // actions come back here
  return {}
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

/** One SSE event per chunk: `event: a2ui-express`, the chunk as its data
 *  (a line each; the client joins them with "\n"). */
async function replay(res: ServerResponse, pause: number) {
  res.writeHead(200, { "content-type": `text/event-stream`, "cache-control": `no-store`, "access-control-allow-origin": `*` })
  for (const [i, chunk] of demo.chunks.entries()) {
    if (i === pause) return // held open: the client stays mid-stream
    await sleep(chunk.delayMs)
    if (res.destroyed) return
    res.write(`event: a2ui-express\n${chunk.text.split(`\n`).map((l) => `data: ${l}`).join(`\n`)}\n\n`)
  }
  res.end()
}

createServer(async (req, res) => {
  const url = new URL(req.url ?? `/`, `http://localhost`)
  // The JSON Schema of the catalog, for structured outputs or a validator.
  if (req.method === `GET` && url.pathname === `/catalog.schema.json`) return void res.end(JSON.stringify(coreSchema(ICON_NAMES)))
  if (req.method === `GET` && url.pathname === `/express`) return void replay(res, Number(url.searchParams.get(`pause`) ?? -1))
  if (req.method === `OPTIONS`) return void res.writeHead(204, { "access-control-allow-origin": `*`, "access-control-allow-headers": `content-type` }).end()
  let body = ``
  for await (const chunk of req) body += chunk
  const { id, method, params } = JSON.parse(body || `{}`)
  res.setHeader(`access-control-allow-origin`, `*`)
  if (id === undefined) return void res.writeHead(202).end() // a notification (or an action POST)
  res.setHeader(`content-type`, `application/json`)
  res.end(JSON.stringify({ jsonrpc: `2.0`, id, result: handle(method, params) }))
}).listen(Number(process.env.PORT ?? 4300), () => console.log(`MCP on http://localhost:${process.env.PORT ?? 4300}`))
