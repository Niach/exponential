// VAPP-91: the policy rules every host applies before a client function, a
// URL open or a media load (`catalog/host.json` functions / urls / media).
// Pure; `fixtures/host-policy.json` locks them on every platform.

import { DEFAULT_MEDIA_SCHEMES, DEFAULT_URL_SCHEMES } from "./contract"
import { coreCatalog } from "../catalog"

/** The catalog's built-in client functions: the basic 14 + round 1's 15
 *  core ones (`functions.names`). */
export const BUILTIN_FUNCTIONS: readonly string[] = (coreCatalog as unknown as { functions: { names: string[] } }).functions.names

export type FunctionDecision = `allow` | `ask` | `deny` | `not_found`

export interface FunctionPolicy {
  allow?: readonly string[]
  ask?: readonly string[]
  deny?: readonly string[]
  /** For a registered name no list matches; default `allow`. */
  default?: `allow` | `ask` | `deny`
}

/** `harness.*` matches `harness.toast`; anything else is exact. */
export function matchesPattern(pattern: string, name: string): boolean {
  return pattern.endsWith(`*`) ? name.startsWith(pattern.slice(0, -1)) : pattern === name
}

/** The gate: built-ins always run; an unregistered name is `not_found`;
 *  then deny wins, then allow, then ask, then the policy's default. */
export function decideFunction(policy: FunctionPolicy | undefined, name: string, registered: boolean): FunctionDecision {
  if (BUILTIN_FUNCTIONS.includes(name)) return `allow`
  if (!registered) return `not_found`
  const p = policy ?? {}
  const any = (list?: readonly string[]) => (list ?? []).some((pattern) => matchesPattern(pattern, name))
  if (any(p.deny)) return `deny`
  if (any(p.allow)) return `allow`
  if (any(p.ask)) return `ask`
  return p.default ?? `allow`
}

/** Two policies stacked (a package's allowlist under the host's own): the
 *  stricter decision wins. */
export function combineDecisions(a: FunctionDecision, b: FunctionDecision): FunctionDecision {
  const rank: Record<FunctionDecision, number> = { allow: 0, ask: 1, deny: 2, not_found: 3 }
  return rank[a] >= rank[b] ? a : b
}

/** A declarative package's `functions` list as a policy: listed = allow,
 *  anything else = deny. */
export function packagePolicy(functions: readonly string[] | undefined): FunctionPolicy {
  return { allow: functions ?? [], default: `deny` }
}

export interface UrlPolicy {
  schemes?: readonly string[]
  /** http(s) hosts allowed: exact or `*.example.com`; unset = any. */
  hosts?: readonly string[]
  baseUrl?: string
}

export interface UrlDecision {
  allowed: boolean
  /** The absolute url when it parses. */
  url?: string
  reason?: `invalid` | `scheme` | `host`
}

function absolute(url: string, baseUrl?: string): URL | null {
  try {
    return new URL(url)
  } catch {
    if (!baseUrl) return null
    try {
      return new URL(url, baseUrl)
    } catch {
      return null
    }
  }
}

function hostMatches(pattern: string, host: string): boolean {
  const p = pattern.toLowerCase()
  return p.startsWith(`*.`) ? host.endsWith(p.slice(1)) && host.length > p.length - 1 : host === p
}

export function decideUrl(policy: UrlPolicy | undefined, url: string): UrlDecision {
  const parsed = absolute(url.trim(), policy?.baseUrl)
  if (!parsed) return { allowed: false, reason: `invalid` }
  const scheme = parsed.protocol.replace(/:$/, ``).toLowerCase()
  const schemes = policy?.schemes ?? DEFAULT_URL_SCHEMES
  if (!schemes.includes(scheme)) return { allowed: false, url: parsed.href, reason: `scheme` }
  if (policy?.hosts && (scheme === `http` || scheme === `https`) && !policy.hosts.some((h) => hostMatches(h, parsed.hostname.toLowerCase())))
    return { allowed: false, url: parsed.href, reason: `host` }
  return { allowed: true, url: parsed.href }
}

export interface MediaRule {
  prefix: string
  headers: Record<string, string>
}

export interface MediaOptions {
  baseUrl?: string
  rules?: readonly MediaRule[]
  /** The schemes a src may use; default `DEFAULT_MEDIA_SCHEMES` (https,
   *  http, data). `file` only when listed. */
  schemes?: readonly string[]
  /** http(s) hosts media may load from: exact or `*.example.com`; unset = any. */
  hosts?: readonly string[]
}

export interface MediaRequest {
  url: string
  headers: Record<string, string>
}

/** The image / media loader's request: the absolute url plus the headers of
 *  every rule whose prefix it starts with (later rules win per header).
 *  Null when the url does not resolve or the media policy (schemes, hosts)
 *  denies it: nothing loads. */
export function mediaRequest(url: string, options: MediaOptions = {}): MediaRequest | null {
  const d = decideUrl({ baseUrl: options.baseUrl, schemes: options.schemes ?? DEFAULT_MEDIA_SCHEMES, hosts: options.hosts }, url)
  if (!d.allowed || !d.url) return null
  const href = d.url
  const headers: Record<string, string> = {}
  for (const rule of options.rules ?? []) if (href.startsWith(rule.prefix)) Object.assign(headers, rule.headers)
  return { url: href, headers: sortKeys(headers) }
}

/** The href a renderer may navigate to (Link, markdown links, FileUpload
 *  file urls), or undefined when the URL policy denies it. */
export function safeHref(policy: UrlPolicy | undefined, url: unknown): string | undefined {
  if (typeof url !== `string` || !url) return undefined
  const d = decideUrl(policy, url)
  return d.allowed ? d.url : undefined
}

export function sortKeys<T>(record: Record<string, T>): Record<string, T> {
  return Object.fromEntries(Object.entries(record).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
}

/** Width × height from an image's header (PNG, JPEG, GIF, WebP, BMP)
 *  without decoding it: what a loader checks against `MEDIA_LIMITS.maxPixels`
 *  BEFORE decoding (the Rust core's `image_dimensions`). Null = not a header
 *  this reads (SVG, truncated bytes). */
export function imageDimensions(bytes: Uint8Array): [number, number] | null {
  const n = bytes.length
  const be16 = (i: number) => (i + 2 <= n ? (bytes[i]! << 8) | bytes[i + 1]! : null)
  const le16 = (i: number) => (i + 2 <= n ? bytes[i]! | (bytes[i + 1]! << 8) : null)
  const be32 = (i: number) => (i + 4 <= n ? ((bytes[i]! << 24) | (bytes[i + 1]! << 16) | (bytes[i + 2]! << 8) | bytes[i + 3]!) >>> 0 : null)
  const le32 = (i: number) => (i + 4 <= n ? (bytes[i]! | (bytes[i + 1]! << 8) | (bytes[i + 2]! << 16) | (bytes[i + 3]! << 24)) >>> 0 : null)
  const le24 = (i: number) => (i + 3 <= n ? bytes[i]! | (bytes[i + 1]! << 8) | (bytes[i + 2]! << 16) : null)
  const starts = (sig: number[], at = 0) => sig.every((b, i) => bytes[at + i] === b)
  const pair = (w: number | null, h: number | null): [number, number] | null => (w === null || h === null ? null : [w, h])
  if (starts([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return pair(be32(16), be32(20))
  if (starts([0x47, 0x49, 0x46, 0x38]) && (bytes[4] === 0x37 || bytes[4] === 0x39) && bytes[5] === 0x61) return pair(le16(6), le16(8))
  if (starts([0x42, 0x4d])) {
    const h = le32(22)
    return pair(le32(18), h === null ? null : Math.abs(h | 0))
  }
  if (n >= 30 && starts([0x52, 0x49, 0x46, 0x46]) && starts([0x57, 0x45, 0x42, 0x50], 8)) {
    const kind = String.fromCharCode(bytes[12]!, bytes[13]!, bytes[14]!, bytes[15]!)
    if (kind === `VP8 `) return pair(le16(26)! & 0x3fff, le16(28)! & 0x3fff)
    if (kind === `VP8L`) {
      const b = le32(21)
      return b === null ? null : [(b & 0x3fff) + 1, ((b >>> 14) & 0x3fff) + 1]
    }
    if (kind === `VP8X`) {
      const w = le24(24)
      const h = le24(27)
      return w === null || h === null ? null : [w + 1, h + 1]
    }
    return null
  }
  if (starts([0xff, 0xd8])) {
    let i = 2
    while (i + 4 <= n) {
      if (bytes[i] !== 0xff) {
        i += 1
        continue
      }
      const marker = bytes[i + 1]!
      if (marker === 0xff) {
        i += 1
        continue
      }
      if (marker === 0xd8 || marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) {
        i += 2
        continue
      }
      const len = be16(i + 2)
      if (len === null) return null
      if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) return pair(be16(i + 7), be16(i + 5))
      i += 2 + len
    }
  }
  return null
}
