import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import fixture from "@exp/domain-contract/fixtures/context-layout.json"
import {
  contextWindowView,
  tokensCompact,
  CONTEXT_WINDOW_TITLE,
  type ContextSegment,
  type ContextWindowView,
} from "@/lib/context-layout"
import type { SessionUsageState } from "@/lib/agent-feed"
import { DANGER_PERCENT, WARNING_PERCENT } from "@/lib/agent-usage"

// EXP-1051: the context-window bar + legend, locked ×4 (desktop
// ui::context_layout, iOS ContextLayoutPresentationTests, Android
// ContextLayoutPresentationTest) against the ONE contract fixture — same
// cases, same names. Porting a client means making this file's fixture pass,
// nothing else.
interface FixtureCase {
  name: string
  usage: SessionUsageState | null
  segments: ContextSegment[] | null
  expected: ContextWindowView | null
}

const cases = fixture as unknown as FixtureCase[]

describe(`contextWindowView`, () => {
  it(`every fixture case folds exactly`, () => {
    expect(cases.length).toBeGreaterThanOrEqual(12)
    for (const { name, usage, segments, expected } of cases) {
      expect(contextWindowView(usage, segments), name).toEqual(expected)
    }
  })

  it(`the fixture covers the rules a port has to get right`, () => {
    const names = cases.map((c) => c.name)
    expect(names.length).toBe(new Set(names).size)
    const views = cases.map((c) => c.expected)
    // A null view (no usage / a zero window) and a real one both appear.
    expect(views.some((view) => view === null)).toBe(true)
    expect(views.some((view) => view !== null)).toBe(true)
    // Every severity.
    for (const level of [`normal`, `warning`, `danger`]) {
      expect(
        views.some((view) => view?.severity === level),
        level
      ).toBe(true)
    }
    // Every contract segment key draws in at least one case, so a client that
    // forgets a label fails here rather than in someone's sidebar.
    for (const spec of contract.contextLayout.segments) {
      expect(
        views.some((view) => view?.bar.some((slice) => slice.key === spec.key)),
        spec.key
      ).toBe(true)
    }
    // The derived rows ride EVERY non-null legend — they are computed, never
    // received, so they can never be missing.
    for (const view of views) {
      if (!view) continue
      const keys = view.legend.map((row) => row.key)
      expect(keys.slice(-2)).toEqual([`conversation`, `free`])
      // The conversation is the bar's last slice, always; free is the track.
      expect(view.bar[view.bar.length - 1].key).toBe(`conversation`)
      expect(view.bar.some((slice) => slice.key === `free`)).toBe(false)
    }
  })

  it(`never rescales an overshooting layout`, () => {
    // The device's estimates may add past the measured `contextUsed`; the
    // renderer clips at 100% rather than shrinking the layers to fit, so the
    // numbers a reader compares stay the numbers the device reported.
    const view = contextWindowView({ contextUsed: 20_000, contextSize: 200_000 }, [
      { key: `base`, tokens: 21_000, source: `measured` },
      { key: `tools`, tokens: 2_400, source: `estimated` },
    ])
    const total = view!.bar.reduce((sum, slice) => sum + slice.percent, 0)
    expect(total).toBeCloseTo(11.7, 5)
    expect(view!.bar.find((slice) => slice.key === `base`)!.percent).toBe(10.5)
  })
})

describe(`tokensCompact`, () => {
  it(`drops a trailing .0 and keeps small counts raw`, () => {
    expect(tokensCompact(0)).toBe(`0`)
    expect(tokensCompact(600)).toBe(`600`)
    expect(tokensCompact(999)).toBe(`999`)
    expect(tokensCompact(1_000)).toBe(`1k`)
    expect(tokensCompact(1_500)).toBe(`1.5k`)
    expect(tokensCompact(21_000)).toBe(`21k`)
    expect(tokensCompact(37_400)).toBe(`37.4k`)
    expect(tokensCompact(134_700)).toBe(`134.7k`)
    // A negative count is a producer bug, never a negative label.
    expect(tokensCompact(-500)).toBe(`0`)
  })
})

describe(`the contract`, () => {
  it(`pins the ticks to the compaction floor and the usage thresholds`, () => {
    // The first tick is the floor `exponential_sessions_compact` refuses
    // below. The engine's COMPACT_MIN_CONTEXT_FRACTION
    // (apps/desktop/crates/engine/src/compaction.rs) mirrors this number as a
    // fraction, and asserts the other direction — so the mark on the bar and
    // the tool's refusal can never disagree.
    expect(contract.contextLayout.compactMinPercent).toBe(50)
    const view = contextWindowView({ contextUsed: 1, contextSize: 100 }, null)
    expect(view!.ticks).toEqual([50, WARNING_PERCENT, DANGER_PERCENT])
    expect(view!.ticks).toEqual([50, 75, 95])
  })

  it(`owns the title and the derived rows' labels`, () => {
    expect(CONTEXT_WINDOW_TITLE).toBe(contract.contextLayout.title)
    expect(contract.contextLayout.derived.map((spec) => spec.key)).toEqual([
      `conversation`,
      `free`,
    ])
  })
})
