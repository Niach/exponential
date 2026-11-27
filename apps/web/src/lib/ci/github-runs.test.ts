import { describe, expect, it } from "vitest"
import { githubLogText, githubRunState, stepsFromGithub, triggerFromGithub } from "./github-runs"

describe(`GitHub Actions rows (#EXP-2172)`, () => {
  it(`rolls a run up like our runs`, () => {
    expect(githubRunState(`queued`, null)).toBe(`pending`)
    expect(githubRunState(`in_progress`, null)).toBe(`pending`)
    expect(githubRunState(`completed`, `success`)).toBe(`success`)
    expect(githubRunState(`completed`, `skipped`)).toBe(`success`)
    expect(githubRunState(`completed`, `failure`)).toBe(`failure`)
    expect(githubRunState(`completed`, `cancelled`)).toBe(`failure`)
  })

  it(`names GitHub's events with our triggers`, () => {
    expect(triggerFromGithub(`push`)).toBe(`push`)
    expect(triggerFromGithub(`pull_request_target`)).toBe(`pull_request`)
    expect(triggerFromGithub(`workflow_dispatch`)).toBe(`manual`)
    expect(triggerFromGithub(`schedule`)).toBe(`schedule`)
    expect(triggerFromGithub(`merge_group`)).toBe(`merge_train`)
    expect(triggerFromGithub(`check_suite_requested_x`)).toHaveLength(16)
  })

  it(`maps steps to the runner's step states`, () => {
    expect(
      stepsFromGithub([
        { name: `Checkout`, status: `completed`, conclusion: `success`, started_at: `2026-11-27T10:00:00Z`, completed_at: `2026-11-27T10:00:04Z` },
        { name: `Test`, status: `in_progress`, conclusion: null, started_at: `2026-11-27T10:00:04Z`, completed_at: null },
        { name: `Upload`, status: `queued` },
      ])
    ).toEqual([
      { name: `Checkout`, status: `completed`, conclusion: `success`, startedAt: Date.parse(`2026-11-27T10:00:00Z`), completedAt: Date.parse(`2026-11-27T10:00:04Z`) },
      { name: `Test`, status: `in_progress`, conclusion: null, startedAt: Date.parse(`2026-11-27T10:00:04Z`), completedAt: null },
      { name: `Upload`, status: `queued`, conclusion: null, startedAt: null, completedAt: null },
    ])
  })

  it(`drops GitHub's timestamps so the log view reads the markers`, () => {
    const raw = `﻿2026-11-27T10:00:00.1234567Z ##[group]Run bun test\r\n2026-11-27T10:00:01.0000000Z ok\n2026-11-27T10:00:02Z ##[error]Process completed with exit code 1.`
    expect(githubLogText(raw)).toBe(`##[group]Run bun test\nok\n##[error]Process completed with exit code 1.`)
  })
})
