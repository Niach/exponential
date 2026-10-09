// VAPP-87: the theme → CSS compiler.

import { describe, expect, it } from "vitest"
import { builtinTheme, loadTheme, BUILTIN_THEMES } from "@exponential-at/ui"
import { compileTheme, cssValue, declarations, ruleSelectors, themeScope, tokenVar } from "./theme-css"
import { nodeSheet } from "./box-css"

describe(`compileTheme`, () => {
  const neutral = compileTheme(builtinTheme(`neutral`))
  it(`scopes by id + content hash and emits both modes`, () => {
    expect(neutral.scope).toMatch(/^xui-t-neutral-[a-z0-9]+$/)
    expect(neutral.css).toContain(`.xui-surface.${neutral.scope}[data-xui-mode="light"]{`)
    expect(neutral.css).toContain(`.xui-surface.${neutral.scope}[data-xui-mode="dark"]{`)
    expect(neutral.css).toContain(`--xui-color-primary:#`)
    expect(neutral.css).toContain(`--primary:var(--xui-color-primary)`)
    expect(neutral.css).toContain(`--glass-fill-card:var(--xui-color-card)`)
    expect(neutral.css).toContain(`--xui-spacing-md:12px`)
    expect(neutral.css).toContain(`--xui-font-sans:"Geist", ui-sans-serif, system-ui, sans-serif`)
  })
  it(`two themes with one id but different content get two scopes; the same theme the same`, () => {
    const a = loadTheme({ id: `brand`, name: `Brand`, extends: `neutral`, modes: { light: { color: { primary: `#2563eb` } } } }, { themes: BUILTIN_THEMES })
    const b = loadTheme({ id: `brand`, name: `Brand`, extends: `neutral`, modes: { light: { color: { primary: `#dc2626` } } } }, { themes: BUILTIN_THEMES })
    expect(themeScope(a)).not.toBe(themeScope(b))
    expect(themeScope(a)).not.toBe(neutral.scope)
    expect(themeScope(builtinTheme(`neutral`))).toBe(neutral.scope)
  })
  it(`recipe rules become part classes with prop and state selectors, in the right layer`, () => {
    const recipe = neutral.css.slice(neutral.css.indexOf(`@layer xui-recipe{`), neutral.css.indexOf(`@layer xui-part{`))
    const part = neutral.css.slice(neutral.css.indexOf(`@layer xui-part{`))
    expect(recipe).toContain(`.${neutral.scope}.xui-Button-root{`)
    expect(recipe).toContain(`.${neutral.scope}.xui-Button-root[data-r-variant="outline"]{`)
    expect(recipe).toContain(`.${neutral.scope}.xui-Button-root[data-r-variant="outline"]:hover,.${neutral.scope}.xui-Button-root[data-r-variant="outline"][data-xs~="hover"]{`)
    // `checked` is a recipe PROP of Switch (not a state): an attribute.
    expect(recipe).toContain(`.${neutral.scope}.xui-Switch-track[data-r-checked="true"]{`)
    expect(neutral.css).toContain(`--xui-type-lineHeight-sm:20px`)
    expect(recipe).toContain(`line-height:var(--xui-type-lineHeight-sm)`)
    // Macro parts: data-m-*, the part layer.
    expect(part).toContain(`.${neutral.scope}.xui-Badge-label[data-m-variant="secondary"]{color:var(--xui-color-secondaryForeground)`)
    expect(recipe).not.toContain(`xui-Badge-`)
    // Values are variables, borders get a style.
    expect(recipe).toContain(`border-width:var(--xui-border-hairline);border-color:var(--xui-color-input)`)
    expect(recipe).toContain(`border-style:solid`)
    expect(neutral.css.startsWith(`@layer xui-base, xui-recipe, xui-node, xui-part;`)).toBe(true)
  })
  it(`the playful theme restyles the same classes`, () => {
    const playful = compileTheme(builtinTheme(`playful`))
    expect(playful.css).toContain(`.${playful.scope}.xui-Button-root{`)
    expect(playful.css).toContain(`border-radius:var(--xui-radius-full)`)
    expect(playful.css).toContain(`--xui-font-sans:"Nunito", Avenir Next, Segoe UI, sans-serif`)
  })
})

describe(`values and selectors`, () => {
  it(`tokens → variables, numbers → px (unitless where CSS is), overflow hidden → clip`, () => {
    expect(tokenVar(`$color.primary`)).toBe(`var(--xui-color-primary)`)
    expect(tokenVar(`$type.family.mono`)).toBe(`var(--xui-font-mono)`)
    expect(tokenVar(`$type.size.sm`)).toBe(`var(--xui-type-size-sm)`)
    expect(cssValue(`gap`, 8)).toBe(`8px`)
    expect(cssValue(`flexGrow`, 1)).toBe(`1`)
    expect(cssValue(`opacity`, 0.5)).toBe(`0.5`)
    expect(cssValue(`overflow`, `hidden`)).toBe(`clip`)
    expect(cssValue(`gridTemplateAreas`, [`nav main`, `nav footer`])).toBe(`"nav main" "nav footer"`)
    expect(cssValue(`boxShadow`, [{ x: 0, y: 1, blur: 2, spread: 0, color: `#0000000d` }])).toBe(`0px 1px 2px 0px #0000000d`)
    expect(cssValue(`native`, true)).toBeNull()
  })
  it(`shorthands expand to physical sides`, () => {
    expect(declarations({ paddingHorizontal: `$spacing.md`, marginVertical: 4 })).toEqual([
      [`padding-left`, `var(--xui-spacing-md)`],
      [`padding-right`, `var(--xui-spacing-md)`],
      [`margin-top`, `4px`],
      [`margin-bottom`, `4px`],
    ])
  })
  it(`multi-state rules take the cross product of the state selectors`, () => {
    const sel = ruleSelectors(`.s.xui-Tabs-tab`, { when: { state: [`hover`, `selected`] }, style: {} })
    expect(sel).toContain(`.s.xui-Tabs-tab:hover[data-state="active"]`)
    expect(sel).toContain(`.s.xui-Tabs-tab[data-xs~="hover"][data-xs~="selected"]`)
    expect(sel.length).toBe(2 * 4)
  })
})

describe(`nodeSheet`, () => {
  it(`per node: base rule, @media blocks in SOURCE order as container queries, then the states`, () => {
    const css = nodeSheet(
      {
        id: `root`,
        component: `Box`,
        props: {},
        style: { display: `grid`, gap: `$spacing.md`, "@media (min-width: 900px)": { gap: 24 }, "@media (min-width: 600px)": { gridTemplateColumns: `1fr 2fr` } },
        children: [{ id: `b`, component: `Button`, props: {}, style: { flexShrink: 0, ":pressed": { opacity: 0.6 } }, children: [] }],
      },
      `s1`
    )
    expect(css).toBe(
      `@layer xui-node{.xui-s-s1 .xui-n-root{display:grid;gap:var(--xui-spacing-md)}@container xui (width >= 900px){.xui-s-s1 .xui-n-root{gap:24px}}@container xui (width >= 600px){.xui-s-s1 .xui-n-root{grid-template-columns:1fr 2fr}}.xui-s-s1 .xui-n-b{flex-shrink:0}.xui-s-s1 .xui-n-b:active,.xui-s-s1 .xui-n-b[data-xs~="pressed"]{opacity:0.6}}`
    )
  })
})
