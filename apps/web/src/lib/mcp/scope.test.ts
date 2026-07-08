import { describe, expect, it } from "vitest"
import {
  buildMcpAccess,
  FULL_ACCESS,
  NO_ACCESS,
  assertFullAccess,
  assertProjectGranted,
  assertWorkspaceFullyGranted,
  assertWorkspaceVisible,
  filterVisibleWorkspaceIds,
  isProjectGranted,
  isWorkspaceFullyGranted,
  isWorkspaceVisible,
} from "@/lib/mcp/scope"

const WS_A = `ws-a`
const WS_B = `ws-b`
const PROJ_A1 = `proj-a1`
const PROJ_B1 = `proj-b1`
const PROJ_B2 = `proj-b2`

// Grant: all of workspace A, plus a single project inside workspace B.
const access = buildMcpAccess(
  { allWorkspaces: false, workspaceIds: [WS_A], projectIds: [PROJ_B1] },
  new Map([[PROJ_B1, WS_B]])
)

describe(`buildMcpAccess`, () => {
  it(`allWorkspaces collapses to FULL_ACCESS`, () => {
    expect(
      buildMcpAccess(
        { allWorkspaces: true, workspaceIds: [], projectIds: [] },
        new Map()
      )
    ).toBe(FULL_ACCESS)
  })

  it(`derives visibility from workspace and project grants`, () => {
    expect(access.full).toBe(false)
    expect([...access.fullWorkspaceIds]).toEqual([WS_A])
    expect([...access.grantedProjectIds]).toEqual([PROJ_B1])
    expect([...access.visibleWorkspaceIds].sort()).toEqual([WS_A, WS_B])
  })

  it(`ignores granted projects whose workspace can't be resolved`, () => {
    const orphan = buildMcpAccess(
      { allWorkspaces: false, workspaceIds: [], projectIds: [`gone`] },
      new Map()
    )
    expect(orphan.visibleWorkspaceIds.size).toBe(0)
  })
})

describe(`predicates`, () => {
  it(`workspace visibility: full grant or granted-project host`, () => {
    expect(isWorkspaceVisible(access, WS_A)).toBe(true)
    expect(isWorkspaceVisible(access, WS_B)).toBe(true)
    expect(isWorkspaceVisible(access, `ws-other`)).toBe(false)
  })

  it(`workspace-level mutations need the whole-workspace grant`, () => {
    expect(isWorkspaceFullyGranted(access, WS_A)).toBe(true)
    expect(isWorkspaceFullyGranted(access, WS_B)).toBe(false)
  })

  it(`project grants: direct, via workspace, and denied siblings`, () => {
    expect(isProjectGranted(access, PROJ_A1, WS_A)).toBe(true) // via workspace
    expect(isProjectGranted(access, PROJ_B1, WS_B)).toBe(true) // direct
    expect(isProjectGranted(access, PROJ_B2, WS_B)).toBe(false) // sibling
  })

  it(`FULL_ACCESS passes everything, NO_ACCESS nothing`, () => {
    expect(isProjectGranted(FULL_ACCESS, `x`, `y`)).toBe(true)
    expect(isWorkspaceFullyGranted(FULL_ACCESS, `y`)).toBe(true)
    expect(isProjectGranted(NO_ACCESS, PROJ_B1, WS_B)).toBe(false)
    expect(isWorkspaceVisible(NO_ACCESS, WS_A)).toBe(false)
  })
})

describe(`assertions`, () => {
  it(`throw actionable errors outside the grant`, () => {
    expect(() => assertWorkspaceVisible(access, `ws-other`)).toThrow(
      /not granted access/
    )
    expect(() => assertWorkspaceFullyGranted(access, WS_B)).toThrow(
      /not granted access/
    )
    expect(() => assertProjectGranted(access, PROJ_B2, WS_B)).toThrow(
      /not granted access/
    )
    expect(() => assertFullAccess(access)).toThrow(/not granted access/)
  })

  it(`pass inside the grant`, () => {
    expect(() => assertWorkspaceVisible(access, WS_B)).not.toThrow()
    expect(() => assertWorkspaceFullyGranted(access, WS_A)).not.toThrow()
    expect(() => assertProjectGranted(access, PROJ_B1, WS_B)).not.toThrow()
    expect(() => assertFullAccess(FULL_ACCESS)).not.toThrow()
  })
})

describe(`filterVisibleWorkspaceIds`, () => {
  it(`intersects with the visible set, passthrough on full`, () => {
    expect(filterVisibleWorkspaceIds(access, [WS_A, WS_B, `ws-x`])).toEqual([
      WS_A,
      WS_B,
    ])
    expect(filterVisibleWorkspaceIds(FULL_ACCESS, [`a`, `b`])).toEqual([
      `a`,
      `b`,
    ])
  })
})
