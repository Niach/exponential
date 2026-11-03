import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/team-zone.json" with { type: "json" }
import { resolveZone, zoneDay, zoneOffset } from "./team-zone"

describe(`team zone (VAPP-201)`, () => {
  test.each(fixture.zoneCases)(`$zone at $at`, (c) => {
    expect(new Date(c.at).getTime()).toBe(c.ms)
    expect(zoneOffset(c.zone, c.ms)).toBe(c.offset)
  })
  test.each(fixture.resolveCases)(`resolveZone: $name`, (c) => {
    expect(resolveZone(c.teamZone)).toBe(c.zone)
  })
})

describe(`zoneDay (dogfood G)`, () => {
  test(`the day under the zone, not UTC's`, () => {
    const ms = Date.parse(`2026-11-03T11:15:00Z`)
    expect(zoneDay((at) => zoneOffset(`Pacific/Kiritimati`, at), ms)).toBe(`2026-11-04`)
    expect(zoneDay((at) => zoneOffset(`America/Los_Angeles`, at), Date.parse(`2026-11-01T06:30:00Z`))).toBe(`2026-10-31`)
    expect(zoneDay(() => 0, ms)).toBe(`2026-11-03`)
  })
})
