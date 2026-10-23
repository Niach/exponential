// FEED-130: shared test helpers: a mounted element, a reduced fixture, a
// flush of microtasks + timers.

import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/ui"
import type { NestedNode } from "@exponential_app/ui"
import { define } from "../index"
import type { ExponentialSurfaceElement } from "../element"

define()

export const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID })

/** A new element, configured, appended to the body. */
export function mount(configure?: (el: ExponentialSurfaceElement) => void, attrs: Record<string, string> = {}): ExponentialSurfaceElement {
  const el = document.createElement(`exponential-surface`)
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v)
  configure?.(el)
  document.body.appendChild(el)
  return el
}

export const surfaceOf = (el: Element) => el.querySelector(`.xui-surface`) as HTMLElement | null

export const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms))

export function cleanup(): void {
  document.body.replaceChildren()
}

export const cardTree: NestedNode = {
  id: `root`,
  component: `Card`,
  props: { title: `Settings` },
  children: [
    { id: `text`, component: `Text`, props: { text: { path: `/name` } } },
    { id: `save`, component: `Button`, props: { label: `Save` }, on: { press: { event: { name: `save`, context: { name: { path: `/name` } } } } } },
    { id: `notify`, component: `Switch`, props: { label: `Notify`, checked: { path: `/notify` } } },
  ],
}
