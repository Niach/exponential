import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/pr-checks.json" with { type: "json" }
import {
  aggregateChecksState,
  checkStatusFromGithub,
  checksBrief,
  checksList,
  checksSummary,
  failingChecksBody,
  formatCheckDuration,
  mergePill,
  checksQueuedText,
  checksWakeText,
  mergeGate,
  mergeGateText,
  type PrChecksState,
} from "./pr-checks"

// EXP-1610: the TS reference replays fixtures/pr-checks.json.
describe(`pr-checks fixture`, () => {
  it.each(fixture.aggregate)(`aggregate: $name`, ({ checks, expected }) => {
    expect(aggregateChecksState(checks)).toBe(expected as PrChecksState)
  })
  it.each(fixture.githubStatus)(`github status $github`, ({ github, expected }) => {
    expect(checkStatusFromGithub(github)).toBe(expected as never)
  })
  it.each(fixture.gate)(`gate: $name`, ({ checks, expected }) => {
    expect(mergeGate(checks)).toEqual(expected as never)
  })
  it.each(fixture.wakeText)(`wake text $state`, ({ number, state, failing, expected }) => {
    expect(checksWakeText(number, state as `success` | `failure`, failing)).toBe(expected)
  })
  it.each(fixture.queuedText)(`queued text`, ({ number, names, expected }) => {
    expect(checksQueuedText(number, names)).toBe(expected)
  })
  it.each(fixture.gateText)(`gate text`, ({ number, verdict, expected }) => {
    expect(mergeGateText(number, verdict as never)).toBe(expected)
  })
  it.each(fixture.brief)(`brief`, ({ state, checks, expected }) => {
    expect(checksBrief(state as PrChecksState, checks)).toEqual(expected)
  })
})

// #EXP-1611: the UI half.
describe(`pr-checks UI fixture`, () => {
  it.each(fixture.mergePill)(`merge pill $input.state stack=$input.stack`, ({ input, expected }) => {
    expect(mergePill(input)).toEqual(expected as never)
  })
  it.each(fixture.formatDuration)(`duration $ms`, ({ ms, expected }) => {
    expect(formatCheckDuration(ms)).toBe(expected)
  })
  it.each(fixture.checksList)(`list: $name`, ({ now, checks, expected }) => {
    expect(checksList(checks, now)).toEqual(expected as never)
  })
  it.each(fixture.checksSummary)(`summary: $name`, ({ checks, expected }) => {
    expect(checksSummary(checks)).toEqual(expected as never)
  })
  it.each(fixture.failingBody)(`failing body`, ({ checks, expected }) => {
    expect(failingChecksBody(checks)).toBe(expected)
  })
  it(`the specimen = the first list case`, () => {
    expect(fixture.checksList[0]?.checks).toEqual(fixture.specimen.checks)
  })
})
