// Inter Symbols, the key-symbol fallback after every theme family: ONE glyph
// list (fonts/inter-symbols.json) behind the subset build, the web recipes,
// SwiftUI, Compose, gpui and the conformance font set. Any drift fails here;
// rebuild with `uvx --from 'fonttools[woff]==4.69.1' python -I
// packages/exponential-ui/fonts/build.py`.

import { describe, expect, it } from "bun:test"
import { createHash } from "node:crypto"
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { builtinTheme, INTER_SYMBOLS_FAMILY } from "@exponential_app/eui"
import { INTER_SYMBOLS_FONT_FACES, INTER_SYMBOLS_WEIGHTS } from "./inter-symbols.generated"
import { fontStack } from "./font-stack"
import { compileTheme } from "./theme-css"

const repo = join(import.meta.dir, `..`, `..`, `..`, `..`)
const read = (rel: string): string => readFileSync(join(repo, rel), `utf8`)
const sha256 = (rel: string): string => createHash(`sha256`).update(readFileSync(join(repo, rel))).digest(`hex`)

interface Spec {
  family: string
  codepoints: { cp: string; char: string }[]
  carried?: { cp: string; char: string }[]
  faces: { weight: number; style: string }[]
  codepointsHash: string
  outputs: Record<string, string>
}
const spec = JSON.parse(read(`packages/exponential-ui/fonts/inter-symbols.json`)) as Spec
const cps = spec.codepoints.map((c) => c.cp)
const hex = cps.map((c) => parseInt(c.slice(2), 16))

describe(`inter-symbols.json`, () => {
  it(`names each codepoint by its char`, () => {
    for (const c of [...spec.codepoints, ...(spec.carried ?? [])]) expect(`U+${c.char.codePointAt(0)!.toString(16).toUpperCase().padStart(4, `0`)}`).toBe(c.cp)
    expect(new Set(cps).size).toBe(cps.length)
  })

  it(`the committed fonts were built from this list (else rerun fonts/build.py)`, () => {
    const built = [...cps, ...(spec.carried ?? []).map((c) => c.cp)]
    expect(createHash(`sha256`).update(built.sort().join(`,`)).digest(`hex`)).toBe(spec.codepointsHash)
    expect(Object.keys(spec.outputs).length).toBeGreaterThan(0)
    for (const [rel, hash] of Object.entries(spec.outputs)) {
      expect(existsSync(join(repo, rel))).toBe(true)
      expect(`${rel} ${sha256(rel)}`).toBe(`${rel} ${hash}`)
    }
  })

  it(`every face stays under 10 KB`, () => {
    for (const rel of Object.keys(spec.outputs)) if (/\.(ttf|woff2)$/.test(rel)) expect(readFileSync(join(repo, rel)).length).toBeLessThan(10 * 1024)
  })
})

describe(`the web recipes`, () => {
  it(`inline one @font-face per weight over exactly the list`, () => {
    expect(INTER_SYMBOLS_FAMILY).toBe(spec.family)
    expect<number[]>([...INTER_SYMBOLS_WEIGHTS]).toEqual(spec.faces.map((f) => f.weight))
    expect(INTER_SYMBOLS_FONT_FACES.length).toBe(spec.faces.length)
    for (const rule of INTER_SYMBOLS_FONT_FACES) {
      expect(rule).toContain(`font-family:"${spec.family}"`)
      expect(rule).toContain(`unicode-range:${[...cps].sort().join(`,`)}`)
    }
  })

  it(`put the family right after the theme family, ahead of the OS fallback`, () => {
    expect(fontStack(`Geist`, { Geist: { fallback: `ui-sans-serif, system-ui, sans-serif` } })).toBe(`"Geist", "Inter Symbols", ui-sans-serif, system-ui, sans-serif`)
    const neutral = compileTheme(builtinTheme(`neutral`)).css
    expect(neutral).toContain(`--xui-font-sans:"Geist", "Inter Symbols", ui-sans-serif`)
    for (const rule of INTER_SYMBOLS_FONT_FACES) expect(neutral).toContain(rule)
  })
})

describe(`the native renderers and the conformance font set`, () => {
  const styles = spec.faces.map((f) => f.style)

  it(`conformance/fonts.json loads the SDK's own faces`, () => {
    const fonts = JSON.parse(read(`packages/exponential-ui/conformance/fonts.json`)) as { families: Record<string, { faces?: { file: string; weight: number }[]; unicodeRange?: string }> }
    expect(fonts.families[spec.family]?.unicodeRange).toBe([...cps].sort().join(`,`))
    const faces = fonts.families[spec.family]?.faces ?? []
    expect(faces.map((f) => [f.file, f.weight])).toEqual(spec.faces.map((f) => [`packages/exponential-ui/fonts/InterSymbols-${f.style}.ttf`, f.weight]))
  })

  it(`SwiftUI (TextFonts) names the family and bundles every face`, () => {
    const swift = read(`packages/exponential-ui-swift/Sources/ExponentialUI/Measure/TextMeasurer.swift`)
    expect(swift).toContain(`static let symbolsFamily = "${spec.family}"`)
    expect(swift).toContain(`static let symbolFiles = [${styles.map((s) => `"InterSymbols-${s}"`).join(`, `)}]`)
    expect(read(`packages/exponential-ui-swift/Package.swift`)).toContain(`resources: [.copy("Resources/Fonts")]`)
    for (const s of styles) expect(spec.outputs[`packages/exponential-ui-swift/Sources/ExponentialUI/Resources/Fonts/InterSymbols-${s}.ttf`]).toBeDefined()
  })

  it(`Compose names the family, the list and every face`, () => {
    const kt = read(`packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/measure/InterSymbols.kt`)
    expect(kt).toContain(`const val INTER_SYMBOLS_FAMILY = "${spec.family}"`)
    const list = /intArrayOf\(([^)]*)\)/.exec(kt)?.[1] ?? ``
    expect([...list.matchAll(/0x([0-9A-Fa-f]+)/g)].map((m) => parseInt(m[1]!, 16))).toEqual(hex)
    for (const s of styles) {
      expect(kt).toContain(`R.font.inter_symbols_${s.toLowerCase()}`)
      expect(spec.outputs[`packages/exponential-ui-compose/ui-compose/src/main/res/font/inter_symbols_${s.toLowerCase()}.ttf`]).toBeDefined()
    }
  })

  it(`gpui names the family first in its fallback chain and embeds every face`, () => {
    const rs = read(`apps/desktop/crates/eui-gpui/src/symbol_fonts.rs`)
    expect(rs).toContain(`pub const INTER_SYMBOLS_FAMILY: &str = "${spec.family}";`)
    for (const s of styles) {
      expect(rs).toContain(`assets/fonts/InterSymbols-${s}.ttf`)
      expect(spec.outputs[`apps/desktop/crates/eui-gpui/assets/fonts/InterSymbols-${s}.ttf`]).toBeDefined()
    }
  })
})
