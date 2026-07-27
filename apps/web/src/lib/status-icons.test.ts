import { describe, expect, it } from "vitest"
import { categoryStatusIcon, hexWithAlpha, startedClockIcon } from "./status-icons"

// The exact clock table is the cross-platform parity contract (EXP-314):
// iOS/Android/desktop lock the SAME literal lists in their own unit tests.
describe(`startedClockIcon`, () => {
  it(`locks the full fill table`, () => {
    // N ≤ 2 — the builtin defaults' half + ¾ look (N=1 can't happen: the two
    // started builtins are undeletable, but the table still answers).
    expect(startedClockIcon(0, 1)).toBe(`progress-2-4`)
    expect(startedClockIcon(0, 2)).toBe(`progress-2-4`)
    expect(startedClockIcon(1, 2)).toBe(`progress-3-4`)
    // N = 3 — quarters.
    expect(startedClockIcon(0, 3)).toBe(`progress-1-4`)
    expect(startedClockIcon(1, 3)).toBe(`progress-2-4`)
    expect(startedClockIcon(2, 3)).toBe(`progress-3-4`)
    // N = 4 — fifths.
    expect(startedClockIcon(0, 4)).toBe(`progress-1-5`)
    expect(startedClockIcon(1, 4)).toBe(`progress-2-5`)
    expect(startedClockIcon(2, 4)).toBe(`progress-3-5`)
    expect(startedClockIcon(3, 4)).toBe(`progress-4-5`)
  })

  it(`clamps out-of-range positions and over-cap counts`, () => {
    // A transiently over-cap team (racing creates) must render, not crash.
    expect(startedClockIcon(4, 5)).toBe(`progress-4-5`)
    expect(startedClockIcon(-1, 2)).toBe(`progress-2-4`)
    expect(startedClockIcon(9, 3)).toBe(`progress-3-4`)
  })
})

describe(`categoryStatusIcon`, () => {
  it(`maps each category to its fixed glyph`, () => {
    expect(categoryStatusIcon(`backlog`, 0, 2)).toBe(`circle-dashed`)
    expect(categoryStatusIcon(`unstarted`, 0, 2)).toBe(`circle`)
    expect(categoryStatusIcon(`completed`, 0, 2)).toBe(`circle-check`)
    expect(categoryStatusIcon(`cancelled`, 0, 2)).toBe(`circle-x`)
    expect(categoryStatusIcon(`duplicate`, 0, 2)).toBe(`copy`)
    expect(categoryStatusIcon(`started`, 1, 2)).toBe(`progress-3-4`)
  })
})

describe(`hexWithAlpha`, () => {
  it(`converts a hex to an rgba wash`, () => {
    expect(hexWithAlpha(`#EAB308`, 0.1)).toBe(`rgba(234, 179, 8, 0.1)`)
  })
})
