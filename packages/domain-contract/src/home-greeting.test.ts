import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/composer-home.json" with { type: "json" }
import { homeGreeting, localWallClock } from "./home-greeting"

describe(`homeGreeting`, () => {
  test(`every hour of the day lands in exactly one bucket`, () => {
    for (let hour = 0; hour < 24; hour++) {
      const hits = fixture.buckets.filter((bucket) =>
        bucket.from <= bucket.to
          ? hour >= bucket.from && hour <= bucket.to
          : hour >= bucket.from || hour <= bucket.to
      )
      expect(hits.length).toBe(1)
    }
  })

  test(`every variant drops cleanly without a name`, () => {
    for (const bucket of fixture.buckets) {
      for (const variant of bucket.variants) {
        expect(variant.replace(`, {name}`, ``)).not.toContain(`{name}`)
      }
    }
  })

  test(`localWallClock pads`, () => {
    expect(localWallClock(new Date(2026, 0, 5, 7, 3))).toBe(`2026-01-05T07:03`)
  })

  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(homeGreeting(entry.local, entry.user)).toBe(entry.expected)
    })
  }
})
