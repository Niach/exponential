// The ONE JSON-RPC answer reader for a Streamable HTTP MCP response (plain
// JSON or an SSE stream): the message whose `id` is the request's, reading an
// SSE body only as far as it (a server may keep the stream open after
// answering, or send notifications first). Browser-safe: the server's MCP
// client (`mcp-client.ts`) and the web Exponential UI host share it.

export type JsonRpcMessage = {
  id?: number | string | null
  result?: Record<string, unknown>
  error?: { code?: number; message?: string; data?: unknown }
}

/** The message with `id` in a JSON body (one message or a batch). */
export function parseJsonRpcMessage(text: string, id: number): JsonRpcMessage | null {
  try {
    const parsed: unknown = JSON.parse(text)
    const messages = Array.isArray(parsed) ? parsed : [parsed]
    for (const message of messages) {
      if (message && typeof message === `object` && (message as JsonRpcMessage).id === id) {
        return message as JsonRpcMessage
      }
    }
  } catch {
    // not JSON (a comment / keep-alive): keep reading
  }
  return null
}

/** Pull the JSON-RPC message with `id` out of an SSE body. */
export async function readSseJsonRpc(response: Response, id: number): Promise<JsonRpcMessage | null> {
  const reader = response.body?.getReader()
  if (!reader) return null
  const decoder = new TextDecoder()
  let buffer = ``
  let data: string[] = []
  try {
    for (;;) {
      const { value, done } = await reader.read()
      buffer += decoder.decode(value ?? new Uint8Array(), { stream: !done })
      let newline: number
      while ((newline = buffer.indexOf(`\n`)) >= 0) {
        const line = buffer.slice(0, newline).replace(/\r$/, ``)
        buffer = buffer.slice(newline + 1)
        if (line.startsWith(`data:`)) {
          data.push(line.slice(5).replace(/^ /, ``))
        } else if (line === `` && data.length > 0) {
          const message = parseJsonRpcMessage(data.join(`\n`), id)
          data = []
          if (message) return message
        }
      }
      if (done) {
        if (buffer.startsWith(`data:`)) data.push(buffer.slice(5).replace(/^ /, ``))
        return data.length > 0 ? parseJsonRpcMessage(data.join(`\n`), id) : null
      }
    }
  } finally {
    reader.cancel().catch(() => undefined)
  }
}

/** The answer to request `id`, by the response's content type. */
export async function readJsonRpcAnswer(response: Response, id: number): Promise<JsonRpcMessage | null> {
  const type = response.headers.get(`content-type`) ?? ``
  if (type.includes(`text/event-stream`)) return readSseJsonRpc(response, id)
  return parseJsonRpcMessage(await response.text(), id)
}
