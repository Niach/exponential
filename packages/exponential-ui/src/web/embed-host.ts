// #EDE-65 lane M (fixtures/embed.json): the web half of Embed every web
// renderer (React, Solid) shares: the SANDBOXED host plugin its nested
// surface runs with, and the box it paints in. The nested surface sees the
// host's PRESENTATION (icons, url + media policy, openUrl, markdown hooks,
// paint/unknown reports) and nothing else: no host data, no host-owned
// inputs, no host functions (a call is DENIED, its action stops), no
// reactive host routes, no uploads, camera or option sources. Every inner
// event reaches the host through the Embed's own `action` instead.

import type { AnyHostPlugin, SurfaceActionEvent } from "./host-types"

/** The outcome an inner host function call gets (round 9 §6.8: a `deny`
 *  stops the action, so its event never fires). A PROMISE: the runners
 *  read a sync outcome as "done", only a settled one as a verdict. */
export const embedDenied = (): Promise<{ decision: `deny` }> => Promise.resolve({ decision: `deny` })

/** What of the host plugin the nested surface keeps (presentation only;
 *  a field added to the plugin stays OUT until listed here). */
const EMBED_KEPT = [`icons`, `openUrl`, `textLinks`, `imageSize`, `onImagePress`, `attachment`, `markdownOptions`, `popupHost`, `urls`, `media`, `mediaRequest`, `mediaStreamUrl`, `Markdown`, `resolveUrl`, `appFile`, `onPaintError`, `onUnknown`] as const

/** The nested surface's host plugin: the host's presentation, `onAction`
 *  = `forward` (the Embed's `action` emitter), host functions denied. */
export function embedHost<H extends AnyHostPlugin>(host: H, forward: (event: SurfaceActionEvent) => void | Promise<void>): H {
  const out: Record<string, unknown> = {}
  for (const key of EMBED_KEPT) if (host[key] !== undefined) out[key] = host[key]
  out.onAction = forward
  out.onFunctionCall = embedDenied
  return out as H
}

/** The Embed root's own style: the full width of its slot (a column
 *  Stack's `align` never shrinks it) unless the author placed it. */
export function embedBoxStyle(nodeStyle: Record<string, unknown> | undefined): { alignSelf: `stretch` } | undefined {
  return nodeStyle && nodeStyle.alignSelf !== undefined ? undefined : { alignSelf: `stretch` }
}
