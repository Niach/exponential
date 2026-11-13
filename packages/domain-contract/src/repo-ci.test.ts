import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/repo-ci.json" with { type: "json" }
import { pipelineRows, repoCiSwitches, requiredSuggestions, runRows, secretName } from "./repo-ci"

// #EXP-1617 E4 part 4: the TS reference replays fixtures/repo-ci.json.
describe(`repo-ci fixture`, () => {
  it.each(fixture.runRows)(`runRows: $name`, (c) => {
    expect(runRows(c.runs, c.jobs, c.now)).toEqual(c.expected as never)
  })
  it.each(fixture.pipelineRows)(`pipelineRows: $name`, (c) => {
    expect(pipelineRows(c.runs)).toEqual(c.expected as never)
  })
  it.each(fixture.requiredSuggestions)(`requiredSuggestions: $name`, (c) => {
    expect(requiredSuggestions(c.runs, c.jobs, c.required)).toEqual(c.expected)
  })
  it.each(fixture.secretName)(`secretName: $input`, (c) => {
    expect(secretName(c.input)).toBe(c.valid)
  })
  it.each(fixture.switches)(`switches: $provider`, (c) => {
    expect(repoCiSwitches(c.provider)).toEqual(c.expected as never)
  })
})
