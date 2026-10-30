// FEED-130: icons for a page with no component framework. The Solid painter
// draws the catalog's `Icon` names through `plugin.icons` (Solid
// components); `iconsFromSvg` wraps plain SVG MARKUP strings (an icon
// sprite, lucide-static, a CMS) into such components. A host icon that is
// not a lucide-solid glyph is painted AS IT IS (icons.tsx `PaintIcon`), so
// the wrapper itself writes what lucide-react's `<svg>` root carries for
// the props it is handed: `width`/`height` = `width` ?? `size` ?? 24,
// `class` = the markup's classes + the painter's, `aria-hidden="true"`
// unless an aria-*/role/title prop is passed, `stroke` = `color`,
// `stroke-width` = `strokeWidth`, and every other string/number attribute
// (`data-icon`, `data-slot`…) as given. The markup is the host's (trusted);
// scripts, `foreignObject`, `on*` handlers and `javascript:` links are
// dropped anyway.

import { createRenderEffect } from "solid-js"
import type { IconComponent, IconMap } from "@exponential_app/eui-solid"

const SKIP = new Set([`size`, `class`, `color`, `strokeWidth`, `absoluteStrokeWidth`, `children`, `ref`, `width`, `height`])

function sanitize(root: Element): void {
  for (const el of Array.from(root.querySelectorAll(`script,foreignObject`))) el.remove()
  for (const el of [root, ...Array.from(root.querySelectorAll(`*`))]) {
    for (const a of Array.from(el.attributes)) {
      if (/^on/i.test(a.name) || (/(^|:)href$/i.test(a.name) && /^\s*javascript:/i.test(a.value))) el.removeAttribute(a.name)
    }
  }
}

function parse(markup: string): SVGSVGElement | null {
  if (typeof document === `undefined`) return null
  const t = document.createElement(`template`)
  t.innerHTML = markup.trim()
  const svg = t.content.firstElementChild
  if (!svg || svg.localName !== `svg`) return null
  sanitize(svg)
  return svg as SVGSVGElement
}

const classTokens = (...values: (string | null | undefined)[]): string =>
  values
    .flatMap((v) => (v ?? ``).split(/\s+/))
    .filter((c, i, all) => c !== `` && all.indexOf(c) === i)
    .join(` `)

/** One SVG markup string as an icon component. */
export function svgIcon(markup: string): IconComponent {
  let template: SVGSVGElement | null | undefined
  return ((props: Record<string, unknown>) => {
    if (template === undefined) template = parse(markup)
    if (!template) return null
    const svg = template.cloneNode(true) as SVGSVGElement
    const base = template.getAttribute(`class`)
    let written = new Set<string>()
    createRenderEffect(() => {
      const next = new Set<string>()
      const set = (name: string, value: unknown) => {
        if (value === undefined || value === null || value === false) return
        svg.setAttribute(name, String(value))
        next.add(name)
      }
      const size = props.size as number | string | undefined
      set(`width`, props.width ?? size ?? 24)
      set(`height`, props.height ?? size ?? 24)
      const cls = classTokens(base, props.class as string | undefined)
      if (cls) set(`class`, cls)
      if (props.color !== undefined) set(`stroke`, props.color)
      if (props.strokeWidth !== undefined) set(`stroke-width`, props.strokeWidth)
      const keys = Object.keys(props)
      const a11y = keys.some((k) => k.startsWith(`aria-`) || k === `role` || k === `title`)
      if (!a11y) set(`aria-hidden`, `true`)
      for (const k of keys) {
        if (SKIP.has(k)) continue
        const v = props[k]
        if (typeof v === `string` || typeof v === `number` || v === true) set(k, v)
      }
      // Attributes a previous run wrote and this one does not: back to the
      // markup's own value (or gone).
      for (const name of written) {
        if (next.has(name)) continue
        const own = template!.getAttribute(name)
        if (own === null) svg.removeAttribute(name)
        else svg.setAttribute(name, own)
      }
      written = next
    })
    return svg
  }) as unknown as IconComponent
}

/** A registry name → SVG markup lookup, as the icon map `plugin.icons`
 *  takes. A name it has no markup for falls through to the renderer's own
 *  glyphs (then the placeholder). One component per distinct markup. */
export function iconsFromSvg(resolve: (name: string) => string | undefined): IconMap {
  const byMarkup = new Map<string, IconComponent>()
  return (name: string) => {
    const markup = resolve(name)
    if (!markup) return undefined
    let icon = byMarkup.get(markup)
    if (!icon) {
      icon = svgIcon(markup)
      byMarkup.set(markup, icon)
    }
    return icon
  }
}

