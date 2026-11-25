import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/issue-draft.json"
import { issueDraftBoards, issueDraftPick } from "./issue-draft"

describe(`issue-draft (#EXP-2125)`, () => {
  for (const c of fixture.boards) {
    test(`boards: ${c.case}`, () => {
      const result = issueDraftBoards(c)
      expect({ boards: result.boards.map((board) => board.id), showTeam: result.showTeam }).toEqual(c.expected)
    })
  }
  for (const c of fixture.picks) {
    test(`pick: ${c.case}`, () => expect(issueDraftPick(c.draft, c.board, c.soleMemberId)).toEqual(c.expected))
  }
})
