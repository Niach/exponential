// VAPP-91: the wire decoders every transport adapter shares (pure,
// incremental, fixture-locked in fixtures/host-transport.json): A2UI JSONL
// over a byte stream, Server-Sent Events, and the A2UI-over-MCP carrier;
// #AXC-12: the Express inference format beside them (ExpressDecoder, the
// `a2ui-express` SSE event, `{express}` WebSocket frames, the Express MCP
// resource), over the compiler a host loads (`@exponential_app/eui/express`).

import { EXPRESS_DEFAULT_SURFACE_ID, EXPRESS_MIME_TYPE, EXPRESS_SSE_EVENT, EXPRESS_STRUCTURED_KEY, EXPRESS_WS_KEY, MCP_ACTION_TOOL, MCP_MIME_TYPES, SSE_EVENTS } from "./contract"
import type { ClientMessage, InferenceFormat } from "./contract"
import { LIMIT_ISSUES, MAX_MESSAGE_BYTES } from "../limits"

export interface DecodeIssue {
  /** 1-based line (JSONL) or event (SSE) number. */
  at: number
  message: string
}

export interface Decoded {
  messages: unknown[]
  issues: DecodeIssue[]
  /** #AXC-12: the prose around an Express response's `<a2ui>` blocks, in
   *  order, as it streamed in (append the pieces; the host shows it through
   *  `onText`, the SDK draws none). Absent for JSON. */
  text?: string[]
  /** #VAPP-300: the surfaces whose Express block is still OPEN after this
   *  read (the ids the open block has written to, in order): the host
   *  reduces them `streaming` (a child not streamed yet renders as
   *  nothing). A STATE, not a delta: absent = none open (JSON never is);
   *  the host replaces its open set with every batch's. */
  open?: string[]
}

/** Appends `from` to `out` (text only when there is some); `open` is the
 *  later batch's (the state after both). */
export function mergeDecoded(out: Decoded, from: Decoded): Decoded {
  out.messages.push(...from.messages)
  out.issues.push(...from.issues)
  if (from.text?.length) (out.text ??= []).push(...from.text)
  if (from.open?.length) out.open = from.open
  else delete out.open
  return out
}

/** #VAPP-300: the surface a message targets (router `surfaceIdOf`: the first
 *  string `surfaceId` among its object values), else `""`. */
function messageSurfaceId(message: unknown): string {
  if (!message || typeof message !== `object`) return ``
  for (const v of Object.values(message)) if (v && typeof v === `object` && typeof (v as { surfaceId?: unknown }).surfaceId === `string`) return (v as { surfaceId: string }).surfaceId
  return ``
}

function parseLine(line: string, at: number, out: Decoded): void {
  const text = line.trim()
  if (!text) return
  try {
    out.messages.push(JSON.parse(text))
  } catch {
    out.issues.push({ at, message: `line ${at} is not JSON` })
  }
}

/** UTF-8 bytes of a string (a surrogate pair = 4; chunk-safe per unit). */
function utf8Bytes(text: string): number {
  let n = 0
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i)
    n += c < 0x80 ? 1 : c < 0x800 ? 2 : c >= 0xd800 && c <= 0xdfff ? 2 : 3
  }
  return n
}

/** VAPP-103: the line splitter both stream decoders share (the Rust core's
 *  `Lines`): each push scans its chunk once, the unterminated tail is kept
 *  as pieces (never rescanned), and a line past `maxMessageBytes` is
 *  `null` (its bytes are dropped up to the next newline, never buffered). */
class Lines {
  private parts: string[] = []
  private bytes = 0
  /** Inside an oversized line: drop until its newline. */
  private skipping = false

  push(chunk: string): (string | null)[] {
    const out: (string | null)[] = []
    let start = 0
    if (this.skipping) {
      const i = chunk.indexOf(`\n`)
      if (i < 0) return out
      this.skipping = false
      start = i + 1
    }
    let nl = chunk.indexOf(`\n`, start)
    while (nl >= 0) {
      const piece = chunk.slice(start, nl)
      const oversized = this.bytes + utf8Bytes(piece) > MAX_MESSAGE_BYTES
      out.push(oversized ? null : this.parts.length ? this.parts.join(``) + piece : piece)
      this.parts = []
      this.bytes = 0
      start = nl + 1
      nl = chunk.indexOf(`\n`, start)
    }
    if (start < chunk.length) {
      const tail = chunk.slice(start)
      this.parts.push(tail)
      this.bytes += utf8Bytes(tail)
    }
    if (this.bytes > MAX_MESSAGE_BYTES) {
      out.push(null)
      this.parts = []
      this.bytes = 0
      this.skipping = true
    }
    return out
  }

  /** The unterminated tail (null inside an oversized line). */
  end(): string | null {
    const skipping = this.skipping
    const tail = this.parts.join(``)
    this.parts = []
    this.bytes = 0
    this.skipping = false
    return skipping ? null : tail
  }
}

const oversized = (at: number, what: `line` | `event`): DecodeIssue => ({ at, message: `${what} ${at}: ${LIMIT_ISSUES.messageBytes}` })

/** One message per line. `push` takes any chunking (a line may span
 *  chunks), `end` flushes a last line without a newline. A line past
 *  `maxMessageBytes` is an issue (`line N: message larger than … bytes`),
 *  never buffered. */
export class JsonlDecoder {
  private lines = new Lines()
  private line = 0

  push(chunk: string): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    for (const raw of this.lines.push(chunk)) {
      this.line += 1
      if (raw === null) out.issues.push(oversized(this.line, `line`))
      else parseLine(raw, this.line, out)
    }
    return out
  }

  end(): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    const raw = this.lines.end()
    if (raw !== null && raw.trim()) {
      this.line += 1
      parseLine(raw, this.line, out)
    }
    return out
  }
}

/** A whole JSONL document (or one JSON value / a JSON array of messages). */
export function decodeJsonl(text: string): Decoded {
  const trimmed = text.trim()
  if (trimmed.startsWith(`[`)) {
    try {
      const value = JSON.parse(trimmed)
      if (Array.isArray(value)) return { messages: value, issues: [] }
    } catch {
      // fall through: JSONL whose first line is an array is still JSONL
    }
  }
  const d = new JsonlDecoder()
  const a = d.push(text)
  const b = d.end()
  return { messages: [...a.messages, ...b.messages], issues: [...a.issues, ...b.issues] }
}

/** Server-Sent Events: `data:` lines join with `\n` per event, a blank line
 *  dispatches, `:` comments and other fields are ignored (`retry:` is
 *  kept as `retryMs`), an `event:` name
 *  outside SSE_EVENTS drops the event. An event's data is one message or
 *  JSONL. */
export class SseDecoder {
  /** The last `retry:` field (ms): the server resumes the stream after it
   *  ends (a transport reconnects on a clean end only then). */
  retryMs?: number
  private lines = new Lines()
  private data: string[] = []
  /** The event's data bytes so far (past `maxMessageBytes` it is dropped
   *  and dispatches as an issue). */
  private dataBytes = 0
  private oversized = false
  private event = ``
  private count = 0
  /** #AXC-12: the stream's ONE Express response (its `a2ui-express`
   *  events), opened on the first such event. */
  private express?: ExpressDecoder

  constructor(private options: ExpressOptions = {}) {}

  push(chunk: string): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    for (const raw of this.lines.push(chunk)) {
      if (raw === null) this.dropData()
      else this.line(raw.endsWith(`\r`) ? raw.slice(0, -1) : raw, out)
    }
    // #VAPP-300: the Express response's open block holds across JSON events.
    const open = this.express?.open
    if (open?.length) out.open = open
    return out
  }

  end(): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    const raw = this.lines.end()
    if (raw === null) this.dropData()
    else if (raw) this.line(raw, out)
    this.dispatch(out)
    if (this.express) this.expressOut(this.express.end(), out)
    return out
  }

  /** An Express event's output: its issues carry the event number. */
  private expressOut(d: Decoded, out: Decoded): void {
    out.messages.push(...d.messages)
    if (d.text?.length) (out.text ??= []).push(...d.text)
    for (const issue of d.issues) out.issues.push({ at: this.count, message: `event ${this.count}: ${issue.message}` })
  }

  /** The event is past `maxMessageBytes`: forget its data (it dispatches
   *  as an issue). */
  private dropData(): void {
    this.data = []
    this.dataBytes = 0
    this.oversized = true
  }

  private line(line: string, out: Decoded): void {
    if (line === ``) return this.dispatch(out)
    if (line.startsWith(`:`)) return
    const colon = line.indexOf(`:`)
    const field = colon < 0 ? line : line.slice(0, colon)
    let value = colon < 0 ? `` : line.slice(colon + 1)
    if (value.startsWith(` `)) value = value.slice(1)
    if (field === `data`) {
      if (this.oversized) return
      this.dataBytes += utf8Bytes(value) + 1
      if (this.dataBytes > MAX_MESSAGE_BYTES) return this.dropData()
      this.data.push(value)
    } else if (field === `event`) this.event = value
    else if (field === `retry` && /^\d+$/.test(value)) this.retryMs = Number(value)
  }

  private dispatch(out: Decoded): void {
    this.dataBytes = 0
    if (this.oversized) {
      this.oversized = false
      this.count += 1
      this.data = []
      this.event = ``
      out.issues.push(oversized(this.count, `event`))
      return
    }
    if (!this.data.length) {
      this.event = ``
      return
    }
    this.count += 1
    const name = this.event || `message`
    const data = this.data.join(`\n`)
    this.data = []
    this.event = ``
    if (name === EXPRESS_SSE_EVENT) {
      if (!this.express) {
        if (!expressEngine) return void out.issues.push({ at: this.count, message: `event ${this.count}: ${EXPRESS_NOT_LOADED}` })
        this.express = new ExpressDecoder(this.options)
      }
      return this.expressOut(this.express.push(data), out)
    }
    if (!SSE_EVENTS.includes(name)) return
    const d = decodeJsonl(data)
    out.messages.push(...d.messages)
    if (d.issues.length) out.issues.push({ at: this.count, message: `event ${this.count} is not JSON` })
  }
}

/** The A2UI-over-MCP carrier: the messages inside an MCP tool result
 *  (`content[]` resources with an A2UI mime type, or
 *  `structuredContent.a2ui`). */
export function messagesFromMcpResult(result: unknown, options: ExpressOptions = {}): Decoded {
  const out: Decoded = { messages: [], issues: [] }
  if (!result || typeof result !== `object`) return out
  const r = result as { content?: unknown; structuredContent?: unknown }
  const structured = (r.structuredContent as { a2ui?: unknown } | undefined)?.a2ui
  if (Array.isArray(structured)) out.messages.push(...structured)
  // #AXC-12: a bare Express body beside (or instead of) the JSON.
  const structuredExpress = (r.structuredContent as Record<string, unknown> | undefined)?.[EXPRESS_STRUCTURED_KEY]
  if (typeof structuredExpress === `string`) {
    const d = decodeExpressBody(structuredExpress, options)
    mergeDecoded(out, { ...d, issues: d.issues.map((issue) => ({ at: 0, message: `structuredContent.${EXPRESS_STRUCTURED_KEY}: ${issue.message}` })) })
  }
  if (Array.isArray(r.content)) {
    r.content.forEach((item, i) => {
      const resource = (item as { type?: string; resource?: { mimeType?: string; text?: string } })?.resource
      if ((item as { type?: string })?.type !== `resource` || !resource || typeof resource.text !== `string`) return
      const express = resource.mimeType === EXPRESS_MIME_TYPE
      if (!express && !MCP_MIME_TYPES.includes(resource.mimeType ?? ``)) return
      const d = express ? decodeExpressBody(resource.text, options) : decodeJsonl(resource.text)
      mergeDecoded(out, { ...d, issues: d.issues.map((issue) => ({ at: i + 1, message: `content[${i}]: ${issue.message}` })) })
    })
  }
  return out
}

/** #AXC-12: whether an MCP result carries Express (a host loads the
 *  compiler before decoding it). */
export function mcpResultHasExpress(result: unknown): boolean {
  if (!result || typeof result !== `object`) return false
  const r = result as { content?: unknown; structuredContent?: unknown }
  if (typeof (r.structuredContent as Record<string, unknown> | undefined)?.[EXPRESS_STRUCTURED_KEY] === `string`) return true
  return Array.isArray(r.content) && r.content.some((item) => (item as { resource?: { mimeType?: string } })?.resource?.mimeType === EXPRESS_MIME_TYPE)
}

/** A renderer message as the MCP tools/call that carries it back (round 9
 *  §10: `metadata` beside `message` when it has any). */
export function mcpActionCall(message: ClientMessage, tool: string = MCP_ACTION_TOOL, metadata?: Record<string, unknown>): { method: `tools/call`; params: { name: string; arguments: { message: ClientMessage; metadata?: Record<string, unknown> } } } {
  return { method: `tools/call`, params: { name: tool, arguments: metadata ? { message, metadata } : { message } } }
}

// ---------------------------------------------------------------------------
// #AXC-12: Express.

/** One part of a streamed Express response, as the compiler yields it. */
export type ExpressPart = { text: string } | { a2ui: unknown[] } | { error: { category: string; message: string } }

/** A streaming Express read (the wasm `StreamCompiler`). Throws an error
 *  with `category` and `partial` (the parts made ready before it). */
export interface ExpressStream {
  push(chunk: string): ExpressPart[]
  finish(): ExpressPart[]
  readonly inBlock: boolean
  free?(): void
}

/** The compiler a host registers (`@exponential_app/eui/express`
 *  `loadExpress()`): opens a stream over the core catalog. The core bundle
 *  never ships it. */
export interface ExpressEngine {
  stream(options: { surfaceId: string; wrapped: boolean }): ExpressStream
}

let expressEngine: ExpressEngine | undefined

/** Registers the Express compiler (`@exponential_app/eui/express` does it
 *  once loaded). */
export function registerExpressEngine(engine: ExpressEngine | undefined): void {
  expressEngine = engine
}

/** Whether Express can be decoded now (the compiler is loaded). */
export function expressLoaded(): boolean {
  return expressEngine !== undefined
}

export const EXPRESS_NOT_LOADED = `Express is not loaded (@exponential_app/eui/express loadExpress())`

export interface ExpressOptions {
  /** The surface of statements before any `surface("…")` line (default
   *  `EXPRESS_DEFAULT_SURFACE_ID`). */
  surfaceId?: string
}

/** The compiler's error categories as the issue text names them (the Rust
 *  `a2ui_express::Error` display). */
function categoryText(category: string): string {
  return category === `ParseError` ? `parse error` : category === `ValidationError` ? `validation error` : category.replace(/Error$/, ``).toLowerCase() + ` error`
}

/** The Express wire decoder, beside JsonlDecoder and SseDecoder (the Rust
 *  core's `host::ExpressDecoder`): model output in (any chunking), A2UI
 *  messages out as each block's statements become ready, so a surface
 *  renders while the agent is still writing it.
 *
 *  - `wrapped` (default true): prose with `<a2ui>…</a2ui>` blocks (a
 *    model's whole response); the prose comes back in `Decoded.text`
 *    (append the pieces; a run after a block starts with a `"\n\n"`
 *    piece). Unwrapped: the whole stream is ONE Express body without tags.
 *  - A block that does not compile is an issue (`at` = the chunk it failed
 *    in, 1-based), never fatal: the stream reads on past it.
 *
 *  Needs the compiler loaded (`loadExpress()`), else the constructor throws. */
export class ExpressDecoder {
  private stream: ExpressStream
  private chunks = 0
  /** The stream failed (an unclosed block at the end, a broken body): it
   *  takes no more input. */
  private failed = false
  /** Prose has come out (the next run after a block starts a paragraph). */
  private texted = false
  /** A block passed since the last prose. */
  private blocked = false
  /** #VAPP-300: the surfaces the OPEN block has written to, in order. */
  private blockSurfaces: string[] = []

  constructor(options: ExpressOptions & { wrapped?: boolean } = {}) {
    if (!expressEngine) throw new Error(EXPRESS_NOT_LOADED)
    this.stream = expressEngine.stream({ surfaceId: options.surfaceId ?? EXPRESS_DEFAULT_SURFACE_ID, wrapped: options.wrapped ?? true })
  }

  push(chunk: string): Decoded {
    if (this.failed) return { messages: [], issues: [] }
    this.chunks += 1
    const out = this.collect(() => this.stream.push(chunk))
    if (!this.failed) this.blocked ||= this.stream.inBlock
    if (this.failed || !this.stream.inBlock) this.blockSurfaces = []
    if (this.blockSurfaces.length) out.open = [...this.blockSurfaces]
    return out
  }

  end(): Decoded {
    if (this.failed) return { messages: [], issues: [] }
    const out = this.collect(() => this.stream.finish())
    this.failed = true
    this.blockSurfaces = []
    this.stream.free?.()
    return out
  }

  /** #VAPP-300: the surfaces whose block is still open (what the last read
   *  reported as `open`). */
  get open(): string[] {
    return [...this.blockSurfaces]
  }

  private collect(read: () => ExpressPart[]): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    const at = Math.max(this.chunks, 1)
    let parts: ExpressPart[]
    try {
      parts = read()
    } catch (e) {
      this.failed = true
      const err = e as { category?: string; message?: string; partial?: ExpressPart[] }
      parts = [...(err.partial ?? []), { error: { category: err.category ?? `ParseError`, message: err.message ?? String(e) } }]
    }
    for (const part of parts) {
      if (`text` in part) {
        // The stream trims each run's edges: a run after a block is a new paragraph.
        if (this.blocked && this.texted) (out.text ??= []).push(`\n\n`)
        // Prose: the block before it closed.
        this.blockSurfaces = []
        this.blocked = false
        this.texted = true
        ;(out.text ??= []).push(part.text)
      } else if (`a2ui` in part) {
        this.blocked = true
        out.messages.push(...part.a2ui)
        for (const m of part.a2ui) {
          const id = messageSurfaceId(m)
          if (id && !this.blockSurfaces.includes(id)) this.blockSurfaces.push(id)
        }
      } else {
        this.blocked = true
        // A rejected block is over.
        this.blockSurfaces = []
        out.issues.push({ at, message: `${categoryText(part.error.category)}: ${part.error.message}` })
      }
    }
    return out
  }
}

/** One bare Express body (an MCP resource, `structuredContent`), decoded
 *  whole; without the compiler loaded, one issue. */
export function decodeExpressBody(text: string, options: ExpressOptions = {}): Decoded {
  if (!expressEngine) return { messages: [], issues: [{ at: 1, message: EXPRESS_NOT_LOADED }] }
  const d = new ExpressDecoder({ ...options, wrapped: false })
  return mergeDecoded(d.push(text), d.end())
}

/** #AXC-12: the inference format of an HTTP body by its Content-Type
 *  (parameters and case ignored): `express` for `application/a2ui+express`,
 *  else `json`. */
export function inferenceFormatOf(contentType: string | null | undefined): InferenceFormat {
  const type = (contentType ?? ``).split(`;`)[0]!.trim().toLowerCase()
  return type === EXPRESS_MIME_TYPE ? `express` : `json`
}

/** The Express chunk a WebSocket frame carries (`{"express": "…"}`: a JSON
 *  object whose ONE key is `express`, a string), else null. */
export function expressFrameChunk(frame: string): string | null {
  const text = frame.trim()
  if (!text.startsWith(`{`) || !text.includes(EXPRESS_WS_KEY)) return null
  try {
    const value = JSON.parse(text) as Record<string, unknown>
    const keys = value && typeof value === `object` && !Array.isArray(value) ? Object.keys(value) : []
    return keys.length === 1 && keys[0] === EXPRESS_WS_KEY && typeof value[EXPRESS_WS_KEY] === `string` ? (value[EXPRESS_WS_KEY] as string) : null
  } catch {
    return null
  }
}

/** WebSocket frames in order (the WebSocket transport's decoding): an
 *  `{express}` frame is the next chunk of the connection's ONE Express
 *  response (issues: `frame N: …`); any other frame is one message or
 *  JSONL (`frame N is not JSON`). `end()` at close flushes the Express
 *  response. */
export class FrameDecoder {
  private frames = 0
  private express?: ExpressDecoder

  constructor(private options: ExpressOptions = {}) {}

  push(frame: string): Decoded {
    this.frames += 1
    const n = this.frames
    const chunk = expressFrameChunk(frame)
    if (chunk === null) {
      const d = decodeJsonl(frame)
      const out: Decoded = { messages: d.messages, issues: d.issues.length ? [{ at: n, message: `frame ${n} is not JSON` }] : [] }
      // #VAPP-300: the Express response's open block holds across JSON frames.
      const open = this.express?.open
      if (open?.length) out.open = open
      return out
    }
    if (!this.express) {
      if (!expressEngine) return { messages: [], issues: [{ at: n, message: `frame ${n}: ${EXPRESS_NOT_LOADED}` }] }
      this.express = new ExpressDecoder(this.options)
    }
    return this.relabel(this.express.push(chunk), n)
  }

  end(): Decoded {
    return this.express ? this.relabel(this.express.end(), this.frames) : { messages: [], issues: [] }
  }

  private relabel(d: Decoded, n: number): Decoded {
    return { ...d, issues: d.issues.map((issue) => ({ at: n, message: `frame ${n}: ${issue.message}` })) }
  }
}
