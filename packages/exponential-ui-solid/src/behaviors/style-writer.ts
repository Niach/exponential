// FEED-131: React's inline-style writes on a live element (react-dom's
// `setValueForStyles`): the order of the `style` attribute's declarations is
// what the web-parity check diffs, so every Radix-shaped style write goes
// through here.

import { createRenderEffect, type Accessor } from "solid-js"

export type StyleObject = Record<string, string | undefined>

/** React's `setValueForStyles(node, next, prev)` (kebab-case keys): a key
 *  gone from `next` is cleared, a changed one is written; an existing
 *  declaration keeps its place in the `style` attribute, a new one appends. */
export function styleWriter(el: HTMLElement): (next: StyleObject) => void {
  let prev: StyleObject | null = null
  const write = (k: string, v: string | undefined) => el.style.setProperty(k, v === undefined ? `` : v)
  return (next) => {
    if (prev) {
      for (const k of Object.keys(prev)) if (!(k in next)) write(k, ``)
      for (const k of Object.keys(next)) if (prev[k] !== next[k]) write(k, next[k])
    } else for (const k of Object.keys(next)) write(k, next[k])
    prev = { ...next }
  }
}

/** Write `style()` with React's semantics on `el`, first now, then on change. */
export function bindStyle(el: HTMLElement, style: Accessor<StyleObject>) {
  const write = styleWriter(el)
  createRenderEffect(() => write(style()))
}
