// VAPP-161: the PACKAGE FILE's author + signature (docs/package-format.md
// "The package file"): Ed25519 (RFC 8032) over a context line + the RFC 8785
// canonical JSON of the whole file without `signature`. WebCrypto, so the
// same code runs in Bun, Node and every current browser; the Rust core
// mirrors `canonicalJson` + `packageFileSignedBytes` (`host/signature.rs`)
// and leaves the curve to the host's own Ed25519. fixtures/host-package.json
// `canonical` + `signatures` replay it (signed with the RFC 8032 §7.1 TEST 1
// keypair, never a real key).

import type { PackageIssue } from "./package"

/** The context the signed bytes start with. */
export const PACKAGE_FILE_SIGNATURE_CONTEXT = `exponential-ui/package-file/v1\n`
export const PACKAGE_FILE_SIGNATURE_ALG = `ed25519`
export const AUTHOR_KEY_PREFIX = `ed25519:`
export const AUTHOR_NAME_MAX = 80

export interface PackageFileAuthor {
  name: string
  /** `ed25519:` + the 32-byte public key, unpadded base64url. */
  key: string
}

export interface PackageFileSignature {
  alg: `ed25519`
  /** The 64-byte signature, unpadded base64url. */
  value: string
}

export type PackageFileVerification =
  | { status: `unsigned` }
  | { status: `valid`; author: PackageFileAuthor }
  | { status: `invalid`; issues: PackageIssue[] }

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)

/** RFC 8785 (JCS): no whitespace, keys sorted by UTF-16 code units, strings
 *  and numbers as ECMAScript's JSON.stringify writes them (`1`, `1e+21`, `-0`
 *  = `0`); an undefined member is skipped. Throws on a non-finite number or a
 *  value JSON has no form for. */
export function canonicalJson(value: unknown): string {
  if (value === null || typeof value === `boolean` || typeof value === `string`) return JSON.stringify(value)
  if (typeof value === `number`) {
    if (!Number.isFinite(value)) throw new Error(`canonicalJson: ${value} has no JSON form`)
    return JSON.stringify(value)
  }
  if (Array.isArray(value)) return `[${value.map((v) => (v === undefined ? `null` : canonicalJson(v))).join(`,`)}]`
  if (isObject(value)) {
    const keys = Object.keys(value)
      .filter((k) => value[k] !== undefined)
      .sort()
    return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(value[k])}`).join(`,`)}}`
  }
  throw new Error(`canonicalJson: a ${typeof value} has no JSON form`)
}

/** What the signature covers: the context line + the canonical JSON of the
 *  file without `signature`, as UTF-8. */
export function packageFileSignedBytes(file: Record<string, unknown>): Uint8Array<ArrayBuffer> {
  const { signature: _signature, ...rest } = file
  return new TextEncoder().encode(PACKAGE_FILE_SIGNATURE_CONTEXT + canonicalJson(rest))
}

export function base64UrlEncode(bytes: Uint8Array): string {
  let s = ``
  for (const b of bytes) s += String.fromCharCode(b)
  return btoa(s).replace(/\+/g, `-`).replace(/\//g, `_`).replace(/=+$/, ``)
}

/** Strict: the url-safe alphabet, no padding; null otherwise. */
export function base64UrlDecode(text: string): Uint8Array<ArrayBuffer> | null {
  if (!/^[A-Za-z0-9_-]*$/.test(text) || text.length % 4 === 1) return null
  const bin = atob(text.replace(/-/g, `+`).replace(/_/g, `/`) + `===`.slice((text.length + 3) % 4))
  return Uint8Array.from(bin, (c) => c.charCodeAt(0))
}

/** The 32-byte public key of `author.key`; null when malformed. */
export function authorPublicKey(key: unknown): Uint8Array<ArrayBuffer> | null {
  if (typeof key !== `string` || !key.startsWith(AUTHOR_KEY_PREFIX)) return null
  const bytes = base64UrlDecode(key.slice(AUTHOR_KEY_PREFIX.length))
  return bytes?.length === 32 ? bytes : null
}

const PKCS8_ED25519_PREFIX = Uint8Array.from([0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20])

/** The file signed by the holder of the 32-byte Ed25519 `seed`, as
 *  `author` `name`: every key but `signature` kept, then `author` and
 *  `signature` (replaced when present). */
export async function signPackageFile<T extends Record<string, unknown>>(
  file: T,
  signer: { name: string; seed: Uint8Array }
): Promise<Omit<T, `author` | `signature`> & { author: PackageFileAuthor; signature: PackageFileSignature }> {
  if (signer.seed.length !== 32) throw new Error(`signPackageFile: the seed is 32 bytes`)
  const pkcs8 = new Uint8Array(48)
  pkcs8.set(PKCS8_ED25519_PREFIX)
  pkcs8.set(signer.seed, 16)
  const privateKey = await crypto.subtle.importKey(`pkcs8`, pkcs8, { name: `Ed25519` }, true, [`sign`])
  const jwk = await crypto.subtle.exportKey(`jwk`, privateKey)
  const { author: _author, signature: _signature, ...rest } = file
  const author: PackageFileAuthor = { name: signer.name, key: `${AUTHOR_KEY_PREFIX}${jwk.x}` }
  const unsigned = { ...rest, author }
  const sig = new Uint8Array(await crypto.subtle.sign({ name: `Ed25519` }, privateKey, packageFileSignedBytes(unsigned)))
  return { ...unsigned, signature: { alg: `ed25519`, value: base64UrlEncode(sig) } } as never
}

/** Is the file signed, and by whom: `unsigned` without `signature` (an
 *  `author` alone is an unverified claim), `valid` with the author, or
 *  `invalid` with every problem as a JSON pointer. */
export async function verifyPackageFile(file: unknown): Promise<PackageFileVerification> {
  if (!isObject(file)) return { status: `invalid`, issues: [{ path: ``, message: `a package file is an object` }] }
  if (file.signature === undefined) return { status: `unsigned` }
  const issues: PackageIssue[] = []
  const sig = file.signature
  let value: Uint8Array<ArrayBuffer> | null = null
  if (!isObject(sig)) issues.push({ path: `/signature`, message: `signature is {alg: "ed25519", value}` })
  else {
    if (sig.alg !== PACKAGE_FILE_SIGNATURE_ALG) issues.push({ path: `/signature/alg`, message: `alg is ed25519` })
    value = typeof sig.value === `string` ? base64UrlDecode(sig.value) : null
    if (value?.length !== 64) issues.push({ path: `/signature/value`, message: `value is 64 bytes, base64url` })
  }
  const author = file.author
  let key: Uint8Array<ArrayBuffer> | null = null
  if (!isObject(author)) issues.push({ path: `/author`, message: `a signed file names its author {name, key}` })
  else {
    const name = author.name
    if (typeof name !== `string` || !name.trim() || [...name].length > AUTHOR_NAME_MAX) issues.push({ path: `/author/name`, message: `name is 1 to ${AUTHOR_NAME_MAX} characters` })
    key = authorPublicKey(author.key)
    if (!key) issues.push({ path: `/author/key`, message: `key is ed25519: and 32 bytes, base64url` })
  }
  if (issues.length) return { status: `invalid`, issues }
  let ok = false
  try {
    const publicKey = await crypto.subtle.importKey(`raw`, key!, { name: `Ed25519` }, false, [`verify`])
    ok = await crypto.subtle.verify({ name: `Ed25519` }, publicKey, value!, packageFileSignedBytes(file))
  } catch {
    ok = false
  }
  if (!ok) return { status: `invalid`, issues: [{ path: `/signature`, message: `does not verify against /author/key` }] }
  const a = author as Record<string, unknown>
  return { status: `valid`, author: { name: a.name as string, key: a.key as string } }
}
