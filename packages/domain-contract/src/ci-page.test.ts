import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/ci-page.json" with { type: "json" }
import {
  ciBadge,
  ciPageTiles,
  ciRepoOptions,
  ciRunList,
  ciRunnerRows,
  ciTabCounts,
  ciTrainGroups,
  type CiPageInput,
  type CiRunFilter,
} from "./ci-page"

// #EXP-2172: the TS reference replays fixtures/ci-page.json.
const scenario = fixture.scenario as CiPageInput

describe(`ci-page fixture`, () => {
  it.each(fixture.runListCases)(`runList: $name`, (c) => {
    expect(ciRunList(scenario, c.filter as CiRunFilter)).toEqual(c.expected as never)
  })
  it(`repoOptions`, () => expect(ciRepoOptions(scenario)).toEqual(fixture.repoOptions))
  it(`tiles`, () => expect(ciPageTiles(scenario)).toEqual(fixture.tiles as never))
  it(`tabCounts`, () => expect(ciTabCounts(scenario)).toEqual(fixture.tabCounts))
  it.each(fixture.trainCases)(`train: $name`, (c) => {
    expect(ciTrainGroups(scenario, c.queue)).toEqual(c.expected as never)
  })
  it(`runners`, () => expect(ciRunnerRows(scenario)).toEqual(fixture.runners))
  it.each(fixture.badgeCases)(`badge: $name`, (c) => {
    expect(ciBadge(c.input as never)).toEqual(c.expected as never)
  })
})
