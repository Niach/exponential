import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/ci-page.json" with { type: "json" }
import {
  ciBadge,
  ciDayTitle,
  ciRepoOptions,
  ciRunGroups,
  ciRunnerChip,
  ciRunnerRows,
  ciTabCounts,
  ciTrainGroups,
  type CiPageInput,
  type CiRunFilter,
} from "./ci-page"

// #EXP-2172: the TS reference replays fixtures/ci-page.json.
const scenario = fixture.scenario as CiPageInput

describe(`ci-page fixture`, () => {
  it.each(fixture.runGroupsCases)(`runGroups: $name`, (c) => {
    expect(ciRunGroups(scenario, c.filter as CiRunFilter)).toEqual(c.expected as never)
  })
  it.each(fixture.dayTitleCases)(`dayTitle: $day`, (c) => {
    expect(ciDayTitle(c.day, c.now, c.utcOffsetMinutes)).toBe(c.expected)
  })
  it(`runnerChip`, () => expect(ciRunnerChip(scenario)).toEqual(fixture.runnerChip))
  it(`repoOptions`, () => expect(ciRepoOptions(scenario)).toEqual(fixture.repoOptions))
  it(`tabCounts`, () => expect(ciTabCounts(scenario)).toEqual(fixture.tabCounts))
  it.each(fixture.trainCases)(`train: $name`, (c) => {
    expect(ciTrainGroups(scenario, c.queue)).toEqual(c.expected as never)
  })
  it(`runners`, () => expect(ciRunnerRows(scenario)).toEqual(fixture.runners))
  it.each(fixture.badgeCases)(`badge: $name`, (c) => {
    expect(ciBadge(c.input as never)).toEqual(c.expected as never)
  })
})
