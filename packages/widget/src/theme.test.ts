// EXP-435 theme engine: palette selection, mode resolution, and the
// buttonCss var-fallback contract the loader FAB depends on.
import { afterEach, describe, expect, it, vi } from "vitest"
import {
  buttonCss,
  darkPalette,
  lightPalette,
  paletteFor,
  pickForeground,
  resolveThemeMode,
  resolveThemePreference,
  theme,
} from "./theme"

afterEach(() => {
  vi.unstubAllGlobals()
})

describe(`resolveThemePreference`, () => {
  it(`first valid candidate wins, junk falls through to dark`, () => {
    expect(resolveThemePreference(null, undefined, `light`)).toBe(`light`)
    expect(resolveThemePreference(`auto`, `light`)).toBe(`auto`)
    expect(resolveThemePreference(`LIGHT`, 7, {})).toBe(`dark`)
    expect(resolveThemePreference()).toBe(`dark`)
  })
})

describe(`resolveThemeMode`, () => {
  it(`passes explicit modes through`, () => {
    expect(resolveThemeMode(`dark`)).toBe(`dark`)
    expect(resolveThemeMode(`light`)).toBe(`light`)
  })

  it(`auto follows prefers-color-scheme`, () => {
    vi.stubGlobal(
      `matchMedia`,
      vi.fn(() => ({ matches: true }))
    )
    expect(resolveThemeMode(`auto`)).toBe(`light`)
    vi.stubGlobal(
      `matchMedia`,
      vi.fn(() => ({ matches: false }))
    )
    expect(resolveThemeMode(`auto`)).toBe(`dark`)
  })

  it(`auto degrades to dark without matchMedia`, () => {
    vi.stubGlobal(`matchMedia`, undefined)
    expect(resolveThemeMode(`auto`)).toBe(`dark`)
  })
})

describe(`paletteFor`, () => {
  it(`returns the stock palettes`, () => {
    expect(paletteFor(`dark`)).toBe(darkPalette)
    expect(paletteFor(`light`)).toBe(lightPalette)
    // The back-compat alias stays the dark palette.
    expect(theme).toBe(darkPalette)
  })
})

describe(`buttonCss`, () => {
  it(`embeds literal fallbacks for the loader's rootless shadow tree`, () => {
    const css = buttonCss(`#336699`, `dark`)
    expect(css).toContain(`var(--exp-accent, #336699)`)
    expect(css).toContain(
      `var(--exp-accent-foreground, ${pickForeground(`#336699`)})`
    )
    expect(css).toContain(`var(--exp-border, ${darkPalette.border})`)
  })

  it(`light mode swaps the fallback palette`, () => {
    const css = buttonCss(`#336699`, `light`)
    expect(css).toContain(`var(--exp-border, ${lightPalette.border})`)
    expect(css).toContain(
      `var(--exp-foreground, ${lightPalette.foreground})`
    )
  })

  it(`carries the edge-tab rules (EXP-569)`, () => {
    const css = buttonCss(`#336699`, `dark`)
    expect(css).toContain(`button.exp-fab.exp-tab {`)
    // The tab must never lift off the edge on hover.
    expect(css).toMatch(/exp-tab:hover \{\n {2}transform: none;/)
    // Inner-side-only rounding, edge side borderless.
    expect(css).toContain(
      `button.exp-tab-right { border-radius: 10px 0 0 10px; border-right: none; }`
    )
    expect(css).toContain(
      `button.exp-tab-left { border-radius: 0 10px 10px 0; border-left: none; }`
    )
  })

  // EXP-642: the tab shows on desktop too now, so it nudges at 36px rather
  // than matching the FAB's bulk.
  it(`sizes the edge tab below the fab`, () => {
    const css = buttonCss(`#336699`, `dark`)
    expect(css).toMatch(/exp-tab \{\n {2}width: 36px;\n {2}height: 36px;/)
    expect(css).toMatch(/exp-tab:hover \{[\s\S]*?width: 42px;/)
    expect(css).toContain(
      `button.exp-fab.exp-tab svg { width: 16px; height: 16px; }`
    )
  })
})
