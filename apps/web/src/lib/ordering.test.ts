import { describe, expect, it } from "vitest"
import { byCreatedAt, byCreatedAtDesc } from "./ordering"

const row = (id: string, createdAt: string) => ({ id, createdAt })

describe(`stable list ordering (EXP-668)`, () => {
  const tied = [
    row(`c`, `2026-08-01T00:00:00Z`),
    row(`a`, `2026-08-01T00:00:00Z`),
    row(`b`, `2026-08-01T00:00:00Z`),
  ]

  it(`resolves equal timestamps by id instead of arrival order`, () => {
    expect([...tied].sort(byCreatedAt).map((r) => r.id)).toEqual([`a`, `b`, `c`])
    // The SAME rows arriving in a different order must sort the same way —
    // this is the whole point: Electric's arrival order is not stable.
    expect([...tied].reverse().sort(byCreatedAt).map((r) => r.id)).toEqual([
      `a`,
      `b`,
      `c`,
    ])
  })

  it(`keeps the timestamp as the primary key, not the id`, () => {
    const rows = [
      row(`a`, `2026-08-03T00:00:00Z`),
      row(`z`, `2026-08-01T00:00:00Z`),
    ]
    expect(rows.sort(byCreatedAt).map((r) => r.id)).toEqual([`z`, `a`])
  })

  it(`orders newest first, and breaks ties the same way in both directions`, () => {
    const rows = [
      row(`a`, `2026-08-01T00:00:00Z`),
      row(`b`, `2026-08-03T00:00:00Z`),
      row(`c`, `2026-08-01T00:00:00Z`),
    ]
    expect([...rows].sort(byCreatedAtDesc).map((r) => r.id)).toEqual([
      `b`,
      `a`,
      `c`,
    ])
    // Reversing the descending list gives the ascending one: the tie-break
    // does not flip with the primary key.
    expect([...rows].sort(byCreatedAt).map((r) => r.id)).toEqual([`a`, `c`, `b`])
  })

  it(`accepts Date, ISO string and epoch alike`, () => {
    const rows = [
      { id: `a`, createdAt: new Date(`2026-08-02T00:00:00Z`) },
      { id: `b`, createdAt: `2026-08-01T00:00:00Z` },
      { id: `c`, createdAt: Date.parse(`2026-08-03T00:00:00Z`) },
    ]
    expect(rows.sort(byCreatedAt).map((r) => r.id)).toEqual([`b`, `a`, `c`])
  })
})
