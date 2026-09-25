import { describe, expect, it } from "vitest"

import {
  liveRunsByTeam,
  otherTeamsLive,
  type TeamLiveRunSession,
} from "./team-live-runs"

// EXP-1075: the team picker's live-run signal — the caller's OWN live runs
// grouped by team, with the active team excluded from the switcher's dot.

const NOW = new Date(`2026-09-25T12:00:00Z`)
const FRESH = new Date(`2026-09-25T11:59:00Z`)
const DEAD = new Date(`2026-09-25T09:00:00Z`)

const run = (over: Partial<TeamLiveRunSession> = {}): TeamLiveRunSession =>
  ({
    teamId: `team-a`,
    userId: `u-me`,
    status: `running`,
    endedBy: null,
    needsInput: false,
    updatedAt: FRESH,
    ...over,
  }) as TeamLiveRunSession

const summary = (map: Map<string, { count: number; needsInput: boolean }>) =>
  Object.fromEntries(map)

describe(`liveRunsByTeam`, () => {
  it(`groups the caller's live runs by team`, () => {
    const map = liveRunsByTeam(
      [run(), run(), run({ teamId: `team-b` })],
      { me: `u-me`, now: NOW }
    )
    expect(summary(map)).toEqual({
      "team-a": { count: 2, needsInput: false },
      "team-b": { count: 1, needsInput: false },
    })
  })

  it(`ignores a teammate's run`, () => {
    const map = liveRunsByTeam([run({ userId: `u-lisa` })], {
      me: `u-me`,
      now: NOW,
    })
    expect(map.size).toBe(0)
  })

  it(`drops an ended row and a heartbeat-stale row`, () => {
    const map = liveRunsByTeam(
      [
        run({ status: `ended`, endedBy: `user` }),
        run({ teamId: `team-b`, updatedAt: DEAD }),
      ],
      { me: `u-me`, now: NOW }
    )
    expect(map.size).toBe(0)
  })

  it(`keeps a swept row (EXP-888: ended + stale, fresh heartbeat)`, () => {
    const map = liveRunsByTeam(
      [run({ status: `ended`, endedBy: `stale` })],
      { me: `u-me`, now: NOW }
    )
    expect(summary(map)).toEqual({
      "team-a": { count: 1, needsInput: false },
    })
  })

  it(`lifts needsInput only on the team that waits`, () => {
    const map = liveRunsByTeam(
      [run({ needsInput: true }), run({ teamId: `team-b` })],
      { me: `u-me`, now: NOW }
    )
    expect(summary(map)).toEqual({
      "team-a": { count: 1, needsInput: true },
      "team-b": { count: 1, needsInput: false },
    })
  })

  it(`never reads needs_input off an in_review run`, () => {
    const map = liveRunsByTeam(
      [run({ status: `in_review`, needsInput: true })],
      { me: `u-me`, now: NOW }
    )
    expect(summary(map)).toEqual({
      "team-a": { count: 1, needsInput: false },
    })
  })
})

describe(`otherTeamsLive`, () => {
  const map = liveRunsByTeam(
    [run(), run({ teamId: `team-b`, needsInput: true })],
    { me: `u-me`, now: NOW }
  )

  it(`excludes the active team`, () => {
    expect(otherTeamsLive(map, `team-b`)).toEqual({
      any: true,
      needsInput: false,
    })
  })

  it(`reports amber when another team waits on the person`, () => {
    expect(otherTeamsLive(map, `team-a`)).toEqual({
      any: true,
      needsInput: true,
    })
  })

  it(`is silent with nothing outside the active team`, () => {
    expect(
      otherTeamsLive(
        liveRunsByTeam([run()], { me: `u-me`, now: NOW }),
        `team-a`
      )
    ).toEqual({ any: false, needsInput: false })
  })
})
