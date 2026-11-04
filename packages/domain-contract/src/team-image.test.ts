import { describe, expect, it } from "vitest"
import fixture from "../fixtures/team-image.json" with { type: "json" }
import { teamImageUploadPath, teamMark } from "./team-image"

describe(`teamMark`, () => {
  for (const { iconUrl, mark } of fixture.markCases) {
    it(`${JSON.stringify(iconUrl)} = ${mark.kind}`, () => {
      expect(teamMark(iconUrl)).toEqual(mark)
    })
  }
})

describe(`teamImageUploadPath`, () => {
  it(`names the team`, () => {
    expect(teamImageUploadPath(`t1`)).toBe(`/api/teams/t1/icon`)
  })
})
