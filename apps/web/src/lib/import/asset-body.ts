// EXP-630: reads an asset download's body with a running byte count and a
// hard cap — `content-length` is only the fast reject (it may be absent or
// wrong, and a chunked body carries none), so `arrayBuffer()` on an
// unchecked response could buffer anything the file store sends. The same
// streaming pattern as routes/api/attachment-uploads/$token.ts.

export interface BoundedBodyResponse {
  headers: { get(name: string): string | null }
  body?: ReadableStream<Uint8Array> | null
}

// Null = the body is larger than `maxBytes` (declared or observed); the
// stream is cancelled so the connection does not keep pulling.
export async function readBodyBounded(
  response: BoundedBodyResponse,
  maxBytes: number
): Promise<Uint8Array | null> {
  const declared = Number(response.headers.get(`content-length`))
  if (Number.isFinite(declared) && declared > maxBytes) return null
  const reader = response.body?.getReader()
  if (!reader) return new Uint8Array(0)
  const chunks: Uint8Array[] = []
  let total = 0
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    total += value.byteLength
    if (total > maxBytes) {
      await reader.cancel().catch(() => undefined)
      return null
    }
    chunks.push(value)
  }
  const bytes = new Uint8Array(total)
  let offset = 0
  for (const chunk of chunks) {
    bytes.set(chunk, offset)
    offset += chunk.byteLength
  }
  return bytes
}
