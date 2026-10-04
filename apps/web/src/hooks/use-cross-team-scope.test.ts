import { describe, expect, it, vi } from "vitest"
import type { Team } from "@/db/schema"

vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: null }) }))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamMemberships: () => ({ myTeams: [], memberships: [] }),
}))

const { crossTeamScope, compareTeams } = await import(
  `@/hooks/use-cross-team-scope`
)

const team = (id: string, name: string) => ({ id, name, slug: id }) as Team

// EXP-1186: only the PHONE reads every member team; grouping only with >1.
describe(`crossTeamScope`, () => {
  const acme = team(`t-a`, `Acme`)
  const beta = team(`t-b`, `Beta`)

  it(`keeps the active team alone on md+`, () => {
    expect(crossTeamScope(beta, [acme, beta], false)).toEqual({
      teams: [beta],
      teamIds: [`t-b`],
      grouped: false,
    })
  })

  it(`reads every member team on a phone, ordered by name, grouped`, () => {
    const scope = crossTeamScope(beta, [beta, acme], true)
    expect(scope.teams.map((t) => t.id)).toEqual([`t-a`, `t-b`])
    expect(scope.teamIds).toEqual([`t-a`, `t-b`])
    expect(scope.grouped).toBe(true)
  })

  it(`never groups a single-team phone`, () => {
    expect(crossTeamScope(acme, [acme], true).grouped).toBe(false)
  })

  it(`falls back to the active team until the memberships land`, () => {
    expect(crossTeamScope(acme, [], true)).toEqual({
      teams: [acme],
      teamIds: [`t-a`],
      grouped: false,
    })
  })

  it(`breaks a name tie by id`, () => {
    expect(compareTeams(team(`b`, `Same`), team(`a`, `Same`))).toBeGreaterThan(0)
  })
})
