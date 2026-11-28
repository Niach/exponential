import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/ci-page.json" with { type: "json" }
import {
  ciBadge,
  ciDayTitle,
  ciPrGroups,
  ciQueue,
  ciRepoOptions,
  ciRunGroups,
  ciRunContext,
  ciRunKind,
  ciRunnerChip,
  ciRunnerRows,
  ciTabCounts,
  ciTrainEta,
  ciTypicalTrainMs,
  type CiPageInput,
  type CiPageRun,
  type CiRunFilter,
} from "./ci-page"

// #EXP-2172 / #EXP-2214: the TS reference replays fixtures/ci-page.json.
const scenario = fixture.scenario as unknown as CiPageInput

describe(`ci-page fixture`, () => {
  it.each(fixture.runGroupsCases)(`runGroups: $name`, (c) => {
    expect(ciRunGroups(scenario, c.filter as CiRunFilter)).toEqual(c.expected as never)
  })
  it.each(fixture.releaseGroupsCases)(`releases: $name`, (c) => {
    expect(ciRunGroups(scenario, c.filter as CiRunFilter, [`release`])).toEqual(c.expected as never)
  })
  it.each(fixture.prGroupsCases)(`pull requests: $name`, (c) => {
    expect(ciPrGroups(scenario, c.filter as CiRunFilter)).toEqual(c.expected as never)
  })
  it.each(fixture.queueCases)(`merge queue: $name`, (c) => {
    expect(ciQueue({ ...scenario, ...((c.input ?? {}) as Partial<CiPageInput>) })).toEqual(c.expected as never)
  })
  it.each(fixture.kindCases)(`kind: $name`, (c) => {
    expect(ciRunKind(c.run as unknown as CiPageRun, c.repos)).toBe(c.expected as never)
  })
  it.each(fixture.runContextCases)(`run context: $run`, (c) => {
    expect(ciRunContext(scenario.runs.find((r) => r.id === c.run)!, scenario)).toEqual(c.expected as never)
  })
  it(`typical train`, () => {
    for (const [repo, ms] of Object.entries(fixture.typicalTrainMs)) expect(ciTypicalTrainMs(scenario, repo)).toBe(ms)
  })
  it.each(fixture.etaCases)(`eta: $typicalMs / $elapsedMs`, (c) => {
    expect(ciTrainEta(c.typicalMs, c.elapsedMs)).toEqual(c.expected as never)
  })
  it.each(fixture.dayTitleCases)(`dayTitle: $day`, (c) => {
    expect(ciDayTitle(c.day, c.now, c.utcOffsetMinutes)).toBe(c.expected)
  })
  it(`runnerChip`, () => expect(ciRunnerChip(scenario)).toEqual(fixture.runnerChip))
  it(`repoOptions`, () => expect(ciRepoOptions(scenario)).toEqual(fixture.repoOptions))
  it(`tabCounts`, () => expect(ciTabCounts(scenario)).toEqual(fixture.tabCounts))
  it(`runners`, () => expect(ciRunnerRows(scenario)).toEqual(fixture.runners))
  it.each(fixture.badgeCases)(`badge: $name`, (c) => {
    expect(ciBadge(c.input as never)).toEqual(c.expected as never)
  })
})
