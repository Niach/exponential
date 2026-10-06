// @vitest-environment node
import { readFileSync } from "node:fs"
// @ts-expect-error -- a plain ESM script outside the package, no types
import { THEME_PATH, buildTheme, renderTheme } from "../../../integrations/openclaw/scripts/theme.mjs"

// EXP-1183: the OpenClaw plugin's "Exponential" theme is generated from the
// styleguide (the design-tokens glass system + the contract's working verbs). A
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
    // Dark only, like every Exponential client.
    expect(Object.keys(theme)).not.toContain(`light`)
    for (const value of Object.values(theme.dark) as string[]) {
      expect(value.length).toBeLessThanOrEqual(120)
    }
    // OpenClaw mixes its colours, so the surfaces are opaque.
    for (const key of [`background`, `card`, `popover`, `secondary`, `muted`, `accent`, `border`, `input`]) {
      expect(theme.dark[key]).toMatch(/^#[0-9a-f]{6}$/)
    }
  })
})
