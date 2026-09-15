// @vitest-environment node
//
// The island contract (EXP-887). These four exports are what a page that does
// NOT run the web app — the styleguide's Components group, the marketing docs
// — assembles a real component out of, so their SHAPE is the contract: a host
// carrying a declarative shadow root, one inert `<template>` of CSS, one
// script that adopts it.
//
// Node environment on purpose: `renderToStaticMarkup` is the server path, and
// jsdom would hand Radix a `document` and change what the primitives render.

import { createElement } from "react"
import { describe, expect, test } from "vitest"
import { fileURLToPath } from "node:url"

import {
  ISLAND_CLIENT_SCRIPT,
  ISLAND_CSS_TEMPLATE_ID,
  ISLAND_ROOT_CLASS,
  compileUiCss,
  renderIsland,
  renderIslandCssTemplate,
} from "./island"
import { IssueChip } from "./issue-chip"

const SRC_DIR = fileURLToPath(new URL(`.`, import.meta.url))

describe(`renderIsland`, () => {
  test(`wraps the static markup in a declarative shadow root on the app's face`, () => {
    const html = renderIsland(
      createElement(
        IssueChip,
        {
          identifier: `EXP-887`,
          title: `Islands`,
          status: { icon: `circle-dashed` },
        }
      )
    )
    expect(html.startsWith(`<div data-ui-island>`)).toBe(true)
    expect(html).toContain(`<template shadowrootmode="open">`)
    expect(html).toContain(`<div class="${ISLAND_ROOT_CLASS}">`)
    expect(html.endsWith(`</div></template></div>`)).toBe(true)
    // The component really rendered inside the wrapper.
    expect(html).toContain(`EXP-887`)
    expect(html).toContain(`issue-chip`)
  })

  test(`the root carries the dark token block and no ground of its own`, () => {
    // `.dark` is what makes the dark token block and every `dark:` variant
    // apply inside the shadow tree; the ground is the HOST page's, because a
    // fixed gradient layer inside a shadow tree is wrong.
    expect(ISLAND_ROOT_CLASS.split(` `)).toContain(`dark`)
    expect(ISLAND_ROOT_CLASS).not.toContain(`bg-`)
  })
})

describe(`the CSS carrier and its script`, () => {
  test(`the template is inert and keyed by the id the script looks up`, () => {
    const html = renderIslandCssTemplate(`.a{color:red}`)
    expect(html).toBe(`<template id="${ISLAND_CSS_TEMPLATE_ID}"><style>.a{color:red}</style></template>`)
    expect(ISLAND_CSS_TEMPLATE_ID).toBe(`ui-css`)
  })

  test(`the script adopts ONE sheet and polyfills declarative shadow DOM`, () => {
    expect(ISLAND_CLIENT_SCRIPT).toContain(`adoptedStyleSheets`)
    expect(ISLAND_CLIENT_SCRIPT).toContain(`attachShadow`)
    expect(ISLAND_CLIENT_SCRIPT).toContain(`new CSSStyleSheet()`)
    expect(ISLAND_CLIENT_SCRIPT).toContain(JSON.stringify(ISLAND_CSS_TEMPLATE_ID))
    expect(ISLAND_CLIENT_SCRIPT).toContain(`[data-ui-island]`)
    // The polyfill arm must not run where the browser already ran DSD.
    expect(ISLAND_CLIENT_SCRIPT).toContain(`if (!host.shadowRoot)`)
  })
})

describe(`compileUiCss`, () => {
  test(
    `emits the theme, the package's own classes and the utilities only its components name`,
    async () => {
      const css = await compileUiCss({ base: SRC_DIR })

      // `:host` is the whole reason an island is styled at all: Tailwind v4
      // writes the theme vars to `:root, :host`, and the package's own token
      // block is written the same way, so both reach a shadow tree.
      expect(css).toMatch(/:root,\s?:host/)
      expect(css).toContain(`--color-glass-card`)
      // The unlayered box the chip component and the editor decoration share.
      expect(css).toContain(`.issue-chip{`)
      // A palette utility NO app file names — it only exists in this package
      // (`BUILTIN_STATUS_COLOR_CLASS`), which is what the package's own
      // `@source` line is for.
      expect(css).toContain(`.text-yellow-500`)
      // It rides inside a `<style>`: a closing tag would end the carrier early.
      expect(css).not.toContain(`</style`)
      expect(css.length).toBeGreaterThan(10_000)
    },
    30_000
  )
})
