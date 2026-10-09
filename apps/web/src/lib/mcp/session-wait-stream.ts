// FEED-83: `exponential_sessions_get({waitForIdle})` may hold a request for
// minutes; routes/api/mcp.ts answers it over SSE through these.

const SSE_KEEPALIVE_MS = 15_000

/** FEED-83: a `tools/call` of `exponential_sessions_get` with `waitForIdle`. */
export function isSessionWait(body: unknown): boolean {
  const messages = Array.isArray(body) ? body : [body]
  return messages.some((message) => {
    if (!message || typeof message !== `object`) return false
    const { method, params } = message as {
      method?: unknown
      params?: { name?: unknown; arguments?: { waitForIdle?: unknown } }
    }
    return (
      method === `tools/call` &&
      params?.name === `exponential_sessions_get` &&
      params.arguments?.waitForIdle === true
    )
  })
}

/** Interleaves SSE comment lines between the transport's whole-event chunks
 *  so neither Bun.serve's 255s idleTimeout nor a proxy drops a long wait,
 *  and closes the MCP server once the stream ends or the client leaves. */
export function withKeepalive(
  body: ReadableStream<Uint8Array>,
  onDone: () => Promise<void>,
  intervalMs = SSE_KEEPALIVE_MS
): ReadableStream<Uint8Array> {
  const reader = body.getReader()
  const ping = new TextEncoder().encode(`: keepalive\n\n`)
  let timer: ReturnType<typeof setInterval> | undefined
  let done = false
  const finish = async () => {
    if (done) return
    done = true
    clearInterval(timer)
    await onDone()
  }
  return new ReadableStream<Uint8Array>({
    start(controller) {
      timer = setInterval(() => {
        try {
          controller.enqueue(ping)
        } catch {
          void finish()
        }
      }, intervalMs)
    },
    async pull(controller) {
      try {
        const { value, done: ended } = await reader.read()
        if (ended) {
          controller.close()
          await finish()
          return
        }
        controller.enqueue(value)
      } catch (e) {
        controller.error(e)
        await finish()
      }
    },
    async cancel(reason) {
      await reader.cancel(reason).catch(() => {})
      await finish()
    },
  })
}
