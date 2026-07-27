import { describe, expect, it } from "vitest"
import {
  dueDateTone,
  dueDateToneClass,
  formatDueDateMenuMeta,
  getDueDatePresets,
} from "@/lib/issue-due-date"
import { formatDateForMutation } from "@/lib/domain"

describe(`issue-due-date`, () => {
  it(`builds the due date presets with the expected work-week behavior`, () => {
    const baseDate = new Date(`2026-03-07T09:00:00Z`)
    const presets = getDueDatePresets(baseDate)

    expect(presets).toHaveLength(3)
    expect(presets[0].label).toBe(`Tomorrow`)
    expect(formatDateForMutation(presets[0].date)).toBe(`2026-03-08`)
    expect(presets[1].label).toBe(`End of this week`)
    expect(formatDateForMutation(presets[1].date)).toBe(`2026-03-13`)
    expect(presets[2].label).toBe(`In one week`)
    expect(formatDateForMutation(presets[2].date)).toBe(`2026-03-14`)
  })

  it(`formats compact menu metadata`, () => {
    expect(formatDueDateMenuMeta(new Date(`2026-03-08T12:00:00Z`))).toBe(
      `Sun, 8 Mar`
    )
  })

  // REV2-48: the mobile rule, ported verbatim — due today WINS over overdue.
  it(`tones a due date like iOS/Android do`, () => {
    expect(dueDateTone(`2026-03-07`, `2026-03-07`)).toBe(`today`)
    expect(dueDateTone(`2026-03-06`, `2026-03-07`)).toBe(`overdue`)
    expect(dueDateTone(`2026-03-08`, `2026-03-07`)).toBe(`upcoming`)
    // ISO strings compare correctly across month/year rollovers.
    expect(dueDateTone(`2025-12-31`, `2026-01-01`)).toBe(`overdue`)
  })

  it(`maps each tone to its text color`, () => {
    expect(dueDateToneClass(`2026-03-06`, `2026-03-07`)).toBe(`text-red-500`)
    expect(dueDateToneClass(`2026-03-07`, `2026-03-07`)).toBe(`text-orange-500`)
    expect(dueDateToneClass(`2026-03-08`, `2026-03-07`)).toBe(
      `text-muted-foreground`
    )
  })
})
