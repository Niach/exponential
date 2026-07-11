import { describe, expect, it } from "vitest"
import type { IssueStatus } from "@/lib/domain"
import {
  compareReleases,
  releaseProgress,
  type SortableRelease,
} from "@/lib/releases"

function issuesOf(...statuses: IssueStatus[]) {
  return statuses.map((status) => ({ status }))
}

function makeRelease(overrides: Partial<SortableRelease>): SortableRelease {
  return {
    targetDate: null,
    shippedAt: null,
    createdAt: new Date(`2026-03-01T10:00:00.000Z`),
    ...overrides,
  }
}

describe(`releaseProgress`, () => {
  it(`returns all-zero (and not complete) for an empty release`, () => {
    expect(releaseProgress([])).toEqual({
      total: 0,
      done: 0,
      dropped: 0,
      denominator: 0,
      fraction: 0,
      isComplete: false,
    })
  })

  it(`counts done against a denominator that excludes cancelled + duplicate`, () => {
    const progress = releaseProgress(
      issuesOf(`done`, `done`, `in_progress`, `todo`, `cancelled`, `duplicate`)
    )
    expect(progress).toEqual({
      total: 6,
      done: 2,
      dropped: 2,
      denominator: 4,
      fraction: 0.5,
      isComplete: false,
    })
  })

  it(`treats backlog as not-done`, () => {
    const progress = releaseProgress(issuesOf(`backlog`, `done`))
    expect(progress.done).toBe(1)
    expect(progress.denominator).toBe(2)
    expect(progress.isComplete).toBe(false)
  })

  it(`is complete when every non-dropped issue is done`, () => {
    const progress = releaseProgress(issuesOf(`done`, `done`, `cancelled`))
    expect(progress).toEqual({
      total: 3,
      done: 2,
      dropped: 1,
      denominator: 2,
      fraction: 1,
      isComplete: true,
    })
  })

  it(`is NOT complete when only dropped issues remain (denominator 0)`, () => {
    const progress = releaseProgress(issuesOf(`cancelled`, `duplicate`))
    expect(progress.denominator).toBe(0)
    expect(progress.fraction).toBe(0)
    expect(progress.isComplete).toBe(false)
  })
})

describe(`compareReleases`, () => {
  it(`sorts unshipped before shipped`, () => {
    const shipped = makeRelease({
      shippedAt: new Date(`2026-03-05T10:00:00.000Z`),
    })
    const unshipped = makeRelease({})
    expect(compareReleases(unshipped, shipped)).toBeLessThan(0)
    expect(compareReleases(shipped, unshipped)).toBeGreaterThan(0)
  })

  it(`sorts unshipped by targetDate asc with nulls last`, () => {
    const early = makeRelease({ targetDate: `2026-03-10` })
    const late = makeRelease({ targetDate: `2026-04-01` })
    const undated = makeRelease({ targetDate: null })
    expect(compareReleases(early, late)).toBeLessThan(0)
    expect(compareReleases(late, early)).toBeGreaterThan(0)
    expect(compareReleases(early, undated)).toBeLessThan(0)
    expect(compareReleases(undated, early)).toBeGreaterThan(0)
  })

  it(`breaks unshipped ties (same/absent targetDate) by createdAt desc`, () => {
    const older = makeRelease({
      createdAt: new Date(`2026-02-01T10:00:00.000Z`),
    })
    const newer = makeRelease({
      createdAt: new Date(`2026-03-01T10:00:00.000Z`),
    })
    expect(compareReleases(newer, older)).toBeLessThan(0)
    expect(compareReleases(older, newer)).toBeGreaterThan(0)

    const datedOlder = makeRelease({
      targetDate: `2026-03-10`,
      createdAt: new Date(`2026-02-01T10:00:00.000Z`),
    })
    const datedNewer = makeRelease({
      targetDate: `2026-03-10`,
      createdAt: new Date(`2026-03-01T10:00:00.000Z`),
    })
    expect(compareReleases(datedNewer, datedOlder)).toBeLessThan(0)
  })

  it(`sorts shipped by shippedAt desc`, () => {
    const shippedFirst = makeRelease({
      shippedAt: new Date(`2026-03-01T10:00:00.000Z`),
    })
    const shippedLast = makeRelease({
      shippedAt: new Date(`2026-03-08T10:00:00.000Z`),
    })
    expect(compareReleases(shippedLast, shippedFirst)).toBeLessThan(0)
    expect(compareReleases(shippedFirst, shippedLast)).toBeGreaterThan(0)
  })

  it(`compares Electric's space-separated timestamps against ISO strings (EXP-38)`, () => {
    // Electric wire format vs ISO — must compare as instants, not strings.
    const electric = makeRelease({
      shippedAt: `2026-03-08 10:00:00+00`,
    })
    const iso = makeRelease({
      shippedAt: `2026-03-01T10:00:00.000Z`,
    })
    expect(compareReleases(electric, iso)).toBeLessThan(0)
  })

  it(`orders a full mixed list correctly end-to-end`, () => {
    const list = [
      makeRelease({
        shippedAt: new Date(`2026-03-01T10:00:00.000Z`),
        targetDate: `2026-01-01`,
      }),
      makeRelease({
        targetDate: null,
        createdAt: new Date(`2026-03-04T10:00:00.000Z`),
      }),
      makeRelease({ targetDate: `2026-04-01` }),
      makeRelease({
        shippedAt: new Date(`2026-03-06T10:00:00.000Z`),
      }),
      makeRelease({ targetDate: `2026-03-10` }),
    ]
    const sorted = [...list].sort(compareReleases)
    expect(sorted).toEqual([
      list[4], // unshipped, earliest target
      list[2], // unshipped, later target
      list[1], // unshipped, no target
      list[3], // shipped most recently
      list[0], // shipped earlier (target date irrelevant once shipped)
    ])
  })
})
