#!/usr/bin/env bun
// VAPP-91 sample: a local A2UI server every sample host connects to. No
// account, no Exponential backend, no dependencies: Bun only.
//
//   bun samples/exponential-ui/server/server.ts            # http://localhost:4190
//
//   GET  /a2ui.jsonl        the surface as A2UI JSONL, then a live tick every 3 s (?once=1: the surface only, then close)
//   GET  /a2ui.sse          the same as Server-Sent Events
//   GET  /ws                the same over a WebSocket (client messages come back as frames)
//   POST /action            client messages (A2UI v0.9 action / error); `refresh` pushes new readings
//   GET  /theme.json        the sample theme (extends neutral)
//   GET  /extension.json    the sample extension catalog (Sparkline)
//   GET  /surface.jsonl     the static surface (what ?once=1 streams)

import { readFileSync } from "node:fs"
import { join } from "node:path"

const shared = join(import.meta.dir, `..`, `shared`)
const surfaceJsonl = readFileSync(join(shared, `surface.jsonl`), `utf8`)
const theme = readFileSync(join(shared, `sample.theme.json`), `utf8`)
const extension = readFileSync(join(shared, `sample.extension.json`), `utf8`)
const port = Number(process.env.PORT ?? 4190)
const SURFACE = `greenhouse`

const cors = { "access-control-allow-origin": `*`, "access-control-allow-headers": `content-type, authorization`, "access-control-allow-methods": `GET, POST, OPTIONS` }

type Sink = (line: string) => void
const sinks = new Set<Sink>()
let series = [12, 14, 13, 17, 16, 19, 18, 21]
let tick = 0

function reading(): string[] {
  tick += 1
  const next = Math.max(8, Math.min(30, series[series.length - 1]! + Math.round(Math.sin(tick) * 3)))
  series = [...series.slice(-15), next]
  const msg = (path: string, value: unknown) => JSON.stringify({ version: `v0.9`, updateDataModel: { surfaceId: SURFACE, path, value } })
  return [msg(`/series`, series), msg(`/temperature`, `${next} °C`), msg(`/status`, `3 sensors online · update ${tick}`)]
}

function broadcast(lines: string[]): void {
  for (const sink of sinks) for (const line of lines) sink(line)
}

setInterval(() => sinks.size && broadcast(reading()), 3000)

function stream(once: boolean, frame: (line: string) => string, type: string): Response {
  let sink: Sink | undefined
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      const enc = new TextEncoder()
      const write: Sink = (line) => controller.enqueue(enc.encode(frame(line)))
      for (const line of surfaceJsonl.split(`\n`)) if (line.trim()) write(line)
      if (once) return controller.close()
      sink = write
      sinks.add(write)
    },
    cancel() {
      if (sink) sinks.delete(sink)
    },
  })
  return new Response(body, { headers: { ...cors, "content-type": type, "cache-control": `no-store` } })
}

function onClientMessage(text: string): void {
  let message: { action?: { name?: string; context?: unknown }; error?: unknown }
  try {
    message = JSON.parse(text)
  } catch {
    return console.log(`[server] bad client message: ${text}`)
  }
  console.log(`[server] ←`, JSON.stringify(message))
  if (message.action?.name === `refresh`) broadcast(reading())
  if (message.action?.name === `fans`) broadcast([JSON.stringify({ version: `v0.9`, updateDataModel: { surfaceId: SURFACE, path: `/status`, value: `fans toggled` } })])
}

Bun.serve({
  port,
  async fetch(req, server) {
    const url = new URL(req.url)
    if (req.method === `OPTIONS`) return new Response(null, { headers: cors })
    const once = url.searchParams.get(`once`) === `1`
    switch (url.pathname) {
      case `/a2ui.jsonl`:
        return stream(once, (line) => `${line}\n`, `application/jsonl`)
      case `/a2ui.sse`:
        return stream(once, (line) => `data: ${line}\n\n`, `text/event-stream`)
      case `/ws`:
        return server.upgrade(req) ? undefined : new Response(`expected a websocket`, { status: 400 })
      case `/action`:
        onClientMessage(await req.text())
        return new Response(null, { status: 204, headers: cors })
      case `/theme.json`:
        return new Response(theme, { headers: { ...cors, "content-type": `application/json` } })
      case `/extension.json`:
        return new Response(extension, { headers: { ...cors, "content-type": `application/json` } })
      case `/surface.jsonl`:
        return new Response(surfaceJsonl, { headers: { ...cors, "content-type": `application/jsonl` } })
    }
    return new Response(`not found`, { status: 404, headers: cors })
  },
  websocket: {
    open(ws) {
      for (const line of surfaceJsonl.split(`\n`)) if (line.trim()) ws.send(line)
      const sink: Sink = (line) => ws.send(line)
      ;(ws as unknown as { sink: Sink }).sink = sink
      sinks.add(sink)
    },
    message(_ws, data) {
      onClientMessage(typeof data === `string` ? data : new TextDecoder().decode(data))
    },
    close(ws) {
      sinks.delete((ws as unknown as { sink: Sink }).sink)
    },
  },
})

console.log(`A2UI sample server on http://localhost:${port} (/a2ui.jsonl, /a2ui.sse, /ws, /action, /theme.json, /extension.json)`)
