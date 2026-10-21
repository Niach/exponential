// FEED-128: THE DOM-parity pattern. One nested tree, rendered by the React
// renderer (`@exponential-at/ui-react`, a dev dependency for tests only) and
// by this one, both in jsdom; the surface roots serialised with their
// attributes SORTED and the framework-generated ids (Radix `radix-…`,
// React `:r0:`/`«r0»`/`_r_0_`, Kobalte `kb-…`, Solid `cl-…`) renamed in order of
// appearance (references to them — `aria-controls`, `aria-labelledby`,
// `aria-describedby`, `for`, … — follow the rename). Every native port adds
// a case with `expectDomParity` (src/parity.test.ts).
//
// No JSX here: React elements come from `createElement`, Solid's from
// `createComponent`, so one file serves both runtimes.

import { expect } from "vitest"
import { act, createElement, type FunctionComponent } from "react"
import { createRoot } from "react-dom/client"
import { createComponent } from "solid-js"
import { render as renderSolid } from "solid-js/web"
import { CORE_CATALOG_ID, reduceNested } from "@exponential-at/ui"
import type { NestedNode } from "@exponential-at/ui"
import { ExponentialSurface as SolidSurface } from "../surface"
import type { HostPlugin as SolidHostPlugin } from "../host"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

/** The React renderer, loaded at run time: its `.tsx` sources are React
 *  JSX, which this package's typecheck (Solid JSX) must not compile. The
 *  host plugin is typed loosely for the same reason. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type ReactHostPlugin = Record<string, any>
const REACT_RENDERER = `@exponential-at/ui-react`
let reactSurface: FunctionComponent<Record<string, unknown>> | undefined
async function ReactSurfaceComponent(): Promise<FunctionComponent<Record<string, unknown>>> {
  reactSurface ??= ((await import(/* @vite-ignore */ REACT_RENDERER)) as { ExponentialSurface: FunctionComponent<Record<string, unknown>> }).ExponentialSurface
  return reactSurface
}

export interface ParityCase {
  tree: NestedNode
  data?: Record<string, unknown>
  theme?: string
  mode?: `light` | `dark`
  width?: number | string
  direction?: `ltr` | `rtl`
  locale?: string
  states?: readonly string[]
  /** The surface id (both sides; default `parity`). */
  id?: string
  reactHost?: ReactHostPlugin
  solidHost?: SolidHostPlugin
}

const GENERATED_ID = /^(radix-|kb-|cl-|:r|«r|_r_|r[0-9a-z]+$)/
const ID_REFS = new Set([`id`, `for`, `aria-controls`, `aria-labelledby`, `aria-describedby`, `aria-owns`, `aria-activedescendant`, `aria-details`, `aria-errormessage`, `aria-flowto`, `list`, `form`])

const escapeAttr = (v: string) => v.replace(/&/g, `&amp;`).replace(/"/g, `&quot;`).replace(/</g, `&lt;`)
const escapeText = (v: string) => v.replace(/&/g, `&amp;`).replace(/</g, `&lt;`).replace(/>/g, `&gt;`)

/** An element as a canonical string: attributes sorted by name, generated
 *  ids renamed (`$id1`, `$id2`… in document order), adjacent text merged,
 *  comments dropped. */
export function serializeDom(el: Element): string {
  const ids = new Map<string, string>()
  const rename = (token: string) => {
    if (!GENERATED_ID.test(token)) return token
    let hit = ids.get(token)
    if (!hit) {
      hit = `$id${ids.size + 1}`
      ids.set(token, hit)
    }
    return hit
  }
  // One element per line, indented: a parity diff names the element.
  const walk = (node: Node, depth: number): string => {
    const pad = `  `.repeat(depth)
    if (node.nodeType === 3) return node.nodeValue ? `${pad}${JSON.stringify(escapeText(node.nodeValue))}\n` : ``
    if (node.nodeType !== 1) return ``
    const e = node as Element
    const attrs = Array.from(e.attributes)
      .map((a) => [a.name, ID_REFS.has(a.name) ? a.value.split(/\s+/).map(rename).join(` `) : a.value] as const)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([n, v]) => ` ${n}="${escapeAttr(v)}"`)
      .join(``)
    // Adjacent text nodes merge (React and Solid split text differently).
    const kids: Node[] = []
    for (const c of Array.from(e.childNodes)) {
      const prev = kids[kids.length - 1]
      if (c.nodeType === 3 && prev?.nodeType === 3) kids[kids.length - 1] = document.createTextNode(`${prev.nodeValue ?? ``}${c.nodeValue ?? ``}`)
      else if (c.nodeType === 1 || c.nodeType === 3) kids.push(c)
    }
    if (e.localName === `style`) return `${pad}<style${attrs}>${escapeText(e.textContent ?? ``)}</style>\n`
    return `${pad}<${e.localName}${attrs}>\n${kids.map((k) => walk(k, depth + 1)).join(``)}${pad}</${e.localName}>\n`
  }
  return walk(el, 0)
}

/** The case reduced once (both renderers paint the SAME normalized tree). */
function reduced(c: ParityCase) {
  return reduceNested(c.tree, { catalogId: CORE_CATALOG_ID })
}

/** React's surface root for the case. */
export async function renderReact(c: ParityCase): Promise<{ root: Element; cleanup: () => void }> {
  const { root, templates } = reduced(c)
  const ReactSurface = await ReactSurfaceComponent()
  const container = document.createElement(`div`)
  document.body.appendChild(container)
  const r = createRoot(container)
  await act(async () => {
    r.render(createElement(ReactSurface, { id: c.id ?? `parity`, root, templates, data: c.data, theme: c.theme ?? `neutral`, mode: c.mode ?? `light`, width: c.width, direction: c.direction, locale: c.locale, states: c.states, host: c.reactHost }))
  })
  return {
    root: container.querySelector(`.xui-surface`)!,
    cleanup: () => {
      act(() => r.unmount())
      container.remove()
    },
  }
}

/** Solid's surface root for the case. */
export async function renderSolidSurface(c: ParityCase): Promise<{ root: Element; cleanup: () => void }> {
  const { root, templates } = reduced(c)
  const container = document.createElement(`div`)
  document.body.appendChild(container)
  const dispose = renderSolid(() => createComponent(SolidSurface, { id: c.id ?? `parity`, root, templates, data: c.data, theme: c.theme ?? `neutral`, mode: c.mode ?? `light`, width: c.width, direction: c.direction, locale: c.locale, states: c.states, host: c.solidHost }), container)
  await Promise.resolve()
  return {
    root: container.querySelector(`.xui-surface`)!,
    cleanup: () => {
      dispose()
      container.remove()
    },
  }
}

/** Both renderers' canonical DOM for the case. */
export async function renderBoth(c: ParityCase): Promise<{ react: string; solid: string }> {
  const r = await renderReact(c)
  const s = await renderSolidSurface(c)
  try {
    return { react: serializeDom(r.root), solid: serializeDom(s.root) }
  } finally {
    r.cleanup()
    s.cleanup()
  }
}

/** THE parity assertion: the Solid surface's DOM equals React's. */
export async function expectDomParity(c: ParityCase): Promise<void> {
  const { react, solid } = await renderBoth(c)
  expect(solid).toBe(react)
}
