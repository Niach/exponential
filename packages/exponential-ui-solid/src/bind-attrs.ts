// VAPP-145: a bag of PLAIN attributes (class, role, data-*, aria-*; no
// events, no style, no properties) bound to an element with ONE render
// effect that writes only what changed. What `{...bag}` in JSX does for such
// a bag, without `spread` + `mergeProps` resolving every key on each run:
// the windowed rows and table cells are created by the dozen on a scroll
// step or a sort.

import { createRenderEffect } from "solid-js"

type Bag = Record<string, unknown>

// Solid's own rule: null / undefined remove the attribute, anything else is
// written as a string (`false` → "false").
const absent = (v: unknown) => v === undefined || v === null

/** Bind `bag()` onto `el` (call it from a `ref`: inside the row's owner). */
export function bindAttrs(el: Element, bag: () => Bag | undefined): void {
  createRenderEffect<Bag>((prev) => {
    const next = bag() ?? {}
    for (const k in prev) if (!(k in next) && !absent(prev[k])) el.removeAttribute(k)
    for (const k in next) {
      const v = next[k]
      if (v === prev[k]) continue
      if (absent(v)) el.removeAttribute(k)
      else el.setAttribute(k, String(v))
    }
    return next
  }, {})
}
