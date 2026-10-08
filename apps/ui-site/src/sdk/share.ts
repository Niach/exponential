/* The playground's shareable state: JSON → deflate-raw (CompressionStream)
   → base64url, carried in the URL hash (`#s=…`), so a link holds the whole
   surface and never reaches a server. */

const toBase64Url = (bytes: Uint8Array) => {
  let bin = ``
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return btoa(bin).replace(/\+/g, `-`).replace(/\//g, `_`).replace(/=+$/, ``)
}

const fromBase64Url = (text: string) => {
  const b64 = text.replace(/-/g, `+`).replace(/_/g, `/`)
  const bin = atob(b64 + `=`.repeat((4 - (b64.length % 4)) % 4))
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

async function pipe(bytes: Uint8Array, stream: CompressionStream | DecompressionStream): Promise<Uint8Array> {
  const body = new Blob([bytes as BlobPart]).stream().pipeThrough(stream)
  return new Uint8Array(await new Response(body).arrayBuffer())
}

export async function encodeShare(state: unknown): Promise<string> {
  const json = new TextEncoder().encode(JSON.stringify(state))
  return toBase64Url(await pipe(json, new CompressionStream(`deflate-raw`)))
}

export async function decodeShare<T>(token: string): Promise<T | null> {
  try {
    const bytes = await pipe(fromBase64Url(token), new DecompressionStream(`deflate-raw`))
    return JSON.parse(new TextDecoder().decode(bytes)) as T
  } catch {
    return null
  }
}

export const SHARE_KEY = `s`

export function shareTokenFromHash(hash: string): string | null {
  const params = new URLSearchParams(hash.replace(/^#/, ``))
  return params.get(SHARE_KEY)
}
