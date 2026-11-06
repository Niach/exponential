// VAPP-281: the web half of the hosted-popup render option. A host that
// sets `HostPlugin.popupHost` hosts every anchored Popover / Menu OUTSIDE
// its surface: when one opens, `mount` receives the layer (its anchor's box
// in SURFACE coordinates, the side and the alignment) and returns the
// element the content renders into (a box positioned over the page, another
// window's body…); null keeps the in-surface overlay. The content then lays
// out STATICALLY inside that element (no popper positioning, no flip, no
// shift: the host owns placement, e.g. at `anchorPosition`), while presses,
// Escape and outside presses still route to the surface (the content stays
// in the renderer's tree). `unmount` hears it close.

import type { OverlayAlign, OverlaySide, Rect } from "../overlay"

/** One anchored popup a host may host outside its surface. */
export interface HostedPopup {
  /** The overlay's node id. */
  owner: string
  kind: `Popover` | `Menu`
  /** The anchor's box (the trigger, or the `anchor` node) in SURFACE coordinates. */
  anchor: Rect
  side: OverlaySide
  /** Physical: `start` = left / top (mirrored under RTL). */
  align: OverlayAlign
}

export interface PopupHost {
  /** The element this popup's content renders into, or null for the
   *  in-surface overlay. Called when it opens (and again when its anchor
   *  moved, with the same owner). */
  mount: (popup: HostedPopup) => HTMLElement | null | undefined
  /** It closed, or its surface went away. */
  unmount?: (popup: HostedPopup) => void
}

/** The attribute a host container carries: its popper wrapper lays out in
 *  flow (base-css.ts), the host places the container. */
export const HOSTED_POPUP_ATTR = `data-xui-hosted`

/** The layer `owner` opens as, measured now: the anchor's box relative to
 *  the surface element (`[data-xui-surface]`) that holds it. */
export function hostedPopupOf(owner: string, kind: HostedPopup[`kind`], anchorEl: Element | null | undefined, side: OverlaySide, align: OverlayAlign, dir: `ltr` | `rtl`): HostedPopup {
  const a = anchorEl?.getBoundingClientRect()
  const s = anchorEl?.closest(`[data-xui-surface]`)?.getBoundingClientRect()
  const physical: OverlayAlign = dir === `rtl` && align !== `center` ? (align === `start` ? `end` : `start`) : align
  return {
    owner,
    kind,
    anchor: a ? { x: a.left - (s?.left ?? 0), y: a.top - (s?.top ?? 0), width: a.width, height: a.height } : { x: 0, y: 0, width: 0, height: 0 },
    side,
    align: physical,
  }
}

/** Ask the host for a container; marks it `data-xui-hosted`. */
export function mountHostedPopup(host: PopupHost | undefined, popup: HostedPopup): HTMLElement | null {
  const el = host?.mount(popup) ?? null
  el?.setAttribute(HOSTED_POPUP_ATTR, popup.owner)
  return el
}
