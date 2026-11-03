import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/team-zone.json" with { type: "json" }
import { resolveZone, zoneOffset } from "./team-zone"

describe(`team zone (VAPP-201)`, () => {
  test.each(fixture.zoneCases)(`$zone at $at`, (c) => {
    expect(new Date(c.at).getTime()).toBe(c.ms)
    expect(zoneOffset(c.zone, c.ms)).toBe(c.offset)
  })
  test.each(fixture.resolveCases)(`resolveZone: $name`, (c) => {
    expect(resolveZone(c.teamZone)).toBe(c.zone)
  })
})
