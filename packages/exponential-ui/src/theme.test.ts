// VAPP-92: the theme loader, the resolver and the built-ins.

import { describe, expect, test } from "bun:test"
import { CORE_CATALOG_ID } from "./catalog"
import { reduceNested } from "./reducer"
import { RECIPE_KEYS, RECIPE_STATES, recipeParts } from "./recipes"
import { THEME_SCHEMA_ID, ThemeError, isFontText, loadTheme, nodeRecipeQuery, resolveNodeStyle, resolveRecipe, resolveToken, tryLoadTheme, validateTheme } from "./theme"
import { BUILTIN_THEMES, BUILTIN_THEME_IDS, builtinTheme, builtinThemes } from "./themes"
import { STYLE_VISUAL_KEYS } from "./style"
import kitchenSink from "../fixtures/kitchen-sink.json" with { type: "json" }
import type { NestedNode, UiNode } from "./types"

describe(`built-in themes`, () => {
  test(`the three built-ins load, neutral is the root`, () => {
    expect(BUILTIN_THEME_IDS).toEqual([`neutral`, `exponential`, `playful`])
    for (const theme of builtinThemes()) {
      expect(theme.chain[0]).toBe(`neutral`)
      expect(Object.keys(theme.modes.light.color).length).toBe(Object.keys(theme.modes.dark.color).length)
    }
    expect(builtinTheme(`playful`).chain).toEqual([`neutral`, `playful`])
    expect(builtinTheme(`exponential`).chain).toEqual([`neutral`, `exponential`])
  })

  test(`every built-in validates against the contract`, () => {
    for (const source of BUILTIN_THEMES) expect(validateTheme(source, { themes: BUILTIN_THEMES })).toEqual([])
  })

  test(`the exponential theme carries the app's values`, () => {
    const t = builtinTheme(`exponential`)
    expect(t.tokens.spacing.md).toBe(12)
    expect(t.tokens.radius.lg).toBe(12)
    expect(t.tokens.type.family.sans).toBe(`Inter`)
    expect(t.tokens.control.buttonMd).toBe(32)
    expect(t.modes.dark.color.background).toBe(`#0a0a0a`)
    expect(t.modes.light.color.background).toBe(`#ffffff`)
    expect(t.modes.dark.color.card).toMatch(/^#ffffff[0-9a-f]{2}$/)
  })

  test(`playful differs from neutral where it says so and inherits the rest`, () => {
    const neutral = builtinTheme(`neutral`)
    const playful = builtinTheme(`playful`)
    expect(playful.tokens.radius.md).not.toBe(neutral.tokens.radius.md)
    expect(playful.tokens.spacing).toEqual(neutral.tokens.spacing)
    expect(playful.tokens.type.family.sans).toBe(`Nunito`)
    const button = (id: string) => resolveRecipe(builtinTheme(id), { component: `Button`, part: `root`, props: { variant: `default`, size: `default` } }, `light`)
    expect(button(`playful`).borderRadius).toBe(9999)
    expect(button(`neutral`).borderRadius).toBe(8)
    expect(button(`playful`).backgroundColor).toBe(`#7c3aed`)
  })
})

describe(`recipe contract`, () => {
  test(`recipe keys are a subset of the Box visual keys plus the box keys and native`, () => {
    const allowed = new Set([...STYLE_VISUAL_KEYS, `padding`, `paddingHorizontal`, `paddingVertical`, `gap`, `width`, `height`, `minWidth`, `minHeight`, `native`])
    for (const key of RECIPE_KEYS) expect(allowed.has(key), key).toBe(true)
    expect(RECIPE_STATES).toContain(`hover`)
  })

  test(`every component has parts; macro parts come from the templates`, () => {
    const parts = recipeParts()
    expect(parts.Badge.parts).toEqual([`root`, `icon`, `label`])
    expect(parts.Badge.props).toEqual([`variant`])
    expect(parts.Meter.props).toEqual([`tone`])
    expect(parts.Row.props).toEqual([`selected`, `density`, `surface`, `depth`])
    expect(parts.Section.props).toEqual([`tint`, `tree`, `open`, `collapsible`])
    expect(parts.Switch.parts).toContain(`track`)
    expect(parts.Button.props).toEqual([`variant`, `size`, `disabled`, `loading`])
    expect(Object.keys(parts).length).toBe(72)
    // Round 1: a template-built slot part (AlertDialog's footer) and painter-supplied discriminators.
    expect(parts.AlertDialog.parts).toEqual([`root`, `footer`, `cancel`, `confirm`])
    expect(parts.CodeBlock.props).toContain(`kind`)
    expect(parts.Stepper.props).toEqual([`orientation`, `status`])
  })
})

describe(`resolution`, () => {
  const neutral = builtinTheme(`neutral`)

  test(`tokens resolve per mode`, () => {
    expect(resolveToken(neutral, `$color.primary`, `light`)).toBe(`#171717`)
    expect(resolveToken(neutral, `$color.primary`, `dark`)).toBe(`#e5e5e5`)
    expect(resolveToken(neutral, `$spacing.md`, `light`)).toBe(12)
    expect(resolveToken(neutral, `$type.family.sans`, `light`)).toBe(`Geist`)
    expect(resolveToken(neutral, `$shadow.none`, `light`)).toEqual([])
    expect(resolveToken(neutral, `nope`, `light`)).toBeUndefined()
  })

  test(`rules merge in order; variant, size and state compose`, () => {
    const q = (props: Record<string, unknown>, states: string[] = []) => resolveRecipe(neutral, { component: `Button`, part: `root`, props, states }, `light`)
    expect(q({ variant: `default`, size: `default` })).toMatchObject({ height: 36, backgroundColor: `#171717`, color: `#fafafa`, borderRadius: 8 })
    expect(q({ variant: `outline`, size: `sm` })).toMatchObject({ height: 32, borderWidth: 1, borderColor: `#e5e5e5`, backgroundColor: `#ffffff` })
    expect(q({ variant: `outline`, size: `sm` }, [`hover`])).toMatchObject({ backgroundColor: `#f5f5f5` })
    expect(q({ variant: `ghost`, size: `icon` })).toMatchObject({ width: 36, height: 36, backgroundColor: `#00000000`, boxShadow: [] })
    expect(q({ variant: `default`, size: `default` }, [`disabled`]).opacity).toBe(0.5)
    expect(q({ variant: `default`, size: `default` }, [`hover`]).opacity).toBe(0.9)
  })

  test(`a state rule needs every listed state; a list value matches any`, () => {
    const style = resolveRecipe(neutral, { component: `Badge`, part: `icon`, props: { variant: `outline` } }, `dark`)
    expect(style.color).toBe(`#fafafa`)
  })

  test(`node precedence: native recipe < node style < macro part recipe`, () => {
    const badge: NestedNode = { id: `b`, component: `Badge`, props: { text: `New`, variant: `destructive` } }
    const root = reduceNested(badge, { catalogId: CORE_CATALOG_ID }).root
    const label = root.children.find((n) => n.id === `b.label`) as UiNode
    expect(nodeRecipeQuery(label)).toEqual({ component: `Badge`, part: `label`, props: { variant: `destructive` } })
    const style = resolveNodeStyle(neutral, label, `light`)
    // Text/root caption gives the size, Badge/label the colour and weight.
    expect(style.fontSize).toBe(12)
    expect(style.color).toBe(neutral.modes.light.color.destructiveForeground)
    expect(style.fontWeight).toBe(500)
    // The root: template structure + the Badge/root recipe's fill.
    const rootStyle = resolveNodeStyle(neutral, root, `light`)
    expect(rootStyle.backgroundColor).toBe(`#e7000b`)
    expect(rootStyle.height).toBe(20)
    expect(rootStyle.display).toBe(`flex`)
  })

  test(`an author's style on a plain native wins over its recipe`, () => {
    const text: NestedNode = { id: `t`, component: `Text`, props: { text: `x`, variant: `muted` }, style: { color: `#ff0000` } }
    const root = reduceNested(text, { catalogId: CORE_CATALOG_ID }).root
    expect(resolveNodeStyle(neutral, root, `light`).color).toBe(`#ff0000`)
  })

  test(`every kitchen-sink node resolves under every built-in without a leftover token`, () => {
    const root = reduceNested(kitchenSink as unknown as NestedNode, { catalogId: CORE_CATALOG_ID }).root
    const walk = (n: UiNode, f: (n: UiNode) => void) => { f(n); n.children.forEach((c) => walk(c, f)); Object.values(n.slots ?? {}).forEach((s) => walk(s, f)) }
    let count = 0
    for (const theme of builtinThemes()) {
      for (const mode of [`light`, `dark`] as const) {
        walk(root, (n) => {
          const style = resolveNodeStyle(theme, n, mode)
          count++
          for (const [k, v] of Object.entries(style)) {
            if (typeof v === `string` && v.startsWith(`$`)) throw new Error(`${theme.id}/${mode} ${n.id}.${k} = ${v}`)
          }
        })
      }
    }
    expect(count).toBeGreaterThan(1000)
  })
})

describe(`extends`, () => {
  test(`a theme that changes primary and the button radius works everywhere`, () => {
    const brand = loadTheme(
      { $schema: THEME_SCHEMA_ID, id: `brand`, name: `Brand`, extends: `neutral`, modes: { light: { color: { primary: `#2563eb` } } }, recipes: { Button: { root: [{ style: { borderRadius: `$radius.full` } }] } } },
      { themes: BUILTIN_THEMES }
    )
    expect(brand.chain).toEqual([`neutral`, `brand`])
    expect(brand.modes.light.color.primary).toBe(`#2563eb`)
    expect(brand.modes.dark.color.primary).toBe(`#e5e5e5`)
    expect(brand.modes.light.color.secondary).toBe(`#f5f5f5`)
    const button = resolveRecipe(brand, { component: `Button`, part: `root`, props: { variant: `default`, size: `sm` } }, `light`)
    expect(button).toMatchObject({ backgroundColor: `#2563eb`, borderRadius: 9999, height: 32 })
    expect(resolveRecipe(brand, { component: `Badge`, part: `root`, props: { variant: `default` } }, `light`).backgroundColor).toBe(`#2563eb`)
    expect(resolveRecipe(brand, { component: `Text`, part: `root`, props: { variant: `title`, tone: `primary` } }, `light`).color).toBe(`#2563eb`)
  })

  test(`chains of chains and resolved parents`, () => {
    const playful = builtinTheme(`playful`)
    const child = loadTheme({ $schema: THEME_SCHEMA_ID, id: `c`, name: `C`, extends: `playful`, tokens: { spacing: { md: 20 } } }, { themes: [playful] })
    expect(child.chain).toEqual([`neutral`, `playful`, `c`])
    expect(child.tokens.spacing.md).toBe(20)
    expect(child.tokens.radius.md).toBe(14)
  })
})

describe(`errors`, () => {
  test(`an invalid theme fails with every issue listed, never a crash`, () => {
    expect(() => loadTheme(null, { themes: BUILTIN_THEMES })).toThrow(ThemeError)
    expect(() => loadTheme(42)).toThrow(/expected a theme object/)
    const bad = { $schema: THEME_SCHEMA_ID, id: `bad`, name: `Bad`, extends: `neutral`, tokens: { spacing: { huge: 1 } }, recipes: { Button: { root: [{ style: { display: `flex` } }] } } }
    try {
      loadTheme(bad, { themes: BUILTIN_THEMES })
      throw new Error(`loaded`)
    } catch (error) {
      expect(error).toBeInstanceOf(ThemeError)
      const e = error as ThemeError
      expect(e.issues.map((i) => i.path)).toEqual([`tokens.spacing.huge`, `recipes.Button.root[0].style.display`])
      expect(e.message).toContain(`Theme "bad" is invalid:`)
      expect(e.message).toContain(`not a recipe key`)
    }
    expect(tryLoadTheme({ $schema: THEME_SCHEMA_ID, id: `x`, name: `X`, extends: `nope` }, { themes: BUILTIN_THEMES }).issues[0].message).toContain(`unknown theme "nope"`)
    expect(validateTheme(`neutral`)).toHaveLength(1)
  })

  test(`font text that could break out of a stylesheet is refused (F50)`, () => {
    const withFamily = (sans: string, fallback?: string) => {
      const source = JSON.parse(JSON.stringify(BUILTIN_THEMES[0])) as Record<string, any>
      source.tokens.type.family.sans = sans
      source.fonts = { ...source.fonts, [sans]: { fallback: fallback ?? `sans-serif` } }
      return source
    }
    const paths = (source: unknown) => validateTheme(source, { themes: BUILTIN_THEMES }).map((i) => `${i.path}: ${i.message}`)
    expect(paths(withFamily(`Inter`))).toEqual([])
    expect(paths(withFamily(`Segoe UI`, `"Helvetica Neue", 'Noto Sans', sans-serif`))).toEqual([])
    for (const bad of [`a"}`, `Inter; background:url(x)`, `Inter}body{color:red`, `Inter\\`, `"Inter`, `Inter(1)`]) {
      const issues = paths(withFamily(bad))
      expect(issues.some((i) => i.startsWith(`tokens.type.family.sans: expected a font family name: letters`))).toBe(true)
    }
    for (const bad of [`red;background:url(x)`, `sans-serif}`, `'Noto Sans, serif`, ``]) {
      const issues = paths(withFamily(`Inter`, bad))
      expect(issues).toContain(`fonts.Inter.fallback: expected a font fallback list: letters, digits, spaces, hyphens, commas and balanced quotes only`)
    }
    expect(isFontText(`Comic Neue`)).toBe(true)
    expect(isFontText(`ui-monospace, SFMono-Regular, Menlo, monospace`)).toBe(true)
    expect(isFontText(`a"b"c`)).toBe(true)
    expect(isFontText(`a"b`)).toBe(false)
    expect(isFontText(42)).toBe(false)
  })

  test(`a root theme must give every token a value`, () => {
    const { theme, issues } = tryLoadTheme({ $schema: THEME_SCHEMA_ID, id: `bare`, name: `Bare`, modes: {}, tokens: {}, recipes: {} })
    expect(theme).toBeNull()
    expect(issues.length).toBeGreaterThan(100)
    expect(issues[0].message).toMatch(/missing value for \$color\./)
  })
})
