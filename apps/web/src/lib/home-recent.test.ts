import { describe, expect, it } from "vitest"
import { selectHomeRecent, type HomeRecentSession } from "./home-recent"

const run = (
  id: string,
  overrides: Partial<HomeRecentSession> = {}
): HomeRecentSession => ({
  id,
  userId: `me`,
  teamId: `t1`,
  parentSessionId: null,
  updatedAt: new Date(`2026-10-12T10:00:00Z`),
  createdAt: new Date(`2026-10-12T09:00:00Z`),
  ...overrides,
})

describe(`selectHomeRecent`, () => {
  it(`keeps my top-level runs in the team, newest update first, three at most`, () => {
    const rows = [
      run(`a`, { updatedAt: new Date(`2026-10-12T08:00:00Z`) }),
      run(`b`, { updatedAt: new Date(`2026-10-12T12:00:00Z`) }),
      run(`other-user`, { userId: `you` }),
      run(`other-team`, { teamId: `t2` }),
      run(`child`, { parentSessionId: `b` }),
      run(`c`, { updatedAt: new Date(`2026-10-12T11:00:00Z`) }),
      run(`d`, { updatedAt: new Date(`2026-10-12T07:00:00Z`) }),
    ]
    expect(selectHomeRecent(rows, `me`, `t1`).map((row) => row.id)).toEqual([`b`, `c`, `a`])
  })

  it(`breaks an updatedAt tie on createdAt`, () => {
    const rows = [
      run(`older`, { createdAt: new Date(`2026-10-12T01:00:00Z`) }),
      run(`newer`, { createdAt: new Date(`2026-10-12T02:00:00Z`) }),
    ]
    expect(selectHomeRecent(rows, `me`, `t1`).map((row) => row.id)).toEqual([`newer`, `older`])
  })

  it(`is empty without a user or a team`, () => {
    expect(selectHomeRecent([run(`a`)], undefined, `t1`)).toEqual([])
    expect(selectHomeRecent([run(`a`)], `me`, undefined)).toEqual([])
  })
})
