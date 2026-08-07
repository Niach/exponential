// EXP-435 theme engine: palette derivation, mode resolution, and the
// buttonCss var-fallback contract the loader FAB depends on.
import { afterEach, describe, expect, it, vi } from "vitest"
import {
  buttonCss,
  darkPalette,
  lightPalette,
  mixHex,
  paletteFor,
  pickForeground,
  resolveThemeMode,
  resolveThemePreference,
  theme,
} from "./theme"

afterEach(() => {
  vi.unstubAllGlobals()
})

describe(`mixHex`, () => {
  it(`interpolates channels linearly`, () => {
    expect(mixHex(`#000000`, `#ffffff`, 0)).toBe(`#000000`)
    expect(mixHex(`#000000`, `#ffffff`, 1)).toBe(`#ffffff`)
    expect(mixHex(`#000000`, `#ffffff`, 0.5)).toBe(`#808080`)
  })

  it(`returns the base unchanged on non-hex6 input`, () => {
    expect(mixHex(`rgba(0,0,0,.5)`, `#ffffff`, 0.5)).toBe(`rgba(0,0,0,.5)`)
    expect(mixHex(`#ffffff`, `junk`, 0.5)).toBe(`#ffffff`)
  })
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
  it(`returns the stock palettes untouched without overrides`, () => {
    expect(paletteFor(`dark`)).toBe(darkPalette)
    expect(paletteFor(`light`)).toBe(lightPalette)
    // The back-compat alias stays the dark palette.
    expect(theme).toBe(darkPalette)
  })

  it(`a custom dark background derives darker insets and readable shades`, () => {
    const palette = paletteFor(`dark`, { backgroundColor: `#101828` })
    expect(palette.card).toBe(`#101828`)
    // Inset surfaces mix toward black on a dark panel.
    expect(palette.background).toBe(mixHex(`#101828`, `#000000`, 0.5))
    expect(palette.secondary).toBe(
      mixHex(`#101828`, darkPalette.foreground, 0.1)
    )
    // Untouched slots keep their stock values.
    expect(palette.destructive).toBe(darkPalette.destructive)
  })

  it(`a light custom background keeps the panel color for insets`, () => {
    const palette = paletteFor(`light`, { backgroundColor: `#fdf6e3` })
    expect(palette.card).toBe(`#fdf6e3`)
    expect(palette.background).toBe(`#fdf6e3`)
  })

  it(`a custom text color re-derives the muted shade`, () => {
    const palette = paletteFor(`dark`, { textColor: `#ffd700` })
    expect(palette.foreground).toBe(`#ffd700`)
    expect(palette.mutedForeground).toBe(
      mixHex(`#ffd700`, darkPalette.card, 0.35)
    )
  })

  it(`ignores junk overrides`, () => {
    expect(paletteFor(`dark`, { backgroundColor: `red`, textColor: `#fff` }))
      .toBe(darkPalette)
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
})
