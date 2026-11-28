import { gunzipSync } from "node:zlib"
import { pruneUtilities } from "./css"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { INTER_SYMBOLS_FONT_FACES } from "../../../exponential-ui/src/web/inter-symbols.generated"
import { INTER_SYMBOLS_FACES, MCP_APP_MAX_BYTES, interSymbolsAsFiles, interSymbolsFile, jsonModule, namespaceJsonImports, packedModule } from "./lean"

describe(`#NEW-140 the views' build`, () => {
  it(`keeps every view's budget under OpenClaw's 2 MiB resource cap`, () => {
    for (const bytes of Object.values(MCP_APP_MAX_BYTES)) expect(bytes).toBeLessThanOrEqual(2 * 1024 * 1024 - 64 * 1024)
  })

  const read = (path: string) => readFileSync(join(__dirname, path))
  it(`the Inter Symbols faces load the app's /fonts/ files, byte-equal to genUI's`, () => {
    const code = interSymbolsAsFiles(read(`../../../exponential-ui/src/web/inter-symbols.generated.ts`).toString())
    expect(code).not.toContain(`data:font`)
    for (const face of INTER_SYMBOLS_FACES) {
      expect(code).toContain(`font-weight:${face.weight};src:url(/fonts/${interSymbolsFile(face.style)}) format("woff2");unicode-range:`)
      expect(read(`../../../../apps/web/public/fonts/${interSymbolsFile(face.style)}`).equals(read(`../../../exponential-ui/fonts/${interSymbolsFile(face.style)}`))).toBe(true)
    }
    expect(INTER_SYMBOLS_FONT_FACES).toHaveLength(INTER_SYMBOLS_FACES.length)
  })

  it(`a fixture read by members only becomes a namespace import`, () => {
    const code = `import fixture from "../f.json" with { type: "json" }\nexport const COPY = fixture.copy\nexport const o = { fixture: 1 }\nexport const p = o.fixture\n`
    expect(namespaceJsonImports(code)).toContain(`import * as fixture from "../f.json"`)
  })
  it(`a fixture read whole, by a computed key, its "default" or shorthand keeps its default import`, () => {
    for (const use of [`export const F = fixture`, `export const d = fixture.default`, `export const a = fixture["copy"]`, `export const o = { fixture }`]) {
      const code = `import fixture from "../f.json"\n${use}\n`
      expect(namespaceJsonImports(code)).toBe(code)
    }
  })
  it(`a packed table keeps its default import`, () => {
    const code = `import core from "../generated/core.runtime.json"\nexport const c = core.components\n`
    expect(namespaceJsonImports(code)).toBe(code)
  })

  it(`a JSON module exports its identifier members, without the spec's prose`, async () => {
    const code = jsonModule(`{"$comment":"prose","copy":{"a":1},"a-b":2}`)
    expect(code).not.toContain(`prose`)
    const mod = (await import(/* @vite-ignore */ `data:text/javascript,${encodeURIComponent(code)}`)) as Record<string, unknown>
    expect(mod.copy).toEqual({ a: 1 })
    expect(mod.default).toEqual({ copy: { a: 1 }, "a-b": 2 })
  })
  it(`a packed table is its own JSON`, () => {
    const packed = /atob\("([^"]+)"\)/.exec(packedModule(`{ "a": "ä" }`))![1]
    expect(JSON.parse(gunzipSync(Buffer.from(packed, `base64`)).toString())).toEqual({ a: `ä` })
  })

  it(`drops the utilities the script never names, and nothing else`, () => {
    const css = `@layer base{.gone{a:b}}@layer utilities{.p-2{padding:8px}.p-3{padding:12px}.hover\\:x:hover{a:b}@media (width>=1px){.sm\\:p-2{a:b}.sm\\:p-9{a:b}}.m-1,.m-2{a:b}:is(.q){a:b}.\\32xl\\:p{a:"}"}}.after{a:b}`
    expect(pruneUtilities(css, `"p-2 sm:p-2 m-2 2xl:p"`)).toBe(`@layer base{.gone{a:b}}@layer utilities{.p-2{padding:8px}@media (width>=1px){.sm\\:p-2{a:b}}.m-1,.m-2{a:b}:is(.q){a:b}.\\32xl\\:p{a:"}"}}.after{a:b}`)
  })
})
