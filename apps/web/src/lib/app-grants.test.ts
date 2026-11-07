import { describe, expect, it } from "vitest"
import { actionGrantable } from "@exp/domain-contract/app-actions"
import { GRANTABLE_ACTIONS, grantableTools } from "@/lib/app-grants"

// FEED-159: a grant stored before its action turned owner-only (or
// destructive, or left the catalog) reads as no grant.
describe(`grantableTools`, () => {
  it(`keeps grantable names in order and drops owner-only, destructive and unknown ones`, () => {
    expect(
      grantableTools([
        `exponential_issues_get`,
        `exponential_teams_update`,
        `exponential_issues_delete`,
        `exponential_nope_op`,
        `exponential_sessions_send`,
      ])
    ).toEqual([`exponential_issues_get`, `exponential_sessions_send`])
  })

  it(`reads a missing list as none`, () => {
    expect(grantableTools(null)).toEqual([])
    expect(grantableTools(undefined)).toEqual([])
  })

  it(`GRANTABLE_ACTIONS is exactly the grantable catalog`, () => {
    expect(GRANTABLE_ACTIONS.length).toBeGreaterThan(0)
    expect(GRANTABLE_ACTIONS.every((name) => actionGrantable(name))).toBe(true)
    expect(GRANTABLE_ACTIONS).not.toContain(`exponential_teams_update`)
  })
})
