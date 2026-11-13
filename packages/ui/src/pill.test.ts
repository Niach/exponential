// EXP-1822 — the pill/chip metrics ×4.
//
// `tokens.json` `size` carries the pill's numbers (padding, gap, glyph, dot,
// the assignee face, the labels chip's dots, the property chip box). Android
// `GlassPillDefaults` and iOS `GlassPillTokens` READ them (their token tests
// pin that); the web's `Pill` spells them as Tailwind classes, so this test
// holds those classes to the same tokens (a Tailwind spacing unit = 4px).

import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"
import { pillVariants } from "./pill"

const __dirname = dirname(fileURLToPath(import.meta.url))
const repoRoot = join(__dirname, `..`, `..`, `..`)
const size = JSON.parse(
  readFileSync(join(repoRoot, `packages/design-tokens/tokens.json`), `utf8`)
).size as Record<string, number>

const unit = (px: number) => `${px / 4}`
const classes = (s: `sm` | `md`) => pillVariants({ size: s }).split(/\s+/)

describe(`Pill metrics = tokens.json size`, () => {
  it(`sm: h-6 px-2 gap-1, 12px glyphs`, () => {
    const c = classes(`sm`)
    expect(c).toContain(`h-${unit(size.controlSm)}`)
    expect(c).toContain(`px-${unit(size.pillPaddingSm)}`)
    expect(c).toContain(`gap-${unit(size.pillGapSm)}`)
    expect(c).toContain(`[&_svg:not([class*='size-'])]:size-${unit(size.pillGlyphSm)}`)
  })

  it(`md: h-8 px-3 gap-1.5, 16px glyphs`, () => {
    const c = classes(`md`)
    expect(c).toContain(`h-${unit(size.controlMd)}`)
    expect(c).toContain(`px-${unit(size.pillPaddingMd)}`)
    expect(c).toContain(`gap-${unit(size.pillGapMd)}`)
    expect(c).toContain(`[&_svg:not([class*='size-'])]:size-${unit(size.pillGlyphMd)}`)
  })

  it(`the property chips: a 16px face, 8px label dots`, () => {
    expect(size.pillDot).toBe(6)
    expect(size.pillAvatar).toBe(16)
    expect((size.controlSm - size.pillAvatar) / 2).toBe(4)
    expect(size.pillLabelDot).toBe(8)
    expect(size.chipBoxGap).toBe(6)
    expect(size.chipBoxPadding).toBe(10)
    const assignee = readFileSync(
      join(repoRoot, `apps/web/src/components/issue-properties/assignee-picker.tsx`),
      `utf8`
    )
    expect(assignee).toContain(`<UserAvatar size={${size.pillAvatar}}`)
    const labels = readFileSync(
      join(repoRoot, `apps/web/src/components/issue-properties/label-picker.tsx`),
      `utf8`
    )
    const dot = unit(size.pillLabelDot)
    expect(labels).toContain(`h-${dot} w-${dot} shrink-0 rounded-full ring-1`)
  })
})
