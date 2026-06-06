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
) as { palette: Record<string, string> }

const stylesCss = readFileSync(
  join(repoRoot, `apps/web/src/styles.css`),
  `utf8`
)

// Pull the `--var: value;` declarations out of the `.dark { … }` block.
function parseDarkVars(css: string): Record<string, string> {
  const block = css.match(/\.dark\s*\{([^}]*)\}/)
  if (!block) throw new Error(`Could not find .dark block in styles.css`)
  const vars: Record<string, string> = {}
  for (const line of block[1].split(`\n`)) {
    const m = line.match(/^\s*--([\w-]+):\s*(.+?);\s*$/)
    if (m) vars[m[1]] = m[2].trim()
  }
  return vars
}

function kebab(camel: string): string {
  return camel.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)
}

describe(`design-tokens parity with web styles.css`, () => {
  const darkVars = parseDarkVars(stylesCss)

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
})
