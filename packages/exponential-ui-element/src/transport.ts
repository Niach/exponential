// FEED-130: the element's `src` / `post` / `transport` attributes → the host
// API (`ExponentialHost` over one of the SDK's transports), so a page
// streams a surface with markup alone. `ws:`/`wss:` or `transport="ws"` =
// WebSocket frames, `transport="sse"` = Server-Sent Events, anything else =
// A2UI JSONL over a streamed HTTP response; client messages POST to `post`
// (default `src`). The element owns a host it built: connect on attach,
// close on detach or when the three change.

import { ExponentialHost, JsonlStreamTransport, SseTransport, WebSocketTransport } from "@exponential-at/ui"
import type { ExtensionDef, Transport } from "@exponential-at/ui"

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

/** A host over the attributes' transport (not connected yet). `extensions`
 *  = the catalogs of the element's `extensions` at build time (the host's
 *  reducer must know them, or an extension component reduces to Unknown). */
export function createHost(attrs: TransportAttributes, extensions: readonly ExtensionDef[] = []): ExponentialHost {
  return new ExponentialHost({ transport: createTransport(attrs), extensions })
}

/** The identity of a built host: a change rebuilds it. */
export const transportKey = (attrs: TransportAttributes): string => `${transportKind(attrs)}\u0000${attrs.src}\u0000${attrs.post ?? ``}`
