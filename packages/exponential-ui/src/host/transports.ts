// VAPP-91: the transport adapters the TS SDK ships. Each speaks A2UI
// messages in and client messages out through the shared decoders; none
// knows anything about the host behind it. Web platform APIs only (fetch,
// WebSocket, TextDecoder): browsers, Bun, Node ≥ 22.
//
// #AXC-12: each takes the Express inference format too (catalog/host.json
// `inference`): an HTTP body by its Content-Type, `a2ui-express` SSE events,
// `{express}` WebSocket frames, Express MCP resources. The compiler is
// loaded on demand (the lazy `@exponential_app/eui/express` chunk) the
// moment a stream turns out to carry Express, or up front with
// `format: "express"`; the prose goes to the host's `onText`.

import { EXPRESS_MIME_TYPE, EXPRESS_SSE_EVENT, EXPRESS_WS_KEY } from "./contract"
import type { ClientMessage, InferenceFormat } from "./contract"
import { ExpressDecoder, FrameDecoder, JsonlDecoder, SseDecoder, decodeJsonl, expressLoaded, inferenceFormatOf, mcpActionCall, mcpResultHasExpress, messagesFromMcpResult } from "./decoders"
import type { Decoded } from "./decoders"
import type { MessageMetadata, Transport, TransportStatus } from "./runtime"

/** Round 9 §1.3: `{message, metadata}` only when metadata is present. */
const wire = (message: ClientMessage, metadata?: MessageMetadata): unknown => (metadata ? { message, metadata } : message)

type Receive = (message: unknown) => void
type Status = (status: TransportStatus, detail?: string) => void
type Text = (text: string) => void

/** What every transport takes for Express (#AXC-12). */
export interface ExpressTransportOptions {
  /** `express`: the agent writes Express (asked for in `accept`, the
   *  compiler loaded before the first byte). Default `json`: Express is
   *  still read when the wire says so (Content-Type, SSE event, frame). */
  format?: InferenceFormat
  /** The surface Express statements before any `surface("…")` target
   *  (default `main`). */
  surfaceId?: string
  /** Where the host serves the Express .wasm (default: beside the module). */
  expressWasmUrl?: string | URL
}

/** Loads the Express compiler (the lazy chunk) once. */
async function ensureExpress(options: ExpressTransportOptions): Promise<void> {
  if (expressLoaded()) return
  const { loadExpress } = await import("../express/index")
  await loadExpress(options.expressWasmUrl)
}

/** Delivers one decoded batch: messages to the host, prose to onText. */
function deliver(d: Decoded, receive: Receive, text?: Text): void {
  d.messages.forEach(receive)
  if (text) d.text?.forEach(text)
}

/** In-memory: `feed` messages in, read what the host sent from `sent`. */
export class MemoryTransport implements Transport {
  readonly sent: ClientMessage[] = []
  private receiveFn?: Receive
  private pending: unknown[] = []
  onSend?: (message: ClientMessage) => void

  start(receive: Receive, status: Status): void {
    this.receiveFn = receive
    status(`open`)
    const queued = this.pending
    this.pending = []
    queued.forEach(receive)
  }

  feed(...messages: unknown[]): void {
    if (!this.receiveFn) this.pending.push(...messages)
    else messages.forEach(this.receiveFn)
  }

  /** JSONL text, as a stream server would send it. */
  feedJsonl(text: string): void {
    this.feed(...decodeJsonl(text).messages)
  }

  send(message: ClientMessage): void {
    this.sent.push(message)
    this.onSend?.(message)
  }

  close(): void {
    this.receiveFn = undefined
  }
}

export interface HttpTransportOptions extends ExpressTransportOptions {
  /** The stream to read (GET). */
  url: string
  /** Where client messages go (POST, JSON body); default `url`. */
  postUrl?: string
  headers?: Record<string, string>
  /** Reconnect after a drop (an error or an aborted stream), ms (0 =
   *  never). Default 2000. */
  reconnectMs?: number
  /** Reconnect after a CLEAN end of the stream too (the server resumes it).
   *  Default false: a stream that ends is done (status `closed`). An SSE
   *  stream that sent a `retry:` field signals this itself. */
  resumable?: boolean
  fetch?: typeof fetch
}

abstract class StreamTransport implements Transport {
  private controller?: AbortController
  /** Bumped by every start/close: a loop whose generation is stale exits
   *  (an old loop asleep in its reconnect wait never runs beside a new
   *  one). */
  private generation = 0

  constructor(protected options: HttpTransportOptions) {}

  /** The body's decoder; `express` = its Content-Type is Express (the
   *  compiler is loaded by then). */
  protected abstract decoder(express: boolean): { push(chunk: string): Decoded; end(): Decoded; retryMs?: number }
  protected abstract accept(): string
  /** Whether this text may start Express the decoder will need the
   *  compiler for (SSE: an `a2ui-express` event name). */
  protected mayNeedExpress(_text: string): boolean {
    return false
  }

  start(receive: Receive, status: Status, text?: Text): void {
    this.controller?.abort()
    const gen = ++this.generation
    void this.run(gen, receive, status, text)
  }

  private async run(gen: number, receive: Receive, status: Status, onText?: Text): Promise<void> {
    const f = this.options.fetch ?? fetch
    const live = () => gen === this.generation
    while (live()) {
      const controller = new AbortController()
      this.controller = controller
      status(`connecting`)
      let wait = this.options.reconnectMs ?? 2000
      try {
        if (this.options.format === `express`) await ensureExpress(this.options)
        const accept = this.options.format === `express` ? `${EXPRESS_MIME_TYPE}, ${this.accept()}` : this.accept()
        const res = await f(this.options.url, { headers: { accept, ...this.options.headers }, signal: controller.signal })
        if (!live()) return
        if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`)
        const express = inferenceFormatOf(res.headers.get(`content-type`)) === `express`
        if (express) await ensureExpress(this.options)
        if (!live()) return
        status(`open`)
        const reader = res.body.getReader()
        const text = new TextDecoder()
        const dec = this.decoder(express)
        // The tail of the previous chunk: an event name split across chunks.
        let tail = ``
        for (;;) {
          const { done, value } = await reader.read()
          if (!live()) return
          if (done) break
          const chunk = text.decode(value, { stream: true })
          if (!expressLoaded() && this.mayNeedExpress(tail + chunk)) {
            await ensureExpress(this.options)
            if (!live()) return
          }
          tail = chunk.slice(-32)
          deliver(dec.push(chunk), receive, onText)
        }
        deliver(dec.end(), receive, onText)
        status(`closed`)
        // A clean end: done, unless the server said it resumes the stream.
        const resumable = this.options.resumable === true || dec.retryMs !== undefined
        if (!resumable) {
          if (live()) this.generation += 1
          return
        }
        if (dec.retryMs !== undefined && this.options.reconnectMs === undefined) wait = dec.retryMs
      } catch (e) {
        if (!live()) return
        status(`error`, e instanceof Error ? e.message : String(e))
      }
      if (!wait || !live()) return
      await new Promise((r) => setTimeout(r, wait))
    }
  }

  async send(message: ClientMessage, metadata?: MessageMetadata): Promise<void> {
    const f = this.options.fetch ?? fetch
    await f(this.options.postUrl ?? this.options.url, { method: `POST`, headers: { "content-type": `application/json`, ...this.options.headers }, body: JSON.stringify(wire(message, metadata)) })
  }

  close(): void {
    this.generation += 1
    this.controller?.abort()
  }
}

/** A2UI JSONL over a streamed HTTP response (`application/jsonl`); an
 *  `application/a2ui+express` response is ONE Express response (prose +
 *  `<a2ui>` blocks). */
export class JsonlStreamTransport extends StreamTransport {
  protected decoder(express: boolean) {
    return express ? new ExpressDecoder({ surfaceId: this.options.surfaceId }) : new JsonlDecoder()
  }
  protected accept() {
    return `application/jsonl, application/x-ndjson`
  }
}

/** Server-Sent Events read through fetch (headers allowed, unlike
 *  EventSource); `a2ui-express` events are the chunks of the stream's ONE
 *  Express response. */
export class SseTransport extends StreamTransport {
  protected decoder() {
    return new SseDecoder({ surfaceId: this.options.surfaceId })
  }
  protected accept() {
    return `text/event-stream`
  }
  protected mayNeedExpress(text: string) {
    return text.includes(EXPRESS_SSE_EVENT)
  }
}

/** One A2UI message (or JSONL) per frame; client messages go back as
 *  frames. */
export class WebSocketTransport implements Transport {
  private socket?: WebSocket
  /** Bumped by every start/close: a stale socket's reconnect timer is a
   *  no-op. */
  private generation = 0

  constructor(private options: ExpressTransportOptions & { url: string; protocols?: string | string[]; reconnectMs?: number; WebSocket?: typeof WebSocket }) {}

  start(receive: Receive, status: Status, text?: Text): void {
    const gen = ++this.generation
    const live = () => gen === this.generation
    const WS = this.options.WebSocket ?? WebSocket
    status(`connecting`)
    const socket = new WS(this.options.url, this.options.protocols)
    this.socket = socket
    // One decoder per connection: its `{express}` frames are ONE response.
    const frames = new FrameDecoder({ surfaceId: this.options.surfaceId })
    // Frames decode in order, also while the compiler loads.
    let queue: Promise<void> = this.options.format === `express` ? ensureExpress(this.options).catch(() => {}) : Promise.resolve()
    const decode = (data: string) => {
      queue = queue.then(async () => {
        if (!expressLoaded() && data.includes(EXPRESS_WS_KEY)) await ensureExpress(this.options).catch(() => {})
        if (live()) deliver(frames.push(data), receive, text)
      })
    }
    socket.onopen = () => live() && status(`open`)
    socket.onmessage = (ev) => {
      if (live() && typeof ev.data === `string`) decode(ev.data)
    }
    socket.onerror = () => live() && status(`error`, `websocket error`)
    socket.onclose = () => {
      if (!live()) return
      queue = queue.then(() => {
        if (live()) deliver(frames.end(), receive, text)
      })
      status(`closed`)
      const wait = this.options.reconnectMs ?? 2000
      if (wait) setTimeout(() => live() && this.start(receive, status, text), wait)
    }
  }

  send(message: ClientMessage, metadata?: MessageMetadata): void {
    if (this.socket?.readyState === 1) this.socket.send(JSON.stringify(wire(message, metadata)))
  }

  close(): void {
    this.generation += 1
    this.socket?.close()
  }
}

export interface McpTransportOptions extends ExpressTransportOptions {
  /** The MCP server's streamable-HTTP endpoint. */
  url: string
  /** The tool whose result carries the surface (called once at start). */
  tool: string
  arguments?: Record<string, unknown>
  /** The tool client messages go to; default `a2ui_event`. */
  actionTool?: string
  headers?: Record<string, string>
  fetch?: typeof fetch
}

/** A2UI over MCP: calls `tool`, delivers the A2UI resources in its result,
 *  and sends every client message as a tools/call to `actionTool` (its
 *  result may carry more messages). JSON-RPC over plain POST; an SSE
 *  response body is read through the SSE decoder. */
export class McpTransport implements Transport {
  private id = 0
  private receiveFn?: Receive
  private textFn?: Text
  private session?: string

  constructor(private options: McpTransportOptions) {}

  /** A tool result's messages (Express resources: the compiler loaded first). */
  private async deliverResult(result: unknown): Promise<void> {
    if (mcpResultHasExpress(result)) await ensureExpress(this.options)
    if (this.receiveFn) deliver(messagesFromMcpResult(result, { surfaceId: this.options.surfaceId }), this.receiveFn, this.textFn)
  }

  start(receive: Receive, status: Status, text?: Text): void {
    this.receiveFn = receive
    this.textFn = text
    status(`connecting`)
    void (async () => {
      try {
        await this.rpc(`initialize`, { protocolVersion: `2025-06-18`, capabilities: {}, clientInfo: { name: `exponential-ui`, version: `0.1.0` } })
        const result = await this.rpc(`tools/call`, { name: this.options.tool, arguments: this.options.arguments ?? {} })
        status(`open`)
        await this.deliverResult(result)
      } catch (e) {
        status(`error`, e instanceof Error ? e.message : String(e))
      }
    })()
  }

  async send(message: ClientMessage, metadata?: MessageMetadata): Promise<void> {
    const call = mcpActionCall(message, this.options.actionTool, metadata)
    const result = await this.rpc(call.method, call.params)
    await this.deliverResult(result)
  }

  close(): void {
    this.receiveFn = undefined
    this.textFn = undefined
  }

  private async rpc(method: string, params: unknown): Promise<unknown> {
    const f = this.options.fetch ?? fetch
    const id = ++this.id
    const res = await f(this.options.url, {
      method: `POST`,
      headers: { "content-type": `application/json`, accept: `application/json, text/event-stream`, ...(this.session ? { "mcp-session-id": this.session } : {}), ...this.options.headers },
      body: JSON.stringify({ jsonrpc: `2.0`, id, method, params }),
    })
    const session = res.headers.get(`mcp-session-id`)
    if (session) this.session = session
    if (!res.ok) throw new Error(`MCP HTTP ${res.status}`)
    const type = res.headers.get(`content-type`) ?? ``
    const body = await res.text()
    const replies = type.includes(`text/event-stream`) ? (() => {
      const d = new SseDecoder()
      return [...d.push(body).messages, ...d.end().messages]
    })() : [JSON.parse(body)]
    const reply = replies.find((r) => (r as { id?: number })?.id === id) as { result?: unknown; error?: { message?: string } } | undefined
    if (!reply) throw new Error(`MCP: no reply to ${method}`)
    if (reply.error) throw new Error(`MCP: ${reply.error.message ?? `error`}`)
    return reply.result
  }
}
