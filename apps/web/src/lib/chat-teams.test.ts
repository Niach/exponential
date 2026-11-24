import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/agent-settings.json"

vi.mock(`@/lib/collections`, () => ({
  issueCollection: { values: () => [] },
  sessionCollection: { get: () => undefined },
  teamCollection: { toArray: [], get: () => undefined },
}))

import { chatTeamIds, issueIdsInTeams, sessionOwnDevices, syncedIssueTeamId, syncedTeamOrder } from "@/lib/chat-teams"

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
    // Real-shaped: the issues shape carries NO team_id; a row's team is its
    // board's (#EXP-2082: rows with a teamId hid that every ref was dropped).
    const boards = new Map([
      [`b-a`, `t-a`],
      [`b-b`, `t-b`],
      [`b-x`, `t-x`],
    ])
    const teamOfBoard = (boardId: string) => boards.get(boardId)
    const issues = [
      { id: `i-b7`, boardId: `b-b`, identifier: `EXP-7` },
      { id: `i-a7`, boardId: `b-a`, identifier: `EXP-7` },
      { id: `i-b9`, boardId: `b-b`, identifier: `web-9` },
      { id: `i-x1`, boardId: `b-x`, identifier: `OPS-1` },
      { id: `i-gone`, boardId: `b-gone`, identifier: `GONE-1` },
    ]
    expect(issues.every((row) => !(`teamId` in row))).toBe(true)
    expect(issueIdsInTeams([`t-a`, `t-b`], [`#exp-7`, `WEB-9`, `OPS-1`, `NOPE-1`, `GONE-1`], issues, teamOfBoard)).toEqual([
      `i-a7`,
      `i-b9`,
    ])
    expect(issueIdsInTeams([`t-b`, `t-a`], [`EXP-7`], issues, teamOfBoard)).toEqual([`i-b7`])
    expect(issueIdsInTeams([], [`EXP-7`], issues, teamOfBoard)).toEqual([])
    expect(syncedIssueTeamId({ boardId: `b-b` }, teamOfBoard)).toBe(`t-b`)
    expect(syncedIssueTeamId({ boardId: null }, teamOfBoard)).toBeUndefined()
  })
})
