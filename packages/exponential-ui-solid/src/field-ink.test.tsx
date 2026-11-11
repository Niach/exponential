// #EXP-1787: interactions-round1.json `fieldInk` on Solid, like React's
// interactions-fixture.test.tsx: an Input/Textarea node's own `style.color`
// colours its TYPED text (the field element), the caret stays pinned to the
// `field` recipe's colour and no node rule reaches the placeholder.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { builtinTheme, nativeRecipeProps, resolveRecipe, type NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { createSurface } from "./create-surface"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

interface Case {
  name: string
  node: NestedNode & { style?: Record<string, unknown> }
  value: string
  valueOpacity?: number
  placeholder: string
  caret: string
}
const fx = (fixture as unknown as { fieldInk: { theme: string; mode: `light` | `dark`; cases: Case[] } }).fieldInk

/** The value the surface's stylesheets give `el` for `prop` (jsdom computes
 *  no cascade: the last matching rule's declaration). */
function cascaded(el: Element, prop: string): string {
  let value = ``
  const walk = (rules: CSSRuleList) => {
    for (const rule of Array.from(rules)) {
      if (rule instanceof CSSStyleRule) {
        const v = rule.style.getPropertyValue(prop)
        if (v && el.matches(rule.selectorText)) value = v
      } else if (`cssRules` in rule) walk((rule as CSSGroupingRule).cssRules)
    }
  }
  for (const sheet of Array.from(document.styleSheets)) walk(sheet.cssRules)
  return el instanceof HTMLElement && el.style.getPropertyValue(prop) ? el.style.getPropertyValue(prop) : value
}

describe(`fixtures/interactions-round1.json fieldInk`, () => {
  const theme = builtinTheme(fx.theme)!
  const palette = theme.modes[fx.mode].color as Record<string, string>
  const hex = (css: string): string => {
    const m = /^var\(--xui-color-([A-Za-z0-9]+)\)$/.exec(css)
    if (m) return palette[m[1]!]!
    const rgb = /^rgb\((\d+), (\d+), (\d+)\)$/.exec(css)
    return rgb ? `#${rgb.slice(1).map((n) => Number(n).toString(16).padStart(2, `0`)).join(``)}` : css
  }
  for (const c of fx.cases) {
    it(`fieldInk: ${c.name}`, () => {
      const { container } = render(() => <ExponentialSurface surface={createSurface({ surfaceId: `fi`, data: {}, initial: { id: `root`, component: `Stack`, children: [c.node] } as NestedNode })} theme={fx.theme} id="fi" />)
      const input = container.querySelector<HTMLElement>(`[data-xui-id="f"] input, [data-xui-id="f"] textarea`)!
      expect(input, c.name).toBeTruthy()
      const row = input.classList.contains(`xui-Input-rowfield`)
      const field = row ? input.closest<HTMLElement>(`.xui-Input-field`)! : input
      const props = nativeRecipeProps(c.node.component, (c.node.props ?? {}) as Record<string, unknown>)
      const recipe = resolveRecipe(theme, { component: c.node.component, part: `field`, props }, fx.mode)
      const painted = cascaded(field, `color`)
      expect(painted ? hex(painted) : String(recipe.color), `${c.name}: value`).toBe(c.value)
      if (row) expect(cascaded(input, `color`), `${c.name}: the row's value tone`).toBe(`color-mix(in srgb, currentcolor ${Math.round((c.valueOpacity ?? 1) * 100)}%, transparent)`)
      const authored = c.node.style?.color !== undefined
      const pinned = row ? `color-mix(in srgb, var(--xui-color-foreground) 70%, transparent)` : `var(--xui-color-foreground)`
      expect(cascaded(input, `caret-color`), `${c.name}: caret`).toBe(authored ? pinned : ``)
      expect(String(recipe.color), `${c.name}: caret ink`).toBe(c.caret)
      const placeholder = resolveRecipe(theme, { component: c.node.component, part: `placeholder`, props }, fx.mode)
      expect(String(placeholder.color), `${c.name}: placeholder`).toBe(c.placeholder)
      const sheets = Array.from(document.querySelectorAll(`style`)).map((s) => s.textContent ?? ``).join(``)
      expect(/xui-n-f [^{]*::placeholder/.test(sheets), `${c.name}: no node rule on the placeholder`).toBe(false)
    })
  }
})
