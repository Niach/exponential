// VAPP-92: the theme fixtures ARE the contract — the TS reference resolver
// replays every case byte for byte; the Rust core and the painters replay
// the same files with these test names.

import { describe, expect, test } from "bun:test"
import { loadTheme, resolveRecipe, tryLoadTheme } from "./theme"
import { BUILTIN_THEMES, builtinTheme } from "./themes"
import { checkGeometry, controlGeometry, verifyPainterGeometry } from "./geometry"
import type { PainterOverride } from "./geometry"
import resolvedFixture from "../fixtures/theme-resolved.json" with { type: "json" }
import recipesFixture from "../fixtures/theme-recipes.json" with { type: "json" }
import extendsFixture from "../fixtures/theme-extends.json" with { type: "json" }
import invalidFixture from "../fixtures/theme-invalid.json" with { type: "json" }
import geometryFixture from "../fixtures/control-geometry.json" with { type: "json" }
import type { ModeName, RecipeQuery, ResolvedStyle, ResolvedTheme, ThemeSource } from "./theme-types"

const canon = (v: unknown) => JSON.stringify(v)

describe(`theme-resolved.json`, () => {
  test(`every built-in resolves to the recorded theme byte for byte`, () => {
    const themes = resolvedFixture.themes as unknown as Record<string, ResolvedTheme>
    expect(Object.keys(themes)).toEqual([`neutral`, `exponential`, `playful`])
    for (const [id, expected] of Object.entries(themes)) expect(canon(builtinTheme(id)), id).toBe(canon(expected))
  })
})

describe(`theme-recipes.json`, () => {
  const cases = recipesFixture.cases as unknown as { component: string; part: string; props: Record<string, unknown>; visuals: Record<string, Record<string, Record<string, ResolvedStyle>>> }[]

  test(`every theme × component part × props resolves to the recorded visuals`, () => {
    expect(cases.length).toBeGreaterThan(100)
    for (const c of cases) {
      for (const [themeId, byMode] of Object.entries(c.visuals)) {
        const theme = builtinTheme(themeId)
        for (const [mode, byState] of Object.entries(byMode)) {
          for (const [state, style] of Object.entries(byState)) {
            const states = state === `default` ? [] : [state]
            expect(canon(resolveRecipe(theme, { component: c.component, part: c.part, props: c.props, states }, mode as ModeName)), `${themeId}/${mode}/${state} ${c.component}/${c.part} ${canon(c.props)}`).toBe(canon(style))
          }
        }
      }
    }
  })

  test(`the three themes give the kitchen-sink controls different looks (recipes drive the painters)`, () => {
    const button = cases.find((c) => c.component === `Button` && c.part === `root` && c.props.variant === `default` && c.props.size === `default`)!
    const looks = new Set(Object.values(button.visuals).map((m) => canon(m.light.default)))
    expect(looks.size).toBe(3)
    const card = cases.find((c) => c.component === `Card` && c.part === `root`)!
    expect(card.visuals.playful.light.default.borderWidth).toBe(0)
    expect(card.visuals.neutral.light.default.borderWidth).toBe(1)
  })
})

describe(`theme-extends.json`, () => {
  const cases = extendsFixture.cases as unknown as { name: string; theme: ThemeSource; expected: { chain: string[]; probes: (RecipeQuery & { mode: ModeName; style: ResolvedStyle })[] } }[]

  test(`every extends case loads over the built-ins and answers its probes byte for byte`, () => {
    for (const c of cases) {
      const theme = loadTheme(c.theme, { themes: BUILTIN_THEMES })
      expect(theme.chain, c.name).toEqual(c.expected.chain)
      for (const p of c.expected.probes) {
        expect(canon(resolveRecipe(theme, { component: p.component, part: p.part, props: p.props, states: p.states }, p.mode)), `${c.name}: ${p.component}/${p.part}`).toBe(canon(p.style))
      }
    }
  })
})

describe(`theme-invalid.json`, () => {
  const cases = invalidFixture.cases as unknown as { name: string; theme: unknown; issues: { path: string; message: string }[] }[]

  test(`every invalid theme raises exactly the recorded issues, never a throw`, () => {
    for (const c of cases) {
      const { theme, issues } = tryLoadTheme(c.theme, { themes: BUILTIN_THEMES })
      expect(theme, c.name).toBeNull()
      expect(canon(issues), c.name).toBe(canon(c.issues))
      expect(issues.length, c.name).toBeGreaterThan(0)
      for (const issue of issues) expect(issue.path.length, c.name).toBeGreaterThan(0)
    }
  })
})

describe(`control-geometry.json`, () => {
  const themes = geometryFixture.themes as unknown as Record<string, Record<string, { part: string; cases: Record<string, { props: Record<string, unknown>; geometry: Record<string, number> }> }>>

  test(`every control's box matches the recorded geometry`, () => {
    for (const [themeId, byComponent] of Object.entries(themes)) {
      const theme = builtinTheme(themeId)
      for (const [component, entry] of Object.entries(byComponent)) {
        for (const [name, c] of Object.entries(entry.cases)) {
          expect(canon(controlGeometry(theme, component, c.props)), `${themeId} ${component} ${name}`).toBe(canon(c.geometry))
        }
      }
    }
  })

  test(`a painter override for Switch passes the geometry fixture`, () => {
    // A host-drawn switch: it may paint anything, but measures off the theme.
    const override: PainterOverride = {
      component: `Switch`,
      measure(theme, props) {
        const g = controlGeometry(theme, `Switch`, props)
        return { width: g.width, height: g.height, borderRadius: g.borderRadius, borderWidth: g.borderWidth }
      },
    }
    const cases = Object.values(themes.neutral.Switch.cases)
    for (const themeId of Object.keys(themes)) expect(verifyPainterGeometry(override, builtinTheme(themeId), cases), themeId).toEqual([])
    // One that ignores the theme fails loudly.
    const wrong: PainterOverride = { component: `Switch`, measure: () => ({ width: 51, height: 31 }) }
    const failures = verifyPainterGeometry(wrong, builtinTheme(`neutral`), cases)
    expect(failures.length).toBe(cases.length)
    expect(failures[0].issues.map((i) => i.key)).toEqual([`width`, `height`, `borderRadius`])
    expect(checkGeometry({ height: 20 }, { height: 20.4 })).toEqual([])
  })
})
