import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/agent-settings.json"

vi.mock(`@/lib/collections`, () => ({
  issueCollection: { values: () => [] },
  sessionCollection: { get: () => undefined },
  teamCollection: { toArray: [], get: () => undefined },
}))

import { chatTeamIds, issueIdsInTeams, sessionOwnDevices, syncedTeamOrder } from "@/lib/chat-teams"

// #EXP-2079 (agent-settings.json `assistant.chatTeams`): the web port over
// the synced store's rows.

describe(`chat teams (assistant.chatTeams)`, () => {
  it(`orders the synced teams by name (case-insensitive), then id`, () => {
    expect(
      syncedTeamOrder([
        { id: `t-3`, name: `beta` },
        { id: `t-2`, name: `Alpha` },
        { id: `t-1`, name: `alpha` },
        { id: `t-4`, name: `Zed` },
      ])
    ).toEqual([`t-1`, `t-2`, `t-3`, `t-4`])
  })

  for (const c of fixture.assistant.chatTeams.cases) {
    it(c.name, () => {
      // The fixture gives the synced teams in the cross-team order: name them so.
      const teams = c.syncedTeamIds.map((id, index) => ({ id, name: `team ${String(index).padStart(2, `0`)}` }))
      expect(chatTeamIds(c.ownTeamId, c.pickedTeamIds, teams)).toEqual(c.expected)
    })
  }

  for (const c of fixture.assistant.chatTeams.deviceCases) {
    it(`devices: ${c.name}`, () => {
      const session = { teamId: `p-me`, userId: c.starterIsViewer ? `me` : `mate` }
      expect(sessionOwnDevices(session, `me`, c.sessionTeamSynced ? [{ id: `p-me` }] : [{ id: `t-a` }])).toBe(c.expected)
    })
  }

  it(`resolves identifiers over the chat teams, the first team's on a clash, nothing outside them`, () => {
    const issues = [
      { id: `i-b7`, teamId: `t-b`, identifier: `EXP-7` },
      { id: `i-a7`, teamId: `t-a`, identifier: `EXP-7` },
      { id: `i-b9`, teamId: `t-b`, identifier: `web-9` },
      { id: `i-x1`, teamId: `t-x`, identifier: `OPS-1` },
    ]
    expect(issueIdsInTeams([`t-a`, `t-b`], [`#exp-7`, `WEB-9`, `OPS-1`, `NOPE-1`], issues)).toEqual([`i-a7`, `i-b9`])
    expect(issueIdsInTeams([`t-b`, `t-a`], [`EXP-7`], issues)).toEqual([`i-b7`])
    expect(issueIdsInTeams([], [`EXP-7`], issues)).toEqual([])
  })
})
