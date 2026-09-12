import { describe, expect, it } from "vitest"
import {
  HEADER_BUTTON_CLASS,
  HEADER_SLOT_CLASS,
  MOBILE_DETAIL_HEADER_CLASS,
} from "@/components/team/mobile-detail-header"
import {
  SEGMENTED_ROW,
  SEGMENTED_ROW_COMPACT,
  SEGMENTED_TAB,
} from "@/components/ui/tabs"

// EXP-851: the ONE detail header — the native layout (round back, centred
// identifier, round `…`). The class strings are the contract; a second
// hand-rolled back row anywhere is the bug this pins.

describe(`MobileDetailHeader`, () => {
  it(`is one line with a hairline, never a card`, () => {
    const tokens = MOBILE_DETAIL_HEADER_CLASS.split(/\s+/)
    expect(tokens).toContain(`h-12`)
    expect(tokens).toContain(`shrink-0`)
    expect(tokens).toContain(`border-b`)
    // A rounded/filled bar would read as a second card inside the panel.
    expect(MOBILE_DETAIL_HEADER_CLASS).not.toContain(`rounded`)
    expect(MOBILE_DETAIL_HEADER_CLASS).not.toContain(`bg-`)
  })

  it(`draws GHOST round controls — no circle stroke, no fill`, () => {
    const tokens = HEADER_BUTTON_CLASS.split(/\s+/)
    expect(tokens).toContain(`rounded-full`)
    expect(tokens).toContain(`size-9`)
    expect(HEADER_BUTTON_CLASS).not.toContain(`border`)
    expect(HEADER_BUTTON_CLASS).not.toContain(`bg-popover`)
    expect(HEADER_BUTTON_CLASS).not.toContain(`backdrop-`)
  })

  it(`balances both ends so the title stays centred`, () => {
    // The empty slot must be exactly the button's footprint.
    for (const token of HEADER_SLOT_CLASS.split(/\s+/)) {
      expect(HEADER_BUTTON_CLASS.split(/\s+/)).toContain(token)
    }
  })
})

// EXP-851 §E: the Inbox / My issues strip and the Support Open / Resolved
// strip are ONE control at ONE size, in the big list views and in the
// sidebar's list nav.
describe(`segmented strips`, () => {
  it(`share one trigger sizing`, () => {
    expect(SEGMENTED_TAB).toBe(`px-3`)
    // No per-surface height/type overrides sneaking back in.
    expect(SEGMENTED_TAB).not.toContain(`h-`)
    expect(SEGMENTED_TAB).not.toContain(`text-`)
  })

  it(`share one row, tighter only in the 16rem sidebar slot`, () => {
    for (const row of [SEGMENTED_ROW, SEGMENTED_ROW_COMPACT]) {
      const tokens = row.split(/\s+/)
      expect(tokens).toContain(`flex`)
      expect(tokens).toContain(`shrink-0`)
      expect(tokens).toContain(`items-center`)
      expect(tokens).toContain(`justify-between`)
      expect(tokens).toContain(`pb-2`)
    }
    expect(SEGMENTED_ROW).toContain(`px-4`)
    expect(SEGMENTED_ROW_COMPACT).toContain(`px-2`)
  })
})
