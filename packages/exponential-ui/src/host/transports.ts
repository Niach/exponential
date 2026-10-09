// VAPP-91: the transport adapters the TS SDK ships. Each speaks A2UI
// messages in and client messages out through the shared decoders; none
// knows anything about the host behind it. Web platform APIs only (fetch,
// WebSocket, TextDecoder): browsers, Bun, Node ≥ 22.

import type { ClientMessage } from "./contract"
import { JsonlDecoder, SseDecoder, decodeJsonl, mcpActionCall, messagesFromMcpResult } from "./decoders"
import type { Decoded } from "./decoders"
import type { Transport, TransportStatus } from "./runtime"

type Receive = (message: unknown) => void
type Status = (status: TransportStatus, detail?: string) => void

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

export interface HttpTransportOptions {
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

  protected abstract decoder(): { push(chunk: string): Decoded; end(): Decoded; retryMs?: number }
  protected abstract accept(): string

  start(receive: Receive, status: Status): void {
    this.controller?.abort()
    const gen = ++this.generation
    void this.run(gen, receive, status)
  }

  private async run(gen: number, receive: Receive, status: Status): Promise<void> {
    const f = this.options.fetch ?? fetch
    const live = () => gen === this.generation
    while (live()) {
      const controller = new AbortController()
      this.controller = controller
      status(`connecting`)
      let wait = this.options.reconnectMs ?? 2000
      try {
        const res = await f(this.options.url, { headers: { accept: this.accept(), ...this.options.headers }, signal: controller.signal })
        if (!live()) return
        if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`)
        status(`open`)
        const reader = res.body.getReader()
        const text = new TextDecoder()
        const dec = this.decoder()
        for (;;) {
          const { done, value } = await reader.read()
          if (!live()) return
          if (done) break
          dec.push(text.decode(value, { stream: true })).messages.forEach(receive)
        }
        dec.end().messages.forEach(receive)
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

  async send(message: ClientMessage): Promise<void> {
    const f = this.options.fetch ?? fetch
    await f(this.options.postUrl ?? this.options.url, { method: `POST`, headers: { "content-type": `application/json`, ...this.options.headers }, body: JSON.stringify(message) })
  }

  close(): void {
    this.generation += 1
    this.controller?.abort()
  }
}

/** A2UI JSONL over a streamed HTTP response (`application/jsonl`). */
export class JsonlStreamTransport extends StreamTransport {
  protected decoder() {
    return new JsonlDecoder()
  }
  protected accept() {
    return `application/jsonl, application/x-ndjson`
  }
}

/** Server-Sent Events read through fetch (headers allowed, unlike
 *  EventSource). */
export class SseTransport extends StreamTransport {
  protected decoder() {
    return new SseDecoder()
  }
  protected accept() {
    return `text/event-stream`
  }
}

/** One A2UI message (or JSONL) per frame; client messages go back as
 *  frames. */
export class WebSocketTransport implements Transport {
  private socket?: WebSocket
  /** Bumped by every start/close: a stale socket's reconnect timer is a
   *  no-op. */
  private generation = 0

  constructor(private options: { url: string; protocols?: string | string[]; reconnectMs?: number; WebSocket?: typeof WebSocket }) {}

  start(receive: Receive, status: Status): void {
    const gen = ++this.generation
    const live = () => gen === this.generation
    const WS = this.options.WebSocket ?? WebSocket
    status(`connecting`)
    const socket = new WS(this.options.url, this.options.protocols)
    this.socket = socket
    socket.onopen = () => live() && status(`open`)
    socket.onmessage = (ev) => {
      if (live() && typeof ev.data === `string`) decodeJsonl(ev.data).messages.forEach(receive)
    }
    socket.onerror = () => live() && status(`error`, `websocket error`)
    socket.onclose = () => {
      if (!live()) return
      status(`closed`)
      const wait = this.options.reconnectMs ?? 2000
      if (wait) setTimeout(() => live() && this.start(receive, status), wait)
    }
  }

  send(message: ClientMessage): void {
    if (this.socket?.readyState === 1) this.socket.send(JSON.stringify(message))
  }

  close(): void {
    this.generation += 1
    this.socket?.close()
  }
}

export interface McpTransportOptions {
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
  private session?: string

  constructor(private options: McpTransportOptions) {}

  start(receive: Receive, status: Status): void {
    this.receiveFn = receive
    status(`connecting`)
    void (async () => {
      try {
        await this.rpc(`initialize`, { protocolVersion: `2025-06-18`, capabilities: {}, clientInfo: { name: `exponential-ui`, version: `0.1.0` } })
        const result = await this.rpc(`tools/call`, { name: this.options.tool, arguments: this.options.arguments ?? {} })
        status(`open`)
        messagesFromMcpResult(result).messages.forEach(receive)
      } catch (e) {
        status(`error`, e instanceof Error ? e.message : String(e))
      }
    })()
  }

  async send(message: ClientMessage): Promise<void> {
    const call = mcpActionCall(message, this.options.actionTool)
    const result = await this.rpc(call.method, call.params)
    if (this.receiveFn) messagesFromMcpResult(result).messages.forEach(this.receiveFn)
  }

  close(): void {
    this.receiveFn = undefined
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
