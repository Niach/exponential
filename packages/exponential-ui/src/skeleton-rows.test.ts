import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import fixture from "../fixtures/skeleton-rows.json" with { type: "json" }
import { SKELETON_OPACITY_HIGH, SKELETON_OPACITY_LOW, SKELETON_PULSE_MS, SKELETON_REDUCED_OPACITY } from "./layout"
import { builtinTheme, builtinThemes } from "./themes"
import { recipeStyle } from "./theme"
import { BASE_CSS } from "./web/base-css"

/** The app's contract (packages/domain-contract): the numbers the fixture mirrors. */
const feedbackStates = JSON.parse(readFileSync(join(import.meta.dir, `../../domain-contract/fixtures/feedback-states.json`), `utf8`)) as { placeholder: typeof fixture.placeholder }

describe(`#EXP-1629 fixtures/skeleton-rows.json`, () => {
  test(`mirrors feedback-states.json placeholder`, () => {
    expect(fixture.placeholder).toEqual(feedbackStates.placeholder)
  })
  test(`the pulse is the layout's`, () => {
    expect([SKELETON_OPACITY_LOW, SKELETON_OPACITY_HIGH]).toEqual(fixture.placeholder.opacity)
    expect(SKELETON_PULSE_MS).toBe(fixture.placeholder.pulseMs)
    expect(SKELETON_REDUCED_OPACITY).toBe(fixture.placeholder.reducedOpacity)
    expect(fixture.pulse.easing).toEqual([0.42, 0, 0.58, 1])
    expect(BASE_CSS).toContain(`animation:xui-skeleton ${fixture.placeholder.pulseMs}ms ease-in-out infinite`)
  })
  test(`rows are borderless`, () => {
    expect(fixture.row.hairline).toBe(false)
    expect(BASE_CSS).not.toMatch(/xui-Skeleton-row\+\.xui-Skeleton-row/)
    expect(BASE_CSS).not.toMatch(/\.xui-Skeleton-row\{[^}]*border/)
  })
  test(`every built-in theme draws the contract's row, dot and bars`, () => {
    for (const theme of builtinThemes()) {
      const part = (name: string) => recipeStyle(theme, { component: `Skeleton`, part: name, props: { rounded: false, dot: true } }) as Record<string, unknown>
      const row = part(`row`)
      expect([row.gap, row.paddingHorizontal].map((v) => (typeof v === `string` ? theme.tokens.spacing[v.replace(`$spacing.`, ``)] : v)), theme.id).toEqual([fixture.row.gap, fixture.row.paddingHorizontal])
      for (const key of [`borderTopWidth`, `borderWidth`, `borderBottomWidth`]) expect(row[key], `${theme.id} row ${key}`).toBeUndefined()
      const dot = part(`dot`)
      expect([dot.width, dot.height, dot.backgroundColor], `${theme.id} dot`).toEqual([fixture.placeholder.dot, fixture.placeholder.dot, fixture.fill])
      const bar = part(`bar`)
      expect([bar.height, bar.borderRadius, bar.backgroundColor], `${theme.id} bar`).toEqual([fixture.placeholder.barHeight, fixture.placeholder.radius, fixture.fill])
    }
    expect(builtinTheme(`exponential`).id).toBe(`exponential`)
  })
  test(`the cases' frames follow from the numbers`, () => {
    for (const c of fixture.cases) {
      const dot = c.skeleton.dot !== false
      const rows = c.skeleton.rows
      for (let r = 0; r < rows; r++) {
        const y = r * c.skeleton.rowHeight
        const [lead, title] = fixture.bars[r % fixture.bars.length]!
        const pad = fixture.row.paddingHorizontal
        const gap = fixture.row.gap
        const leadX = pad + (dot ? fixture.placeholder.dot + gap : 0)
        const titleX = leadX + lead + gap
        const barY = y + (c.skeleton.rowHeight - fixture.placeholder.barHeight) / 2
        const frames = c.frames as Record<string, number[]>
        expect(frames[`sk.row.${r}`], c.name).toEqual([0, y, c.width, c.skeleton.rowHeight])
        if (dot) expect(frames[`sk.dot.${r}`], c.name).toEqual([pad, y + (c.skeleton.rowHeight - fixture.placeholder.dot) / 2, fixture.placeholder.dot, fixture.placeholder.dot])
        expect(frames[`sk.bar.${r}.0`], c.name).toEqual([leadX, barY, lead, fixture.placeholder.barHeight])
        expect(frames[`sk.bar.${r}.1`], c.name).toEqual([titleX, barY, Math.min(title, c.width - pad - titleX), fixture.placeholder.barHeight])
      }
    }
  })
})
