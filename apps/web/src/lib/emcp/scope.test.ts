import { describe, expect, it } from "vitest"
import {
  buildMcpAccess,
  clampMcpAccessToTeam,
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
  reporterFallbackAllowed,
} from "@/lib/emcp/scope"

// FEED-76: the same confinement behind a scoped API key picks its own
// denial wording; both keep "not granted access".
describe(`origin (FEED-76)`, () => {
  const grant = { allTeams: false, teamIds: [`ws-a`], boardIds: [] }

  it(`stamps the origin, keeping FULL_ACCESS identity for allTeams`, () => {
    expect(buildMcpAccess(grant, new Map(), `apiKey`).origin).toBe(`apiKey`)
    expect(buildMcpAccess(grant, new Map()).origin).toBe(`oauth`)
    expect(
      buildMcpAccess({ allTeams: true, teamIds: [], boardIds: [] }, new Map(), `apiKey`)
    ).toBe(FULL_ACCESS)
  })

  it(`an api-key denial points at a new key, an OAuth one at re-consenting`, () => {
    const key = buildMcpAccess(grant, new Map(), `apiKey`)
    expect(() => assertTeamVisible(key, `ws-x`)).toThrow(
      /This API key was not granted access to this team.*Settings → Security/
    )
    expect(() => assertFullAccess(key)).toThrow(/API key.*not granted access/)
    const oauth = buildMcpAccess(grant, new Map())
    expect(() => assertTeamVisible(oauth, `ws-x`)).toThrow(
      /This MCP connection was not granted access.*consent screen/
    )
  })
})

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

// #EXP-1383: an agent-tree session is confined to its own team.
describe(`clampMcpAccessToTeam`, () => {
  it(`turns full access into the one team, worded for the agent`, () => {
    const clamped = clampMcpAccessToTeam(FULL_ACCESS, `ws-a`)
    expect(clamped.full).toBe(false)
    expect(isTeamFullyGranted(clamped, `ws-a`)).toBe(true)
    expect(isTeamVisible(clamped, `ws-b`)).toBe(false)
    expect(isBoardGranted(clamped, `b-1`, `ws-a`)).toBe(true)
    expect(isBoardGranted(clamped, `b-2`, `ws-b`)).toBe(false)
    expect(() => assertTeamVisible(clamped, `ws-b`)).toThrow(/agent session was not granted access/)
    expect(() => assertFullAccess(clamped)).toThrow(/not granted access/)
  })

  it(`keeps only what a confined grant held inside the team`, () => {
    const access = buildMcpAccess(
      { allTeams: false, teamIds: [`ws-b`], boardIds: [`b-a`, `b-c`] },
      new Map([
        [`b-a`, `ws-a`],
        [`b-c`, `ws-c`],
      ])
    )
    const clamped = clampMcpAccessToTeam(access, `ws-a`, new Map([[`b-a`, `ws-a`], [`b-c`, `ws-c`]]))
    expect([...clamped.fullTeamIds]).toEqual([])
    expect([...clamped.grantedBoardIds]).toEqual([`b-a`])
    expect([...clamped.visibleTeamIds]).toEqual([`ws-a`])
    expect(isTeamFullyGranted(clamped, `ws-b`)).toBe(false)
    expect(isBoardGranted(clamped, `b-c`, `ws-c`)).toBe(false)
    // A grant that never reached the team reaches nothing there.
    const none = clampMcpAccessToTeam(buildMcpAccess({ allTeams: false, teamIds: [`ws-b`], boardIds: [] }, new Map()), `ws-a`)
    expect(isTeamVisible(none, `ws-a`)).toBe(false)
  })
})

// NEW-85: the widget reporter fallback sits outside every team/board
// selection, so only the person's own full-access credential in an attended
// session (or no session header at all) may take it.
describe(`reporterFallbackAllowed (NEW-85)`, () => {
  const boardOnly = buildMcpAccess(
    { allTeams: false, teamIds: [], boardIds: [`b-1`] },
    new Map([[`b-1`, `ws-a`]])
  )
  const scopedKey = buildMcpAccess(
    { allTeams: false, teamIds: [`ws-a`], boardIds: [] },
    new Map(),
    `apiKey`
  )
  const agent = clampMcpAccessToTeam(FULL_ACCESS, `ws-a`)

  it(`allows full access with no session or an attended one`, () => {
    expect(reporterFallbackAllowed(FULL_ACCESS, null)).toBe(true)
    expect(reporterFallbackAllowed(FULL_ACCESS, { unattended: false })).toBe(true)
  })

  it(`refuses an unattended run even on full access`, () => {
    expect(reporterFallbackAllowed(FULL_ACCESS, { unattended: true })).toBe(false)
  })

  it(`refuses every confined credential`, () => {
    for (const access of [boardOnly, scopedKey, agent, NO_ACCESS]) {
      expect(reporterFallbackAllowed(access, null)).toBe(false)
      expect(reporterFallbackAllowed(access, { unattended: false })).toBe(false)
    }
  })
})
