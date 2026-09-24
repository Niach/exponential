// EXP-1051: where a run's context window actually GOES — the stacked bar and
// its legend, folded from the `usage` meter (how full the window is) plus the
// device's `context_layout` state (what the launcher put in it before the
// first turn).
//
// Hand-mirrored ×4 and byte-locked by the ONE contract fixture every client's
// test iterates (`packages/domain-contract/fixtures/context-layout.json`):
//   desktop  apps/desktop/crates/ui/src/context_layout.rs
//   iOS      apps/ios/ExpCore/Sources/Domain/ContextLayoutPresentation.swift
//   Android  apps/android/.../domain/ContextLayoutPresentation.kt
// Changing a rule or a string here means changing it in all four and in the
// fixture — the fixture IS the spec.
//
// Two rules the renderers depend on:
//  • `conversation` and `free` are DERIVED here, never on the wire: the device
//    publishes only what it can attribute, and everything else the window
//    holds is the conversation.
//  • The segments are NEVER scaled to fit. When the device's estimates
//    overshoot the measured `contextUsed` the percents still add past 100 and
//    the renderer CLIPS — a silently rescaled bar would lie about the layer
//    sizes to hide a rounding error.

import { contract } from "@exp/domain-contract"
import type { ContextSegment, SessionUsageState } from "@/lib/agent-feed"
import {
  contextPercent,
  formatContextUsage,
  severity,
  DANGER_PERCENT,
  WARNING_PERCENT,
  type UsageSeverity,
} from "@/lib/agent-usage"

export type { ContextSegment }

/** EXP-1051 section title. Byte-identical ×4. */
export const CONTEXT_WINDOW_TITLE = contract.contextLayout.title

/** One slice of the stacked bar. `percent` is of the WHOLE window (size), to
 *  two decimals — small layers (a 600-token task prompt in a 200k window) are
 *  a hairline rather than nothing. `free` is not a slice: it is the track. */
export interface ContextBarSlice {
  key: string
  tone: string
  percent: number
}

/** One legend row. Both numbers are already FORMATTED — the ×4 lock is on the
 *  strings, not on the arithmetic behind them. */
export interface ContextLegendRow {
  key: string
  label: string
  tone: string
  /** `21k`, `37.4k`, `600` — see `tokensCompact`. */
  tokens: string
  /** `10.5%`, `0.3%` — one decimal, always. */
  percent: string
  /** The device guessed this layer rather than measuring it; the UI prefixes
   *  `≈`. Always false for the derived rows. */
  estimated: boolean
  /** What the layer is made of, when the device named it (`CLAUDE.md,
   *  ~/.claude/CLAUDE.md`). */
  detail?: string
}

export interface ContextWindowView {
  /** `65k / 200k (32%)` — `formatContextUsage`, unchanged. */
  headline: string
  /** 0-100, floored (`contextPercent`). */
  percent: number
  severity: UsageSeverity
  /** Where the bar draws its marks: the compaction floor, then the two usage
   *  thresholds. */
  ticks: number[]
  bar: ContextBarSlice[]
  legend: ContextLegendRow[]
}

const SEGMENT_SPECS = contract.contextLayout.segments
const DERIVED_SPECS = contract.contextLayout.derived

const CONVERSATION = DERIVED_SPECS.find((spec) => spec.key === `conversation`)
const FREE = DERIVED_SPECS.find((spec) => spec.key === `free`)

/** `21k`, `37.4k`, `1.5k`, `134.7k`, and the raw number under 1000 (`600`).
 *  One decimal, with a trailing `.0` dropped — `21.0k` reads as false
 *  precision on a number the device estimated. */
export function tokensCompact(tokens: number): string {
  const value = Math.max(0, Math.round(tokens))
  if (value < 1000) return `${value}`
  const text = (value / 1000).toFixed(1)
  return `${text.endsWith(`.0`) ? text.slice(0, -2) : text}k`
}

/** Percent of the whole window, to TWO decimals — the bar's geometry.
 *  Integer arithmetic (multiply before divide) so the four clients agree on
 *  the rounding of an exact half. */
function barPercent(tokens: number, size: number): number {
  return Math.round((tokens * 10_000) / size) / 100
}

/** Percent of the whole window, to ONE decimal plus the sign — the legend's
 *  string. Rounded from the same integer arithmetic as `barPercent`, so a row
 *  reading `0.0%` is a row whose slice is a hairline, never a rounding
 *  disagreement between the two. */
function legendPercent(tokens: number, size: number): string {
  return `${(Math.round((tokens * 1_000) / size) / 10).toFixed(1)}%`
}

/** The wire segments this client KNOWS, in the contract's render order: an
 *  unknown key is dropped (an older client never draws a layer it cannot
 *  label), the FIRST of a duplicate key wins, a negative count reads as zero
 *  and a zero-token layer never draws at all. */
function knownSegments(
  segments: ContextSegment[] | null
): { spec: { key: string; label: string; tone: string }; segment: ContextSegment }[] {
  if (!segments) return []
  const out: {
    spec: { key: string; label: string; tone: string }
    segment: ContextSegment
  }[] = []
  for (const spec of SEGMENT_SPECS) {
    const segment = segments.find((entry) => entry.key === spec.key)
    if (!segment) continue
    const tokens = Math.max(0, Math.round(segment.tokens))
    if (tokens === 0) continue
    out.push({ spec, segment: { ...segment, tokens } })
  }
  return out
}

/** The whole context-window view, or null when there is no window to draw:
 *  the engine has published no `usage` yet, or it reported a zero size
 *  ("unknown"). A layout WITHOUT a usage is nothing — the bar has no scale.
 *
 *  `conversation` = what the window holds that the device could not attribute
 *  (clamped at 0 when its estimates overshoot), `free` = the rest of the
 *  window (clamped at 0 once a run runs past its own size). */
export function contextWindowView(
  usage: SessionUsageState | null,
  segments: ContextSegment[] | null
): ContextWindowView | null {
  const percent = contextPercent(usage)
  if (!usage || percent === null) return null
  const size = usage.contextSize
  const used = Math.max(0, usage.contextUsed)

  const known = knownSegments(segments)
  const attributed = known.reduce((sum, entry) => sum + entry.segment.tokens, 0)
  const conversation = Math.max(0, used - attributed)
  const free = Math.max(0, size - used)

  const bar: ContextBarSlice[] = known.map(({ spec, segment }) => ({
    key: spec.key,
    tone: spec.tone,
    percent: barPercent(segment.tokens, size),
  }))
  // Always drawn, even at zero: the conversation is the slice that GROWS, and
  // a bar that gains a segment mid-run would re-key its slices.
  bar.push({
    key: CONVERSATION?.key ?? `conversation`,
    tone: CONVERSATION?.tone ?? `blue`,
    percent: barPercent(conversation, size),
  })

  const legend: ContextLegendRow[] = known.map(({ spec, segment }) => {
    const row: ContextLegendRow = {
      key: spec.key,
      label: spec.label,
      tone: spec.tone,
      tokens: tokensCompact(segment.tokens),
      percent: legendPercent(segment.tokens, size),
      estimated: segment.source === `estimated`,
    }
    if (segment.detail) row.detail = segment.detail
    return row
  })
  for (const [spec, tokens] of [
    [CONVERSATION, conversation],
    [FREE, free],
  ] as const) {
    if (!spec) continue
    legend.push({
      key: spec.key,
      label: spec.label,
      tone: spec.tone,
      tokens: tokensCompact(tokens),
      percent: legendPercent(tokens, size),
      // Derived from the engine's own measurement — never a guess.
      estimated: false,
    })
  }

  return {
    headline: formatContextUsage(usage),
    percent,
    severity: severity(percent),
    // The compaction floor first: below it `exponential_sessions_compact`
    // refuses, so the tick is what makes "not yet" legible.
    ticks: [contract.contextLayout.compactMinPercent, WARNING_PERCENT, DANGER_PERCENT],
    bar,
    legend,
  }
}
