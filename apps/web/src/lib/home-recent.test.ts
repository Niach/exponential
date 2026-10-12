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
  resumedFromId: null,
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

  // Review #20 (composer-home.json): a resume succession is ONE row, its
  // newest member, so the band shows three DIFFERENT pieces of work.
  it(`folds a resume succession into its newest member`, () => {
    const rows = [
      run(`first`, {
        createdAt: new Date(`2026-10-12T08:00:00Z`),
        updatedAt: new Date(`2026-10-12T08:30:00Z`),
      }),
      run(`second`, {
        resumedFromId: `first`,
        createdAt: new Date(`2026-10-12T09:00:00Z`),
        updatedAt: new Date(`2026-10-12T09:30:00Z`),
      }),
      run(`third`, {
        resumedFromId: `second`,
        createdAt: new Date(`2026-10-12T10:00:00Z`),
        updatedAt: new Date(`2026-10-12T10:30:00Z`),
      }),
      run(`x`, { updatedAt: new Date(`2026-10-12T07:00:00Z`) }),
      run(`y`, { updatedAt: new Date(`2026-10-12T06:00:00Z`) }),
    ]
    expect(selectHomeRecent(rows, `me`, `t1`).map((row) => row.id)).toEqual([`third`, `x`, `y`])
  })

  it(`sorts a folded succession by its newest member's stamps`, () => {
    const rows = [
      run(`old`, { updatedAt: new Date(`2026-10-12T12:00:00Z`) }),
      run(`resumed`, {
        resumedFromId: `old`,
        createdAt: new Date(`2026-10-12T10:00:00Z`),
        updatedAt: new Date(`2026-10-12T05:00:00Z`),
      }),
      run(`other`, { updatedAt: new Date(`2026-10-12T06:00:00Z`) }),
    ]
    expect(selectHomeRecent(rows, `me`, `t1`).map((row) => row.id)).toEqual([`other`, `resumed`])
  })
})
