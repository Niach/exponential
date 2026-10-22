import { SEGMENTED_TAB } from "./tabs"

// EXP-941 — the segmented capsule as a COMPONENT.
//
// `tabs.tsx` has owned the class recipe for a while, but the eight strips that
// use it each wired their own `Tabs` + `TabsList` + N `TabsTrigger`s by hand,
// so the triggers drifted in padding and in whether they carried a glyph. This
// renders the strip from an option array instead. It changes no string in
// `tabs.tsx`: the constants stay exactly as they are (other surfaces, and
// `mobile-detail-header.test.ts`, read them directly).
//
// `embedded` swaps the free-floating capsule for `GlassTabsRow` — the strip as
// the FIRST ROW of a glass group (EXP-694), which is what the settings sheets
// and the natives' `GlassSegmentedControl(embedded:)` draw.
export const SEGMENTED_CONTROL_CLASSES = {
  trigger: SEGMENTED_TAB,
  /** `fill`: span the container instead of the capsule's natural `w-fit`. */
  fill: `w-full`,
} as const
