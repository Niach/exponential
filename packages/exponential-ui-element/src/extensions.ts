// FEED-130: extensions without a framework. An `ElementPainter` is a plain
// function: the node, its resolved props, the theme and mode in, ONE DOM
// element out. `defineElementExtension` adapts each into the Solid
// extension component the painter expects: the painter runs in a memo over
// the node, props, theme, mode and root attributes (a change paints a NEW
// element, which replaces the old; Solid inserts a DOM node as it is), and
// the node's children and slots are resolved once through Solid's
// `children()` (the same nodes move into each new element).

import { children as resolveChildren, createMemo, getOwner, runWithOwner, untrack } from "solid-js"
import type { JSX } from "solid-js"
import type { ExtensionDef, ModeName, ResolvedTheme, UiNode } from "@exponential_app/ui"
import { defineSolidExtension, registerExtension as registerSolidExtension, type ExtensionComponent, type ExtensionComponentProps, type RegisterExtensionOptions, type SolidExtension } from "@exponential_app/ui-solid"

export interface ElementPaintContext {
  node: UiNode
  /** The node's props, resolved against the data model. */
  props: Record<string, unknown>
  theme: ResolvedTheme
  mode: ModeName
  tokens: ResolvedTheme[`tokens`]
  /** Fire one of the node's `on` handlers (`own` = what the element just
   *  wrote, by prop). A promise = the host is still handling it. */
  emit: (event: string, payload?: Record<string, unknown>, own?: Record<string, unknown>) => Promise<void> | void
  /** What the renderer wants on the root: `class`, `data-xui-id`,
   *  `data-xui-c`, recipe `data-r-*`/`data-m-*`, `data-xs`, aria, `dir`,
   *  `style` (kebab-case keys). */
  rootProps: Record<string, unknown>
  /** Sets `rootProps` on `el` (classes added to its own). */
  applyRootProps: (el: Element) => void
  /** The node's painted children (append them where they belong). */
  children: () => Node[]
  /** One slot's painted nodes (empty when the node has no such slot). */
  slot: (name: string) => Node[]
}

export type ElementPainter = (ctx: ElementPaintContext) => Element

/** `rootProps` onto an element: classes merge, `style` entries are set one
 *  by one, null/undefined skip, everything else as a string. */
export function applyRootProps(el: Element, rootProps: Record<string, unknown>): void {
  for (const [name, value] of Object.entries(rootProps)) {
    if (value === undefined || value === null) continue
    if (name === `class`) {
      for (const c of String(value).split(/\s+/)) if (c) el.classList.add(c)
    } else if (name === `style`) {
      const style = (el as HTMLElement).style
      if (!style || typeof value !== `object`) continue
      for (const [k, v] of Object.entries(value as Record<string, unknown>)) if (v !== undefined && v !== null) style.setProperty(k, String(v))
    } else el.setAttribute(name, String(value))
  }
}

const toNodes = (value: unknown): Node[] => {
  const out: Node[] = []
  const visit = (v: unknown) => {
    if (Array.isArray(v)) v.forEach(visit)
    else if (typeof v === `function`) visit((v as () => unknown)())
    else if (v instanceof Node) out.push(v)
    else if (typeof v === `string` || typeof v === `number`) out.push(document.createTextNode(String(v)))
  }
  visit(value)
  return out
}

function adapt(painter: ElementPainter): ExtensionComponent {
  return (p: ExtensionComponentProps): JSX.Element => {
    const owner = getOwner()
    const kids = resolveChildren(() => p.children)
    const slots = new Map<string, () => unknown>()
    const slotOf = (name: string) => {
      let hit = slots.get(name)
      if (!hit) {
        // Created under the component (not the paint memo): a re-paint keeps
        // the slot's painted subtree.
        hit = runWithOwner(owner, () => resolveChildren(() => untrack(() => p.slots)[name])) as () => unknown
        slots.set(name, hit)
      }
      return hit
    }
    for (const name of Object.keys(untrack(() => p.node.slots) ?? {})) slotOf(name)
    const painted = createMemo(() => {
      const rootProps = p.rootProps
      const ctx: ElementPaintContext = {
        node: p.node,
        props: p.props,
        theme: p.theme,
        mode: p.mode,
        tokens: p.tokens,
        emit: p.emit,
        rootProps,
        applyRootProps: (el) => applyRootProps(el, rootProps),
        children: () => toNodes(kids()),
        slot: (name) => (p.node.slots?.[name] ? toNodes(slotOf(name)()) : []),
      }
      return painter(ctx)
    })
    return painted as unknown as JSX.Element
  }
}

/** A per-surface extension (the `extensions` property) or, through
 *  `registerExtension`, a page-wide one, painted by vanilla painters. A
 *  `components` key naming a core native OVERRIDES its painter. */
export function defineElementExtension(options: { catalog?: ExtensionDef | unknown; components: Record<string, ElementPainter> }): SolidExtension {
  const components: Record<string, ExtensionComponent> = {}
  for (const [name, painter] of Object.entries(options.components)) components[name] = adapt(painter)
  return defineSolidExtension({ catalog: options.catalog as ExtensionDef | undefined, components })
}

/** Register an extension for every surface on the page (Solid's
 *  `registerExtension`), taking a `defineElementExtension` /
 *  `defineSolidExtension` result as well as the raw options. Returns the
 *  unregister function. */
export function registerExtension(extension: SolidExtension | RegisterExtensionOptions): () => void {
  const options: RegisterExtensionOptions = `catalog` in extension && extension.catalog === null ? { components: extension.components } : (extension as RegisterExtensionOptions)
  return registerSolidExtension(options)
}
