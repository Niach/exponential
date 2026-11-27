import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/ci-run.json" with { type: "json" }
import runner from "../fixtures/runner-settings.json" with { type: "json" }
import {
  ciDefaultJob,
  ciFixPrompt,
  ciJobRows,
  ciLogView,
  ciRunActions,
  ciRunStatus,
  ciFailureSummary,
  ciJobsCaption,
  ciJobSummary,
  ciRunChips,
  ciSteps,
  normalizeRunnerLabel,
  runnerNotice,
  runnerUnsupported,
  runnerOsLabel,
  runnerRecentJobs,
  type RunnerClient,
} from "./ci-run"

// #EXP-1804: the TS reference replays fixtures/ci-run.json and runner-settings.json.
describe(`ci-run fixture`, () => {
  it.each(fixture.runStatus)(`runStatus: $name`, (c) => {
    expect(ciRunStatus(c.state, c.jobs)).toEqual(c.expected)
  })
  it.each(fixture.jobsCaption)(`jobsCaption: $name`, (c) => expect(ciJobsCaption(c.jobs)).toBe(c.expected))
  it.each(fixture.jobSummary)(`jobSummary: $name`, (c) => expect(ciJobSummary(c.job as never, c.now)).toBe(c.expected))
  it.each(fixture.chips)(`chips: $name`, (c) => expect(ciRunChips(c as never)).toEqual(c.expected as never))
  it.each(fixture.failureSummary)(`failureSummary: $name`, (c) => expect(ciFailureSummary(c.job, c.log)).toEqual(c.expected as never))
  it.each(fixture.jobRows)(`jobRows: $name`, (c) => {
    expect(ciJobRows(c.jobs, c.now)).toEqual(c.expected)
  })
  it.each(fixture.defaultJob)(`defaultJob: $name`, (c) => {
    expect(ciDefaultJob(c.jobs, c.requested)).toBe(c.expected)
  })
  it.each(fixture.steps)(`steps: $name`, (c) => {
    expect(ciSteps(c.steps, c.now)).toEqual(c.expected)
  })
  it.each(fixture.logView)(`logView: $name`, (c) => {
    expect(ciLogView(c.text)).toEqual(c.expected)
  })
  it.each(fixture.actions)(`actions: $name`, (c) => {
    expect(ciRunActions(c)).toEqual(c.expected)
  })
  it.each(fixture.fixPrompt)(`fixPrompt: $name`, (c) => {
    expect(ciFixPrompt(c.input)).toBe(c.expected)
  })
  it(`the specimen selects its failing job`, () => {
    const s = fixture.specimen
    expect(ciDefaultJob(s.jobs, null)).toBe(s.selected)
    expect(ciLogView(s.log).highlight.length).toBe(1)
  })
})

describe(`runner-settings fixture`, () => {
  it.each(runner.normalizeLabel)(`normalizeLabel: $input`, (c) => {
    expect(normalizeRunnerLabel(c.input)).toBe(c.expected)
  })
  it.each(runner.notice)(`notice: $name`, (c) => {
    expect(runnerNotice(c.client as RunnerClient, c.caps, c.kind, c.platform)).toBe(c.expected as never)
  })
  it.each(runner.unsupported)(`unsupported: $name`, (c) => {
    expect(runnerUnsupported(c.kind, c.platform)).toBe(c.expected)
  })
  it.each(runner.osLabel)(`osLabel: $platform`, (c) => {
    expect(runnerOsLabel(c.platform)).toBe(c.expected)
  })
  it.each(runner.recentJobs)(`recentJobs: $name`, (c) => {
    expect(runnerRecentJobs(c.deviceId, c.jobs, c.now)).toEqual(c.expected)
  })
})
