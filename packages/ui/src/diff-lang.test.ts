// EXP-895: the diff view's private highlighter. Not barrel-exported, so this is
// the only thing keeping the extension table and the fallbacks honest.
import { describe, expect, it } from "vitest"
import { EXT_TO_LANG, highlightLine, languageFor } from "./diff-lang"

describe(`languageFor`, () => {
  it(`maps a path's extension to a registered grammar`, () => {
    expect(languageFor(`apps/web/src/lib/diff.ts`)).toBe(`typescript`)
    expect(languageFor(`Component.tsx`)).toBe(`typescript`)
    expect(languageFor(`crates/ui/src/diff.rs`)).toBe(`rust`)
    expect(languageFor(`Domain/Diff.swift`)).toBe(`swift`)
    expect(languageFor(`ui/Screen.kt`)).toBe(`kotlin`)
  })

  it(`knows Makefile by NAME, case-insensitively`, () => {
    expect(languageFor(`Makefile`)).toBe(`makefile`)
    expect(languageFor(`build/makefile`)).toBe(`makefile`)
  })

  it(`answers null for anything it has no grammar for`, () => {
    expect(languageFor(`LICENCE`)).toBeNull()
    expect(languageFor(`assets/logo.png`)).toBeNull()
    // A dotfile is all extension and no stem — never a grammar.
    expect(languageFor(`.gitignore`)).toBeNull()
  })

  it(`every entry of the table is a grammar lowlight actually registered`, () => {
    for (const ext of Object.keys(EXT_TO_LANG)) {
      expect(languageFor(`x.${ext}`), `.${ext} resolves`).not.toBeNull()
    }
  })
})

describe(`highlightLine`, () => {
  it(`splits a line into hljs token nodes`, () => {
    const out = highlightLine(`typescript`, `const x = 1`, `k`)
    expect(Array.isArray(out)).toBe(true)
    expect(JSON.stringify(out)).toContain(`hljs-keyword`)
  })

  it(`returns the empty string untouched`, () => {
    expect(highlightLine(`typescript`, ``, `k`)).toBe(``)
  })

  it(`falls back to plain text for an unknown grammar`, () => {
    expect(highlightLine(`not-a-language`, `hello`, `k`)).toBe(`hello`)
  })
})
