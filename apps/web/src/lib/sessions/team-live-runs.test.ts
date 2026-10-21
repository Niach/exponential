import { describe, expect, it } from "vitest"

import {
  countsAsLiveSession,
  liveRunsByTeam,
  otherTeamsLive,
  type TeamLiveRunSession,
} from "./team-live-runs"

// EXP-1075: the team picker's live signal — the caller's OWN live sessions
// grouped by team, with the active team excluded from the switcher's dot.

const run = (over: Partial<TeamLiveRunSession> = {}): TeamLiveRunSession =>
  ({
    teamId: `team-a`,
    userId: `u-me`,
    status: `running`,
    needsInput: false,
    agentId: null,
    rootId: null,
    ...over,
  }) as TeamLiveRunSession

const summary = (map: Map<string, { count: number; needsInput: boolean }>) =>
  Object.fromEntries(map)

describe(`liveRunsByTeam`, () => {
  it(`groups the caller's live runs by team`, () => {
    const map = liveRunsByTeam(
      [run(), run(), run({ teamId: `team-b` })],
      { me: `u-me` }
    )
    expect(summary(map)).toEqual({
      "team-a": { count: 2, needsInput: false },
      "team-b": { count: 1, needsInput: false },
    })
  })

  it(`ignores a teammate's run`, () => {
    const map = liveRunsByTeam([run({ userId: `u-lisa` })], { me: `u-me` })
    expect(map.size).toBe(0)
  })

  it(`drops an ended row and counts queued and idle ones`, () => {
    const map = liveRunsByTeam(
      [
        run({ status: `ended` }),
        run({ teamId: `team-b`, status: `queued` }),
        run({ teamId: `team-b`, status: `idle` }),
      ],
      { me: `u-me` }
    )
    expect(summary(map)).toEqual({
      "team-b": { count: 2, needsInput: false },
    })
  })

  it(`skips team agents and every session under one`, () => {
    const map = liveRunsByTeam(
      [
        run({ agentId: `agent-def-1` }),
        run({ rootId: `agent-1` }),
        run({ teamId: `team-b`, rootId: `thread-1` }),
      ],
      { me: `u-me`, agentRootIds: new Set([`agent-1`]) }
    )
    expect(summary(map)).toEqual({
      "team-b": { count: 1, needsInput: false },
    })
  })

  it(`lifts needsInput only on the team that waits`, () => {
    const map = liveRunsByTeam(
      [run({ needsInput: true }), run({ teamId: `team-b` })],
      { me: `u-me` }
    )
    expect(summary(map)).toEqual({
      "team-a": { count: 1, needsInput: true },
      "team-b": { count: 1, needsInput: false },
    })
  })
})

describe(`otherTeamsLive`, () => {
  const map = liveRunsByTeam(
    [run(), run({ teamId: `team-b`, needsInput: true })],
    { me: `u-me` }
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
        liveRunsByTeam([run()], { me: `u-me` }),
        `team-a`
      )
    ).toEqual({ any: false, needsInput: false })
  })
})

describe(`countsAsLiveSession`, () => {
  const agents = new Set([`agent-1`])

  it(`counts a live thread root and its children`, () => {
    expect(countsAsLiveSession(run(), agents)).toBe(true)
    expect(countsAsLiveSession(run({ rootId: `thread-1` }), agents)).toBe(true)
  })

  it(`never counts an agent root, its children or an ended row`, () => {
    expect(countsAsLiveSession(run({ agentId: `agent-def-1` }), agents)).toBe(false)
    expect(countsAsLiveSession(run({ rootId: `agent-1` }), agents)).toBe(false)
    expect(countsAsLiveSession(run({ status: `ended` }), agents)).toBe(false)
  })
})
