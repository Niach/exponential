// VAPP-87 — compiles src/primitives/tailwind.css into the stylesheet the
// package ships (`@exponential-at/ui-react/styles.css` →
// dist/exponential-ui-react.css). Same wiring as @exp/ui's `compileUiCss`
// (packages/ui/src/island.ts): `compile` resolves the imports and the
// `@source` globs, the oxide `Scanner` collects the candidates from the
// primitives' sources, `optimize` lowers + minifies.
//
//   bun run --filter @exponential-at/ui-react build:css

import { mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { compile, optimize } from "@tailwindcss/node"
import { Scanner } from "@tailwindcss/oxide"

const packageRoot = join(import.meta.dirname, `..`)
const base = join(packageRoot, `src/primitives`)
const outFile = join(packageRoot, `dist/exponential-ui-react.css`)

const compiler = await compile(`@import "./tailwind.css";`, {
  base,
  shouldRewriteUrls: true,
  onDependency() {},
})
const root =
  compiler.root === `none`
    ? []
    : compiler.root === null
      ? [{ base, pattern: `**/*`, negated: false }]
      : [{ ...compiler.root, negated: false }]
const scanner = new Scanner({ sources: [...root, ...compiler.sources] })
const css = compiler.build(scanner.scan())
const { code } = optimize(css, { minify: true })

if (!code.trim()) throw new Error(`build-css: the compiled stylesheet is empty`)
mkdirSync(dirname(outFile), { recursive: true })
writeFileSync(outFile, code)
console.log(`build-css: ${outFile} (${code.length} bytes)`)
