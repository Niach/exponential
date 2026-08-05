// EXP-423 — drift gate for the hand-mirrored status glyph masks.
//
// `lib/status-icon-svg.ts` restates the registry's SVG geometry as raw markup
// because a CSS `::before` mask cannot render a React component. This file is
// what keeps the two in step: it renders the real registry components and
// compares element-for-element, attribute-for-attribute. Sibling of
// icons.test.ts, which locks the registry itself.

import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { ICON_COMPONENTS } from "./icons.generated"
import { categoryStatusIcon } from "./status-icons"
import { issueStatusCategoryValues } from "./domain"
import {
  STATUS_ICON_BODIES,
  STATUS_ICON_NAME_LIST,
  STATUS_ICON_SVG_ATTRS,
  statusIconDataUri,
  statusIconSvg,
  type StatusIconName,
} from "./status-icon-svg"

interface SvgElement {
  tag: string
  attrs: Record<string, string>
}

/** Split an SVG document into its root attributes and its child elements. */
function parseSvg(markup: string): {
  root: Record<string, string>
  children: SvgElement[]
} {
  const doc = new DOMParser().parseFromString(markup, `image/svg+xml`)
  const svg = doc.documentElement
  expect(svg.nodeName).toBe(`svg`)
  const attrsOf = (element: Element): Record<string, string> =>
    Object.fromEntries(
      Array.from(element.attributes).map((attr) => [attr.name, attr.value])
    )
  return {
    root: attrsOf(svg),
    children: Array.from(svg.children).map((child) => ({
      tag: child.nodeName,
      attrs: attrsOf(child),
    })),
  }
}

function registryMarkup(name: StatusIconName): string {
  return renderToStaticMarkup(createElement(ICON_COMPONENTS[name]))
}

describe(`status icon masks`, () => {
  it.each(STATUS_ICON_NAME_LIST)(`%s matches the registry geometry`, (name) => {
    const registry = parseSvg(registryMarkup(name))
    const mirrored = parseSvg(statusIconSvg(name))

    // Same shapes, same order, same coordinates — a moved path or a changed
    // radius in the registry must fail here.
    expect(mirrored.children).toEqual(registry.children)

    // …and drawn the same way: a stroke-width or linecap drift changes the
    // mask's alpha, i.e. the glyph's weight in the pill.
    for (const [key, value] of Object.entries(STATUS_ICON_SVG_ATTRS)) {
      expect({ key, value }).toEqual({ key, value: registry.root[key] })
    }
  })

  it(`covers every glyph the status resolver can produce`, () => {
    // Started rows pick a clock by position, so the reachable set grows with
    // the started cap — a 5th clock would land here before it lands in a pill.
    const reachable = new Set<string>()
    for (const category of issueStatusCategoryValues) {
      for (let count = 1; count <= 6; count++) {
        for (let index = 0; index < count; index++) {
          reachable.add(categoryStatusIcon(category, index, count))
        }
      }
    }
    expect([...reachable].sort()).toEqual([...STATUS_ICON_NAME_LIST].sort())
  })

  it(`encodes the mask so it survives a style attribute`, () => {
    const uri = statusIconDataUri(`circle-check`)
    expect(uri.startsWith(`url("data:image/svg+xml,`)).toBe(true)
    // A raw `#`, `;` or `"` inside the declaration would truncate it (or the
    // whole `style` attribute) — the encoding must leave none behind.
    const encoded = uri.slice(`url("data:image/svg+xml,`.length, -2)
    expect(encoded).not.toMatch(/[#;"<>]/)
    expect(decodeURIComponent(encoded)).toBe(statusIconSvg(`circle-check`))
  })

  it(`falls back to the backlog glyph for an unknown name`, () => {
    expect(statusIconDataUri(`not-a-status-glyph`)).toBe(
      statusIconDataUri(`circle-dashed`)
    )
  })

  it(`has a body for every declared name`, () => {
    expect(Object.keys(STATUS_ICON_BODIES).sort()).toEqual(
      [...STATUS_ICON_NAME_LIST].sort()
    )
  })
})
