// FEED-131 — the stylesheet the Solid primitives SHIP
// (`@exponential-at/ui-solid/styles.css` → dist/exponential-ui-solid.css):
// THE SAME source as the React package's (packages/exponential-ui-react/
// src/primitives/tailwind.css), compiled with `base` = that file's directory
// so its relative `@source` globs (and the tw-animate-css import) resolve
// exactly as `@exponential-at/ui-react`'s build-css.ts does. The two dist
// files are byte-identical (src/primitives/styles.test.ts locks it): one
// class means one rule on both renderers.
//
//   bun run --filter @exponential-at/ui-solid build:css

import { mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { compile, optimize } from "@tailwindcss/node"
import { Scanner } from "@tailwindcss/oxide"

const packageRoot = join(import.meta.dirname, `..`)
/** The React package's primitives directory: the ONE stylesheet source. */
export const SOURCE_BASE = join(packageRoot, `..`, `exponential-ui-react`, `src`, `primitives`)
export const OUT_FILE = join(packageRoot, `dist`, `exponential-ui-solid.css`)

/** The compiled, minified stylesheet. */
export async function compileStylesheet(): Promise<string> {
  const base = SOURCE_BASE
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
  return code
}

/** Compiles and writes dist/exponential-ui-solid.css. */
export async function buildCss(outFile = OUT_FILE): Promise<string> {
  const code = await compileStylesheet()
  mkdirSync(dirname(outFile), { recursive: true })
  writeFileSync(outFile, code)
  return outFile
}

if (import.meta.main) {
  const out = await buildCss()
  console.log(`build-css: ${out}`)
}
