import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/pr-checks.json" with { type: "json" }
import {
  aggregateChecksState,
  checkStatusFromGithub,
  checksBrief,
  checksList,
  checksSummary,
  formatCheckDuration,
  checksQueuedText,
  checksWakeState,
  gatedChecksState,
  requiredChecksState,
  checksWakeText,
  type PrChecksState,
  type PrChecksUnknown,
} from "./pr-checks"

// EXP-1610: the TS reference replays fixtures/pr-checks.json.
describe(`pr-checks fixture`, () => {
  it.each(fixture.aggregate)(`aggregate: $name`, ({ checks, expected }) => {
    expect(aggregateChecksState(checks)).toBe(expected as PrChecksState)
  })
  it(`gated state: an informational leg never holds a PR (#EXP-1711)`, () => {
    const c = (name: string, status: string, conclusion: string | null, required: boolean) => ({ name, status, conclusion, required })
    expect(gatedChecksState([c(`gate`, `completed`, `success`, true), c(`ios-core`, `queued`, null, false)])).toBe(`success`)
    expect(gatedChecksState([c(`gate`, `in_progress`, null, true), c(`lint`, `completed`, `success`, false)])).toBe(`pending`)
    expect(gatedChecksState([c(`gate`, `completed`, `success`, true), c(`ios-core`, `completed`, `failure`, false)])).toBe(`success`)
    expect(gatedChecksState([c(`lint`, `queued`, null, false)])).toBe(`pending`)
    expect(gatedChecksState([])).toBe(`none`)
    // #FEED-221: the names decide, whatever the flags say.
    expect(requiredChecksState([c(`gate`, `completed`, `success`, false), c(`ios-core`, `queued`, null, false)], [`gate`])).toBe(`success`)
    expect(requiredChecksState([c(`ios-core`, `queued`, null, false)], [`gate`])).toBe(`pending`)
    expect(requiredChecksState([c(`gate`, `completed`, `failure`, false)], [`gate`])).toBe(`failure`)
    expect(requiredChecksState([c(`lint`, `queued`, null, false)], [])).toBe(`pending`)
  })
  it.each(fixture.wake)(`wake: $name`, ({ checks, required, expected }) => {
    expect(checksWakeState(checks, required)?.state ?? null).toBe(expected as never)
  })
  it.each(fixture.githubStatus)(`github status $github`, ({ github, expected }) => {
    expect(checkStatusFromGithub(github)).toBe(expected as never)
  })
  it.each(fixture.wakeText)(`wake text $state`, ({ number, state, failing, expected }) => {
    expect(checksWakeText(number, state as `success` | `failure`, failing)).toBe(expected)
  })
  it.each(fixture.queuedText)(`queued text`, ({ number, names, expected }) => {
    expect(checksQueuedText(number, names)).toBe(expected)
  })
  it.each(fixture.brief)(`brief`, ({ state, checks, unknown, expected }) => {
    expect(checksBrief(state as PrChecksState, checks, unknown as PrChecksUnknown | undefined)).toEqual(expected)
  })
})

// #EXP-1611: the UI half.
describe(`pr-checks UI fixture`, () => {
  it.each(fixture.formatDuration)(`duration $ms`, ({ ms, expected }) => {
    expect(formatCheckDuration(ms)).toBe(expected)
  })
  it.each(fixture.checksList)(`list: $name`, ({ now, checks, expected }) => {
    expect(checksList(checks, now)).toEqual(expected as never)
  })
  it.each(fixture.checksSummary)(`summary: $name`, ({ checks, expected }) => {
    expect(checksSummary(checks)).toEqual(expected as never)
  })
  it(`the specimen = the first list case`, () => {
    expect(fixture.checksList[0]?.checks).toEqual(fixture.specimen.checks)
  })
})
