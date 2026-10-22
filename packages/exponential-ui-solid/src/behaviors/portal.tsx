// FEED-131: Radix `Portal` on Solid: the content is appended to the mount
// node ITSELF, no wrapper element (Solid's `<Portal>` adds a `<div>`).

import { Show, children as resolveChildren, onCleanup, type JSX } from "solid-js"
import { insert } from "solid-js/web"

/** A portal WITHOUT a wrapper (Solid's `<Portal>` adds a `<div>`, Radix's
 *  appends the content itself): the children are inserted before an empty
 *  text marker appended to `mount`, in mount order. */
export function LayerPortal(props: { mount: HTMLElement | null; children: JSX.Element }): JSX.Element {
  return (
    <Show when={props.mount}>
      {(mount) => {
        const el = mount()
        // The children first: a portal mounting INSIDE this content in the
        // same pass (a Popover open in an opening Dialog) lands BEFORE it,
        // as React places a portal's deeper host nodes first; one opened
        // later appends after it.
        const nodes = resolveChildren(() => props.children)
        const marker = document.createTextNode(``)
        el.appendChild(marker)
        insert(el, nodes, marker)
        onCleanup(() => {
          for (const n of [nodes()].flat(Infinity as 1) as unknown[]) if (n instanceof Node && n.parentNode === el) el.removeChild(n)
          marker.remove()
        })
        return null
      }}
    </Show>
  )
}

/** Mounts `el` at the end of `container` while the owner lives (Radix's
 *  Portal: no wrapper element, unlike Solid's <Portal>). */
export function mountIn(container: HTMLElement, el: Node) {
  container.appendChild(el)
  onCleanup(() => el.parentNode?.removeChild(el))
}
