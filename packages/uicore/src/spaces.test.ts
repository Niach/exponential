import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/spaces.json" with { type: "json" }
import { spaceRows, spaceView, type SpaceDisplay, type SpaceInput, type SpaceSession } from "./spaces"

describe(`spaces.json rows (#EXP-2024)`, () => {
  test(`has its cases`, () => expect(fixture.rows.length).toBeGreaterThanOrEqual(8))
  for (const c of fixture.rows) test(c.name, () => expect(spaceRows(c.spaces as SpaceInput[], c.sessions as SpaceSession[])).toEqual(c.expected as ReturnType<typeof spaceRows>))
})

describe(`spaces.json view (#EXP-2024)`, () => {
  test(`has its cases`, () => expect(fixture.view.length).toBeGreaterThanOrEqual(10))
  for (const c of fixture.view) test(c.name, () => expect(spaceView(c.space, c.display as SpaceDisplay, c.large)).toEqual(c.expected))
})
