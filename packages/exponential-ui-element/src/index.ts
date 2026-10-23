// @exponential_app/ui-element — Exponential UI as a custom element (FEED-130):
// `<exponential-surface>` paints a core A2UI surface in any page, with no
// framework, by mounting the Solid renderer inside itself (the same DOM,
// stylesheet and runtime themes as the React renderer). This entry has no
// side effects: `define()` registers the tag (`./define` does it on import,
// `./standalone` is the self-contained bundle).

import { ExponentialSurfaceElement, TAG } from "./element"

export { ExponentialSurfaceElement, TAG, lengthOf } from "./element"
export { iconsFromSvg, svgIcon } from "./icons"
export { defineElementExtension, registerExtension, applyRootProps } from "./extensions"
export type { ElementPainter, ElementPaintContext } from "./extensions"
export { createHost, createTransport, transportKind, decideTransport, transportSchemes } from "./transport"
export type { TransportKind, TransportAttributes, TransportDecision } from "./transport"
export type { SurfaceActionDetail, SurfaceInputDetail, FunctionCallDetail, OpenUrlDetail, UploadDetail, StatusDetail, WaitUntil, ExponentialSurfaceEventMap, ExponentialSurfaceEventName } from "./events"
export { defineSolidExtension, createSurface } from "@exponential_app/ui-solid"
export type { A2uiMessage, SurfaceState, SurfaceHandle, HostPlugin, IconMap, IconComponent, ThemeInput, SolidExtension } from "@exponential_app/ui-solid"

/** Register the element under `tag` (default `exponential-surface`).
 *  Idempotent; a tag already taken by another class throws. Another tag
 *  gets its own subclass (one constructor per tag). */
export function define(tag: string = TAG): void {
  if (typeof customElements === `undefined`) return
  const existing = customElements.get(tag)
  if (existing) {
    if (existing === ExponentialSurfaceElement || existing.prototype instanceof ExponentialSurfaceElement) return
    throw new Error(`exponential-ui: <${tag}> is already defined by another class (a second copy of @exponential_app/ui-element, or another library); pick another tag with define("my-surface")`)
  }
  customElements.define(tag, tag === TAG ? ExponentialSurfaceElement : class extends ExponentialSurfaceElement {})
  installHostRule(tag)
}

/** ONE document rule per tag, `:where(<tag>){display:contents}` (specificity
 *  0: any author rule wins): the element is no box of its own, the surface's
 *  `.xui-surface` root is. Idempotent; nothing without a document. */
export function installHostRule(tag: string): void {
  if (typeof document === `undefined` || !document.head) return
  if (document.head.querySelector(`style[data-xui-element="${tag}"]`)) return
  const style = document.createElement(`style`)
  style.setAttribute(`data-xui-element`, tag)
  style.textContent = `:where(${tag}){display:contents}`
  document.head.appendChild(style)
}
