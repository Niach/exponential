// MCP call results and strict base64 for the web capabilities (#VAPP-265).
// No Buffer, no DOM: the same code runs in browsers and under bun:test.

/** An MCP call result as a native capability answers it. */
export type McpResult =
  | { content: [{ type: `text`; text: string }]; structuredContent: Record<string, unknown> }
  | { content: [{ type: `text`; text: string }]; isError: true }

export const ok = (structuredContent: Record<string, unknown>): McpResult => ({
  content: [{ type: `text`, text: JSON.stringify(structuredContent) }],
  structuredContent,
})

export const fail = (reason: string): McpResult => ({
  content: [{ type: `text`, text: reason }],
  isError: true,
})

export const notFound = (): McpResult => fail(`notFound`)
export const unavailable = (): McpResult => fail(`unavailable`)
export const failed = (message: string): McpResult => fail(`failed: ${message}`)

/** The message of anything thrown, short and on one line. */
export const errorMessage = (error: unknown): string => {
  const text = error instanceof Error ? error.message || error.name : String(error)
  const line = text.replace(/\s+/g, ` `).trim()
  return line.length > 200 ? `${line.slice(0, 199)}…` : line || `error`
}

/** A DOMException-style abort (a cancelled picker or share sheet). */
export const isAbort = (error: unknown): boolean =>
  typeof error === `object` && error !== null && (error as { name?: unknown }).name === `AbortError`

const ALPHABET = `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/`
const DECODE = new Int16Array(128).fill(-1)
for (let i = 0; i < ALPHABET.length; i++) DECODE[ALPHABET.charCodeAt(i)] = i

/** Standard padded base64. */
export const encodeBase64 = (bytes: Uint8Array): string => {
  let out = ``
  let i = 0
  for (; i + 2 < bytes.length; i += 3) {
    const n = (bytes[i]! << 16) | (bytes[i + 1]! << 8) | bytes[i + 2]!
    out += ALPHABET[n >> 18]! + ALPHABET[(n >> 12) & 63]! + ALPHABET[(n >> 6) & 63]! + ALPHABET[n & 63]!
  }
  const rest = bytes.length - i
  if (rest === 1) {
    const n = bytes[i]! << 16
    out += `${ALPHABET[n >> 18]}${ALPHABET[(n >> 12) & 63]}==`
  } else if (rest === 2) {
    const n = (bytes[i]! << 16) | (bytes[i + 1]! << 8)
    out += `${ALPHABET[n >> 18]}${ALPHABET[(n >> 12) & 63]}${ALPHABET[(n >> 6) & 63]}=`
  }
  return out
}

/** Strict standard padded base64 (no whitespace, no URL alphabet, zero pad bits); null when malformed. */
export const decodeBase64 = (text: string): Uint8Array | null => {
  if (text.length % 4 !== 0) return null
  const pad = text.endsWith(`==`) ? 2 : text.endsWith(`=`) ? 1 : 0
  const out = new Uint8Array((text.length / 4) * 3 - pad)
  let o = 0
  for (let i = 0; i < text.length; i += 4) {
    const last = i + 4 === text.length
    const v = [0, 0, 0, 0]
    for (let j = 0; j < 4; j++) {
      const c = text.charCodeAt(i + j)
      if (c === 61 /* = */ && last && j >= 4 - pad) continue
      const d = c < 128 ? DECODE[c]! : -1
      if (d < 0) return null
      v[j] = d
    }
    const n = (v[0]! << 18) | (v[1]! << 12) | (v[2]! << 6) | v[3]!
    out[o++] = n >> 16
    if (o < out.length) out[o++] = (n >> 8) & 255
    if (o < out.length) out[o++] = n & 255
    if (last && pad === 2 && (v[1]! & 15) !== 0) return null
    if (last && pad === 1 && (v[2]! & 3) !== 0) return null
  }
  return out
}
