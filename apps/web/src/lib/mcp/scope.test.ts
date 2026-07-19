import { describe, expect, it } from "vitest"
import {
  buildMcpAccess,
  FULL_ACCESS,
  NO_ACCESS,
  assertFullAccess,
  assertBoardGranted,
  assertTeamFullyGranted,
  assertTeamVisible,
  filterVisibleTeamIds,
  isBoardGranted,
  isTeamFullyGranted,
  isTeamVisible,
} from "@/lib/mcp/scope"

const WS_A = `ws-a`
const WS_B = `ws-b`
const PROJ_A1 = `proj-a1`
const PROJ_B1 = `proj-b1`
const PROJ_B2 = `proj-b2`

// Grant: all of team A, plus a single board inside team B.
const access = buildMcpAccess(
  { allTeams: false, teamIds: [WS_A], boardIds: [PROJ_B1] },
  new Map([[PROJ_B1, WS_B]])
)

describe(`buildMcpAccess`, () => {
  it(`allTeams collapses to FULL_ACCESS`, () => {
    expect(
      buildMcpAccess(
        { allTeams: true, teamIds: [], boardIds: [] },
        new Map()
      )
    ).toBe(FULL_ACCESS)
  })

  it(`derives visibility from team and board grants`, () => {
    expect(access.full).toBe(false)
    expect([...access.fullTeamIds]).toEqual([WS_A])
    expect([...access.grantedBoardIds]).toEqual([PROJ_B1])
    expect([...access.visibleTeamIds].sort()).toEqual([WS_A, WS_B])
  })

  it(`ignores granted boards whose team can't be resolved`, () => {
    const orphan = buildMcpAccess(
      { allTeams: false, teamIds: [], boardIds: [`gone`] },
      new Map()
    )
    expect(orphan.visibleTeamIds.size).toBe(0)
  })
})

describe(`predicates`, () => {
  it(`team visibility: full grant or granted-board host`, () => {
    expect(isTeamVisible(access, WS_A)).toBe(true)
    expect(isTeamVisible(access, WS_B)).toBe(true)
    expect(isTeamVisible(access, `ws-other`)).toBe(false)
  })

  it(`team-level mutations need the whole-team grant`, () => {
    expect(isTeamFullyGranted(access, WS_A)).toBe(true)
    expect(isTeamFullyGranted(access, WS_B)).toBe(false)
  })

  it(`board grants: direct, via team, and denied siblings`, () => {
    expect(isBoardGranted(access, PROJ_A1, WS_A)).toBe(true) // via team
    expect(isBoardGranted(access, PROJ_B1, WS_B)).toBe(true) // direct
    expect(isBoardGranted(access, PROJ_B2, WS_B)).toBe(false) // sibling
  })

  it(`FULL_ACCESS passes everything, NO_ACCESS nothing`, () => {
    expect(isBoardGranted(FULL_ACCESS, `x`, `y`)).toBe(true)
    expect(isTeamFullyGranted(FULL_ACCESS, `y`)).toBe(true)
    expect(isBoardGranted(NO_ACCESS, PROJ_B1, WS_B)).toBe(false)
    expect(isTeamVisible(NO_ACCESS, WS_A)).toBe(false)
  })
})

describe(`assertions`, () => {
  it(`throw actionable errors outside the grant`, () => {
    expect(() => assertTeamVisible(access, `ws-other`)).toThrow(
      /not granted access/
    )
    expect(() => assertTeamFullyGranted(access, WS_B)).toThrow(
      /not granted access/
    )
    expect(() => assertBoardGranted(access, PROJ_B2, WS_B)).toThrow(
      /not granted access/
    )
    expect(() => assertFullAccess(access)).toThrow(/not granted access/)
  })

  it(`pass inside the grant`, () => {
    expect(() => assertTeamVisible(access, WS_B)).not.toThrow()
    expect(() => assertTeamFullyGranted(access, WS_A)).not.toThrow()
    expect(() => assertBoardGranted(access, PROJ_B1, WS_B)).not.toThrow()
    expect(() => assertFullAccess(FULL_ACCESS)).not.toThrow()
  })
})

describe(`filterVisibleTeamIds`, () => {
  it(`intersects with the visible set, passthrough on full`, () => {
    expect(filterVisibleTeamIds(access, [WS_A, WS_B, `ws-x`])).toEqual([
      WS_A,
      WS_B,
    ])
    expect(filterVisibleTeamIds(FULL_ACCESS, [`a`, `b`])).toEqual([
      `a`,
      `b`,
    ])
  })
})
