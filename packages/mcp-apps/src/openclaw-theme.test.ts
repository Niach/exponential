// @vitest-environment node
import { readFileSync } from "node:fs"
// @ts-expect-error -- a plain ESM script outside the package, no types
import { THEME_PATH, buildTheme, renderTheme } from "../../../integrations/openclaw/scripts/theme.mjs"

// EXP-1183: the OpenClaw plugin's "Exponential" theme is generated from the
// styleguide (packages/ui/src/styles.css + the contract's working verbs). A
// token change that leaves it behind fails here, like the other generated
// cross-client outputs.
describe(`OpenClaw theme`, () => {
  it(`matches the styleguide`, () => {
    expect(readFileSync(THEME_PATH, `utf8`)).toBe(renderTheme())
  })

  it(`fits OpenClaw's theme limits`, () => {
    const theme = buildTheme()
    expect(new TextEncoder().encode(JSON.stringify(theme)).length).toBeLessThan(4096)
    expect(theme.workingPhrases.length).toBeLessThanOrEqual(24)
    for (const phrase of theme.workingPhrases) expect(phrase.length).toBeLessThanOrEqual(24)
    for (const mode of [`light`, `dark`] as const) {
      for (const value of Object.values(theme[mode]) as string[]) {
        expect(value.length).toBeLessThanOrEqual(120)
      }
    }
  })
})
