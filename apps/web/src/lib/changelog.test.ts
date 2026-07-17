import { describe, expect, it } from "vitest"
import { CHANGELOG, latestChangelogEntry } from "./changelog"

// Guards the authoring convention (EXP-164): the head entry's id is the
// per-device dismissal key for the sidebar "What's new" card, so ids must be
// unique and the newest entry must be prepended.

describe(`CHANGELOG`, () => {
  it(`has at least one entry`, () => {
    expect(CHANGELOG.length).toBeGreaterThan(0)
  })

  it(`has unique, non-empty ids`, () => {
    const ids = CHANGELOG.map((entry) => entry.id)
    expect(ids.every((id) => id.length > 0)).toBe(true)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it(`has valid ISO dates sorted newest first`, () => {
    for (const entry of CHANGELOG) {
      expect(entry.date).toMatch(/^\d{4}-\d{2}-\d{2}$/)
      expect(Number.isNaN(new Date(entry.date).getTime())).toBe(false)
    }
    const dates = CHANGELOG.map((entry) => entry.date)
    const sorted = [...dates].sort((a, b) => b.localeCompare(a))
    expect(dates).toEqual(sorted)
  })

  it(`has a one-line summary and non-empty content on every entry`, () => {
    for (const entry of CHANGELOG) {
      expect(entry.title.length).toBeGreaterThan(0)
      expect(entry.summary.length).toBeGreaterThan(0)
      expect(entry.summary).not.toContain(`\n`)
      expect(entry.body.length).toBeGreaterThan(0)
    }
  })

  it(`latestChangelogEntry returns the head entry`, () => {
    expect(latestChangelogEntry()).toBe(CHANGELOG[0])
  })
})
