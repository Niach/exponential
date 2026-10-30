// FEED-130: the element's `src` / `post` / `transport` attributes → the host
// API (`ExponentialHost` over one of the SDK's transports), so a page
// streams a surface with markup alone. `ws:`/`wss:` or `transport="ws"` =
// WebSocket frames, `transport="sse"` = Server-Sent Events, anything else =
// A2UI JSONL over a streamed HTTP response; client messages POST to `post`
// (default `src`). The element owns a host it built: connect on attach,
// close on detach or when the three change.
//
// VAPP-107: `src` and `post` pass the page's URL policy (`plugin.urls`, the
// same one every href passes) BEFORE a transport exists: a private,
// loopback or link-local host is denied unless `allowPrivate` opts it in
// (the page's own private origin is, as for every href), a `hosts`
// allowlist holds, `schemes: ["https"]` means https + wss only, and a
// javascript:/file:/data: src never connects. ws(s) urls are decided as
// their http(s) twins (the policy knows http hosts). Denied = no host, no
// request; the element reports `closed` with the reason.

import { DEFAULT_URL_SCHEMES, ExponentialHost, JsonlStreamTransport, SseTransport, WebSocketTransport, decideUrl } from "@exponential_app/eui"
import type { ExtensionDef, Transport, UrlDecision, UrlPolicy } from "@exponential_app/eui"
import { ownPrivateHosts } from "@exponential_app/eui/web"

export type TransportKind = `jsonl` | `sse` | `ws`

export interface TransportAttributes {
  src: string
  post?: string | null
  transport?: string | null
}

/** Which transport the attributes name. */
export function transportKind(attrs: TransportAttributes): TransportKind {
  if (attrs.transport === `ws` || /^wss?:/i.test(attrs.src)) return `ws`
  if (attrs.transport === `sse`) return `sse`
  return `jsonl`
}

export function createTransport(attrs: TransportAttributes): Transport {
  const kind = transportKind(attrs)
  const postUrl = attrs.post || undefined
  if (kind === `ws`) return new WebSocketTransport({ url: attrs.src })
  if (kind === `sse`) return new SseTransport({ url: attrs.src, postUrl })
  return new JsonlStreamTransport({ url: attrs.src, postUrl })
}

/** The decision for `src` and `post` under the page's URL policy. */
export interface TransportDecision {
  allowed: boolean
  src: UrlDecision
  /** Only when `post` is set. */
  post?: UrlDecision
  /** Why not, for the element's status (`src denied by the URL policy (private)`). */
  reason?: string
}

const WS_OF: Record<string, string> = { http: `ws`, https: `wss` }

/** The transport schemes a URL policy allows: its http(s) entries (default
 *  both) and their ws(s) twins; nothing else (mailto, tel, javascript,
 *  file, data never carry a transport). */
export function transportSchemes(policy: UrlPolicy | undefined): string[] {
  const http = (policy?.schemes ?? DEFAULT_URL_SCHEMES).filter((s) => s === `http` || s === `https`)
  return [...http, ...http.map((s) => WS_OF[s]!)]
}

const documentBase = (): string | undefined => (typeof document !== `undefined` && document.baseURI ? document.baseURI : undefined)

function decideOne(url: string, policy: UrlPolicy | undefined, schemes: readonly string[]): UrlDecision {
  const baseUrl = policy?.baseUrl ?? documentBase()
  let parsed: URL
  try {
    parsed = new URL(url.trim(), baseUrl)
  } catch {
    return { allowed: false, reason: `invalid` }
  }
  const scheme = parsed.protocol.replace(/:$/, ``).toLowerCase()
  if (!schemes.includes(scheme)) return { allowed: false, url: parsed.href, reason: `scheme` }
  // ws(s) decided as http(s): the host allowlist and the private-host rule apply alike.
  const twin = scheme === `ws` ? `http:${parsed.href.slice(3)}` : scheme === `wss` ? `https:${parsed.href.slice(4)}` : parsed.href
  const own = ownPrivateHosts()
  const allowPrivate = own.length ? [...(policy?.allowPrivate ?? []), ...own] : policy?.allowPrivate
  const d = decideUrl({ schemes: [`http`, `https`], hosts: policy?.hosts, allowPrivate, baseUrl }, twin)
  return d.allowed ? { allowed: true, url: parsed.href } : { allowed: false, url: parsed.href, reason: d.reason }
}

/** VAPP-107: `src` (and `post` when set) under the page's URL policy. */
export function decideTransport(attrs: TransportAttributes, policy?: UrlPolicy): TransportDecision {
  const schemes = transportSchemes(policy)
  const src = decideOne(attrs.src, policy, schemes)
  const post = attrs.post ? decideOne(attrs.post, policy, schemes) : undefined
  const denied = !src.allowed ? [`src`, src] as const : post && !post.allowed ? [`post`, post] as const : undefined
  return denied ? { allowed: false, src, post, reason: `${denied[0]} denied by the URL policy (${denied[1].reason})` } : { allowed: true, src, post }
}

/** A host over the attributes' transport (not connected yet). `extensions`
 *  = the catalogs of the element's `extensions` at build time (the host's
 *  reducer must know them, or an extension component reduces to Unknown).
 *  VAPP-107: `policy` (the page's URL policy, default the SDK's) decides
 *  `src` and `post` first; a denied one throws before any transport exists. */
export function createHost(attrs: TransportAttributes, extensions: readonly ExtensionDef[] = [], policy?: UrlPolicy): ExponentialHost {
  const decision = decideTransport(attrs, policy)
  if (!decision.allowed) throw new Error(`exponential-ui: ${decision.reason}`)
  return new ExponentialHost({ transport: createTransport(attrs), extensions })
}

/** The identity of a built host: a change rebuilds it. */
export const transportKey = (attrs: TransportAttributes): string => `${transportKind(attrs)}\u0000${attrs.src}\u0000${attrs.post ?? ``}`
