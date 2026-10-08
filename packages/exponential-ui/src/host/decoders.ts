// VAPP-91: the wire decoders every transport adapter shares (pure,
// incremental, fixture-locked in fixtures/host-transport.json): A2UI JSONL
// over a byte stream, Server-Sent Events, and the A2UI-over-MCP carrier.

import { MCP_ACTION_TOOL, MCP_MIME_TYPES, SSE_EVENTS } from "./contract"
import type { ClientMessage } from "./contract"

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

/** One message per line. `push` takes any chunking (a line may span
 *  chunks), `end` flushes a last line without a newline. */
export class JsonlDecoder {
  private buffer = ``
  private line = 0

  push(chunk: string): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    this.buffer += chunk
    let nl = this.buffer.indexOf(`\n`)
    while (nl >= 0) {
      const raw = this.buffer.slice(0, nl)
      this.buffer = this.buffer.slice(nl + 1)
      this.line += 1
      parseLine(raw, this.line, out)
      nl = this.buffer.indexOf(`\n`)
    }
    return out
  }

  end(): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    if (this.buffer.trim()) {
      this.line += 1
      parseLine(this.buffer, this.line, out)
    }
    this.buffer = ``
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
 *  dispatches, `:` comments and other fields are ignored, an `event:` name
 *  outside SSE_EVENTS drops the event. An event's data is one message or
 *  JSONL. */
export class SseDecoder {
  private buffer = ``
  private data: string[] = []
  private event = ``
  private count = 0

  push(chunk: string): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    this.buffer += chunk
    let nl = this.buffer.indexOf(`\n`)
    while (nl >= 0) {
      let line = this.buffer.slice(0, nl)
      this.buffer = this.buffer.slice(nl + 1)
      if (line.endsWith(`\r`)) line = line.slice(0, -1)
      this.line(line, out)
      nl = this.buffer.indexOf(`\n`)
    }
    return out
  }

  end(): Decoded {
    const out: Decoded = { messages: [], issues: [] }
    if (this.buffer) this.line(this.buffer, out)
    this.buffer = ``
    this.dispatch(out)
    return out
  }

  private line(line: string, out: Decoded): void {
    if (line === ``) return this.dispatch(out)
    if (line.startsWith(`:`)) return
    const colon = line.indexOf(`:`)
    const field = colon < 0 ? line : line.slice(0, colon)
    let value = colon < 0 ? `` : line.slice(colon + 1)
    if (value.startsWith(` `)) value = value.slice(1)
    if (field === `data`) this.data.push(value)
    else if (field === `event`) this.event = value
  }

  private dispatch(out: Decoded): void {
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
