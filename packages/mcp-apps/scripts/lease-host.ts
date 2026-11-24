// #NEW-24 — a stand-in MCP Apps host for the two states the reference host
// (modelcontextprotocol/ext-apps `examples/basic-host`) cannot produce: a host
// that passes NO host context, and a host whose lease on the view expired
// (OpenClaw after ten minutes: the document is refetched, no result is
// replayed, every tools/call is refused). One page, a real browser:
//
//   bun scripts/connector-harness.ts                 # the server, :3001
//   bun scripts/lease-host.ts                        # http://localhost:3002
//   /?tool=exponential_issues&args={"op":"list"}&mode=<mode>&theme=<light|dark>
//
//   mode=bare      initialize answers `{}`; input and result are forwarded
//   mode=live      a full host context (theme, tool-call id), live calls
//   mode=expired   the context names the call, nothing else ever arrives and
//                  tools/call is refused
//   mode=rebuild   live first; then the SAME call is mounted again expired
//                  (a reconstruction within the page: the last screen, read-only)
//   mode=refuse    the result arrives, but every tools/call is refused
//
// `window.hostLog` lists every message the view sent.

const PORT = Number(process.env.PORT ?? 3002)
const SERVER = process.env.SERVER ?? `http://localhost:3001/mcp`

const PAGE = `<!doctype html><html><head><meta charset="UTF-8"><title>lease host</title>
<style>html,body{margin:0}body{padding:24px;font:14px system-ui}body.dark{background:#1a1a1a;color:#f3f4f6}body.light{background:#fff;color:#1f2937}iframe{display:block;width:100%;border:0;height:0}p{margin:0 0 12px;opacity:.6}</style></head>
<body><p id="caption"></p><div id="slot"></div><script type="module">
const query = new URLSearchParams(location.search)
const tool = query.get("tool") ?? "exponential_issues"
const args = JSON.parse(query.get("args") ?? "{}")
const mode = query.get("mode") ?? "live"
const theme = query.get("theme") === "light" ? "light" : "dark"
const server = ${JSON.stringify(SERVER)}
document.body.className = theme
document.getElementById("caption").textContent = "lease-host · " + mode + " · " + tool + " " + JSON.stringify(args)
window.hostLog = []
let rpcId = 0
const rpc = async (method, params) => {
  const response = await fetch(server, { method: "POST", headers: { "content-type": "application/json", accept: "application/json, text/event-stream" }, body: JSON.stringify({ jsonrpc: "2.0", id: ++rpcId, method, params }) })
  const body = await response.json()
  if (body.error) throw new Error(body.error.message)
  return body.result
}
const tools = (await rpc("tools/list", {})).tools
const uri = tools.find((entry) => entry.name === tool)?._meta?.ui?.resourceUri
const html = (await rpc("resources/read", { uri })).contents[0].text

function mount({ live, context }) {
  const frame = document.createElement("iframe")
  frame.setAttribute("sandbox", "allow-scripts allow-same-origin allow-forms")
  frame.srcdoc = html
  document.getElementById("slot").replaceChildren(frame)
  const post = (message) => frame.contentWindow.postMessage({ jsonrpc: "2.0", ...message }, "*")
  const onMessage = async (event) => {
    if (event.source !== frame.contentWindow) return
    const message = event.data
    window.hostLog.push(message.method ?? "response")
    if (message.method === "ui/initialize") return post({ id: message.id, result: { protocolVersion: "2026-01-26", hostInfo: { name: "lease-host", version: "1" }, hostCapabilities: {}, hostContext: context } })
    if (message.method === "ui/notifications/initialized") {
      if (!live.result) return
      post({ method: "ui/notifications/tool-input", params: { arguments: args } })
      return post({ method: "ui/notifications/tool-result", params: await rpc("tools/call", { name: tool, arguments: args }) })
    }
    if (message.method === "ui/notifications/size-changed") return void (frame.style.height = message.params.height + "px")
    if (message.method === "tools/call") {
      if (!live.calls) return post({ id: message.id, error: { code: -32000, message: "This view's lease expired; relaunch it to call tools." } })
      return post({ id: message.id, result: await rpc("tools/call", message.params) })
    }
    if (message.id !== undefined) post({ id: message.id, error: { code: -32601, message: "Method not found" } })
  }
  window.addEventListener("message", onMessage)
  return () => window.removeEventListener("message", onMessage)
}
const full = { theme, displayMode: "inline", availableDisplayModes: ["inline"], toolInfo: { id: "call-1", tool: { name: tool } } }
if (mode === "bare") mount({ live: { result: true, calls: true }, context: {} })
else if (mode === "expired") mount({ live: { result: false, calls: false }, context: full })
else if (mode === "refuse") mount({ live: { result: true, calls: false }, context: full })
else if (mode === "rebuild") {
  const stop = mount({ live: { result: true, calls: true }, context: full })
  window.rebuild = () => { stop(); mount({ live: { result: false, calls: false }, context: full }) }
} else mount({ live: { result: true, calls: true }, context: full })
window.hostReady = true
</script></body></html>`

Bun.serve({
  port: PORT,
  fetch: () => new Response(PAGE, { headers: { "content-type": `text/html; charset=utf-8` } }),
})
console.log(`lease host: http://localhost:${PORT}  →  ${SERVER}`)
