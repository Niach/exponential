import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"

// Guards that packages/design-tokens/tokens.json stays in lockstep with the web
// theme it mirrors (the `.dark` block of apps/web/src/styles.css). tokens.json
// is the single source the native palettes are generated from, so if the web
// designer changes a swatch here without updating the shared token, this fails.

const __dirname = dirname(fileURLToPath(import.meta.url))
const repoRoot = join(__dirname, `..`, `..`, `..`, `..`)

const tokens = JSON.parse(
  readFileSync(join(repoRoot, `packages/design-tokens/tokens.json`), `utf8`)
) as {
  palette: Record<string, string>
  semantic: Record<string, string>
  glass: Record<string, string>
}

const stylesCss = readFileSync(
  join(repoRoot, `apps/web/src/styles.css`),
  `utf8`
)

// Pull the `--var: value;` declarations out of a top-level `<selector> { … }`
// block. Neither block nests braces, so `[^}]*` stops at the right place.
function parseBlockVars(
  css: string,
  selector: RegExp,
  label: string
): Record<string, string> {
  const block = css.match(selector)
  if (!block) throw new Error(`Could not find ${label} block in styles.css`)
  const vars: Record<string, string> = {}
  for (const line of block[1].split(`\n`)) {
    const m = line.match(/^\s*--([\w-]+):\s*(.+?);\s*$/)
    if (m) vars[m[1]] = m[2].trim()
  }
  return vars
}

// The brand + glass tokens are theme-invariant (the app is dark-only) and are
// deliberately duplicated into BOTH :root and .dark. Only .dark is parity-
// checked against tokens.json, so this is the set the two blocks must agree on.
function themeInvariantVars(
  vars: Record<string, string>
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(vars).filter(
      ([k]) => k.startsWith(`brand`) || k.startsWith(`glass-`)
    )
  )
}

function kebab(camel: string): string {
  return camel.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)
}

describe(`design-tokens parity with web styles.css`, () => {
  const darkVars = parseBlockVars(stylesCss, /\.dark\s*\{([^}]*)\}/, `.dark`)
  const rootVars = parseBlockVars(stylesCss, /:root\s*\{([^}]*)\}/, `:root`)

  it(`every palette token matches the corresponding .dark CSS variable`, () => {
    for (const [key, value] of Object.entries(tokens.palette)) {
      if (key.startsWith(`$`)) continue
      const cssVar = kebab(key)
      expect(
        darkVars[cssVar],
        `tokens.palette.${key} should equal --${cssVar} in styles.css`
      ).toBe(value)
    }
  })

  it(`every glass token matches the corresponding --glass-* CSS variable`, () => {
    for (const [key, value] of Object.entries(tokens.glass)) {
      if (key.startsWith(`$`)) continue
      const cssVar = `glass-${kebab(key)}`
      expect(
        darkVars[cssVar],
        `tokens.glass.${key} should equal --${cssVar} in styles.css`
      ).toBe(value)
    }
  })

  it(`the brand accent matches --brand`, () => {
    expect(darkVars.brand).toBe(tokens.semantic.brand)
  })

  // EXP-280: --brand-strong is the fill every client must use for a solid
  // brand surface carrying text (white on --brand is 4.28:1, under AA). It was
  // web-only until it landed in tokens.json; keep the two in lockstep so the
  // native clients can't drift back onto the raw accent.
  it(`the text-bearing brand fill matches --brand-strong`, () => {
    expect(darkVars[`brand-strong`]).toBe(tokens.semantic.brandStrong)
  })

  // EXP-269 duplicated the brand + glass vars into :root as well, but only the
  // .dark copy is parity-checked above — so nothing kept the two in sync.
  it(`the :root and .dark brand + glass blocks are identical`, () => {
    const rootInvariant = themeInvariantVars(rootVars)
    expect(
      Object.keys(rootInvariant).length,
      `expected :root to carry the brand + glass vars`
    ).toBeGreaterThan(0)
    expect(
      rootInvariant,
      `the theme-invariant brand/glass vars must be byte-identical in :root and .dark`
    ).toEqual(themeInvariantVars(darkVars))
  })
})
