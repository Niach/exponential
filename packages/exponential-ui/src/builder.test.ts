// VAPP-92: the builder engine — shadcn/tweakcn import, the minimal diff, export.

import { describe, expect, test } from "bun:test"
import { contrast, parseColor, toHex, toThemeHex } from "./color"
import { diffTheme, exportThemeJson, importShadcnCss, parseShadow, parseThemeJson, themeFromImport } from "./builder"
import { loadTheme, resolveRecipe } from "./theme"
import { BUILTIN_THEMES, builtinTheme } from "./themes"
import { styleToCss } from "./css"

describe(`color`, () => {
  test(`parses every form a globals.css carries`, () => {
    expect(toThemeHex(`oklch(1 0 0)`)).toBe(`#ffffff`)
    expect(toThemeHex(`oklch(0.145 0 0)`)).toBe(`#0a0a0a`)
    expect(toThemeHex(`oklch(1 0 0 / 10%)`)).toBe(`#ffffff1a`)
    expect(toThemeHex(`hsl(0 0% 100%)`)).toBe(`#ffffff`)
    expect(toThemeHex(`0 0% 3.9%`)).toBe(`#0a0a0a`)
    expect(toThemeHex(`222.2 84% 4.9%`)).toBe(`#020817`)
    expect(toThemeHex(`rgb(255 0 0 / 0.5)`)).toBe(`#ff000080`)
    expect(toThemeHex(`#FFF`)).toBe(`#ffffff`)
    expect(toThemeHex(`#ffffffff`)).toBe(`#ffffff`)
    expect(toThemeHex(`var(--primary)`)).toBeNull()
    expect(toThemeHex(42)).toBeNull()
    expect(toHex(parseColor(`#123456`)!)).toBe(`#123456`)
    expect(contrast(parseColor(`#000000`)!, parseColor(`#ffffff`)!)).toBeCloseTo(21, 0)
  })
})

const SHADCN_V4 = `
:root {
  --radius: 0.625rem;
  --background: oklch(1 0 0);
  --foreground: oklch(0.145 0 0);
  --primary: oklch(0.55 0.2 260);
  --primary-foreground: oklch(0.985 0 0);
  --border: oklch(0.922 0 0);
  --sidebar: oklch(0.985 0 0);
  --font-sans: "Inter", ui-sans-serif, system-ui, sans-serif;
  --font-mono: var(--font-geist-mono);
  --shadow-sm: 0 1px 2px 0 hsl(0 0% 0% / 0.05);
}
.dark {
  --background: oklch(0.145 0 0);
  --primary: oklch(0.7 0.15 260);
  --chart-1: oklch(0.488 0.243 264.376);
}
`

describe(`importShadcnCss`, () => {
  test(`maps the colours per mode, the radius ladder, the families and the shadows`, () => {
    const imp = importShadcnCss(SHADCN_V4)
    expect(imp.modes.light?.color.primary).toBe(toThemeHex(`oklch(0.55 0.2 260)`)!)
    expect(imp.modes.light?.color.primaryForeground).toBe(`#fafafa`)
    expect(imp.modes.dark?.color.background).toBe(`#0a0a0a`)
    expect(imp.modes.dark?.color.chart1).toBe(toThemeHex(`oklch(0.488 0.243 264.376)`)!)
    expect(imp.tokens.radius).toEqual({ none: 0, sm: 6, md: 8, lg: 10, xl: 14, xl2: 18, xl3: 22, full: 9999 })
    expect(imp.tokens.type?.family?.sans).toBe(`Inter`)
    expect(imp.fonts.Inter).toEqual({ fallback: `ui-sans-serif, system-ui, sans-serif`, source: `host` })
    expect(imp.modes.light?.shadow?.sm).toEqual([{ x: 0, y: 1, blur: 2, spread: 0, color: `#0000000d` }])
    expect(imp.unmapped).toEqual([`--font-mono`, `--sidebar`])
  })

  test(`a v3 globals.css with bare HSL triplets imports too`, () => {
    const imp = importShadcnCss(`@layer base { :root { --background: 0 0% 100%; --primary: 222.2 47.4% 11.2%; --radius: 0.5rem; } .dark { --background: 222.2 84% 4.9%; } }`)
    expect(imp.modes.light?.color.background).toBe(`#ffffff`)
    expect(imp.modes.dark?.color.background).toBe(`#020817`)
    expect(imp.tokens.radius?.lg).toBe(8)
  })

  test(`the import becomes an extends theme that loads over a built-in`, () => {
    const source = themeFromImport(importShadcnCss(SHADCN_V4), { id: `acme`, name: `Acme`, extends: `neutral` })
    expect(source.modes?.light?.shadow?.none).toEqual([])
    const theme = loadTheme(source, { themes: BUILTIN_THEMES })
    expect(theme.chain).toEqual([`neutral`, `acme`])
    expect(resolveRecipe(theme, { component: `Button`, part: `root`, props: { variant: `default`, size: `default` } }, `light`).backgroundColor).toBe(toThemeHex(`oklch(0.55 0.2 260)`)!)
    expect(theme.tokens.type.family.sans).toBe(`Inter`)
    expect(theme.tokens.type.family.mono).toBe(`Geist Mono`)
  })

  test(`parseShadow`, () => {
    expect(parseShadow(`none`)).toEqual([])
    expect(parseShadow(`0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)`)).toHaveLength(2)
    expect(parseShadow(`inset 0 1px 0 red`)).toBeNull()
  })
})

describe(`diffTheme / export`, () => {
  test(`the smallest extends theme that reproduces a draft`, () => {
    const neutral = builtinTheme(`neutral`)
    const draft = loadTheme({ id: `d`, name: `D`, extends: `neutral`, modes: { dark: { color: { primary: `#60a5fa` } } }, tokens: { radius: { md: 12 }, type: { family: { sans: `Inter` } } }, fonts: { Inter: { source: `host` } }, recipes: { Card: { root: [{ style: { borderWidth: `$border.none` } }] } } }, { themes: [neutral] })
    const minimal = diffTheme(draft, neutral)
    expect(minimal).toEqual({
      id: `d`, name: `D`, extends: `neutral`,
      modes: { dark: { color: { primary: `#60a5fa` } } },
      tokens: { radius: { md: 12 }, type: { family: { sans: `Inter` } } },
      fonts: { Inter: { source: `host` } },
      recipes: { Card: { root: [{ style: { borderWidth: `$border.none` } }] } },
    })
    const again = loadTheme(minimal, { themes: [neutral] })
    expect(JSON.stringify(again)).toBe(JSON.stringify(draft))
    const text = exportThemeJson(minimal)
    expect(text.startsWith(`{\n  "$schema": "https://ui.exponential.at/schemas/theme/v1.json",\n  "id": "d"`)).toBe(true)
    expect(parseThemeJson(text, BUILTIN_THEMES).theme?.id).toBe(`d`)
    expect(parseThemeJson(`{nope`, BUILTIN_THEMES).issues[0].message).toMatch(/not JSON/)
    expect(parseThemeJson(`{"id":"x","name":"X","extends":"neutral","tokens":{"spacing":{"huge":1}}}`, BUILTIN_THEMES).issues[0].path).toBe(`tokens.spacing.huge`)
  })

  test(`diffTheme on an unchanged theme is empty`, () => {
    const neutral = builtinTheme(`neutral`)
    const copy = loadTheme({ id: `same`, name: `Same`, extends: `neutral` }, { themes: [neutral] })
    expect(diffTheme(copy, neutral)).toEqual({ id: `same`, name: `Same`, extends: `neutral` })
  })
})

describe(`styleToCss`, () => {
  test(`maps resolved values to declarations`, () => {
    const neutral = builtinTheme(`neutral`)
    const style = resolveRecipe(neutral, { component: `Button`, part: `root`, props: { variant: `outline`, size: `default` } }, `light`)
    const css = styleToCss(style, neutral.fonts)
    expect(css).toMatchObject({ height: `36px`, "padding-left": `16px`, "padding-right": `16px`, "border-width": `1px`, "border-style": `solid`, "border-color": `#e5e5e5`, "font-weight": `500`, "box-shadow": `0px 1px 2px 0px #0000000d` })
    expect(css[`font-family`]).toBe(`"Geist", ui-sans-serif, system-ui, sans-serif`)
    expect(styleToCss({ display: `flex`, flexDirection: `column`, gap: 8, flexGrow: 1, paddingHorizontal: 12, native: true })).toEqual({ display: `flex`, "flex-direction": `column`, gap: `8px`, "flex-grow": `1`, "padding-left": `12px`, "padding-right": `12px` })
  })
})

describe(`round 1: diffTheme keeps the new groups and the contrast overlays`, () => {
  test(`breakpoints, easings and contrast survive the round trip`, () => {
    const base = builtinTheme(`neutral`)
    const draft = loadTheme({ id: `brand`, name: `Brand`, extends: `neutral`, tokens: { breakpoint: { md: 720 }, ease: { standard: [0.4, 0, 0.2, 1] } }, contrast: { dark: { color: { ring: `#ffff00` } } } }, { themes: BUILTIN_THEMES })
    const diff = diffTheme(draft, base, { id: `brand`, name: `Brand` })
    expect(diff.tokens).toEqual({ breakpoint: { md: 720 }, ease: { standard: [0.4, 0, 0.2, 1] } })
    expect(diff.contrast).toEqual({ dark: { color: { ring: `#ffff00` } } })
  })
})
