// VAPP-91: the wire decoders every transport adapter shares (pure,
// incremental, fixture-locked in fixtures/host-transport.json): A2UI JSONL
// over a byte stream, Server-Sent Events, and the A2UI-over-MCP carrier.

import { MCP_ACTION_TOOL, MCP_MIME_TYPES, SSE_EVENTS } from "./contract"
import type { ClientMessage } from "./contract"
import { LIMIT_ISSUES, MAX_MESSAGE_BYTES } from "../limits"

export interface DecodeIssue {
  /** 1-based line (JSONL) or event (SSE) number. */
  at: number
  message: string
}

export interface Decoded {
  messages: unknown[]
  issues: DecodeIssue[]
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

  push(chunk: string): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    for (const raw of this.lines.push(chunk)) {
      if (raw === null) this.dropData()
      else this.line(raw.endsWith(`\r`) ? raw.slice(0, -1) : raw, out)
    }
    return out
  }

  end(): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    const raw = this.lines.end()
    if (raw === null) this.dropData()
    else if (raw) this.line(raw, out)
    this.dispatch(out)
    return out
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
    if (!SSE_EVENTS.includes(name)) return
    const d = decodeJsonl(data)
    out.messages.push(...d.messages)
    if (d.issues.length) out.issues.push({ at: this.count, message: `event ${this.count} is not JSON` })
  }
}

/** The A2UI-over-MCP carrier: the messages inside an MCP tool result
 *  (`content[]` resources with an A2UI mime type, or
 *  `structuredContent.a2ui`). */
export function messagesFromMcpResult(result: unknown): Decoded {
  const out: Decoded = { messages: [], issues: [] }
  if (!result || typeof result !== `object`) return out
  const r = result as { content?: unknown; structuredContent?: unknown }
  const structured = (r.structuredContent as { a2ui?: unknown } | undefined)?.a2ui
  if (Array.isArray(structured)) out.messages.push(...structured)
  if (Array.isArray(r.content)) {
    r.content.forEach((item, i) => {
      const resource = (item as { type?: string; resource?: { mimeType?: string; text?: string } })?.resource
      if ((item as { type?: string })?.type !== `resource` || !resource || !MCP_MIME_TYPES.includes(resource.mimeType ?? ``)) return
      if (typeof resource.text !== `string`) return
      const d = decodeJsonl(resource.text)
      out.messages.push(...d.messages)
      for (const issue of d.issues) out.issues.push({ at: i + 1, message: `content[${i}]: ${issue.message}` })
    })
  }
  return out
}

/** A client message as the MCP tools/call that carries it back. */
export function mcpActionCall(message: ClientMessage, tool: string = MCP_ACTION_TOOL): { method: `tools/call`; params: { name: string; arguments: { message: ClientMessage } } } {
  return { method: `tools/call`, params: { name: tool, arguments: { message } } }
}
