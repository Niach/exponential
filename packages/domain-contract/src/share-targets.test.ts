import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/share-targets.json" with { type: "json" }
import { directShareEntries, parseDirectShareId, shareChatText, shareIssueBoards, shareTargets } from "./share-targets"

describe(`shareTargets`, () => {
  for (const entry of fixture.targetCases) {
    test(entry.name, () => {
      const out = shareTargets(entry.input as Parameters<typeof shareTargets>[0])
      expect({ rows: out.rows, agents: out.agents.map((agent) => agent.id) }).toEqual(entry.expected)
    })
  }
})

describe(`shareChatText`, () => {
  for (const entry of fixture.textCases) {
    test(entry.name, () => expect(shareChatText(entry.input)).toBe(entry.expected))
  }
})

describe(`directShareEntries`, () => {
  for (const entry of fixture.directShareCases) {
    test(entry.name, () => {
      expect(directShareEntries(entry.input as Parameters<typeof directShareEntries>[0])).toEqual(entry.expected)
    })
  }
})

describe(`parseDirectShareId`, () => {
  for (const entry of fixture.parseCases) {
    test(entry.name, () => expect(parseDirectShareId(entry.input)).toEqual(entry.expected as never))
  }
})

describe(`shareIssueBoards`, () => {
  for (const entry of fixture.issueBoardCases) {
    test(entry.name, () => {
      const result = shareIssueBoards(entry.input)
      expect({ boards: result.boards.map((board) => board.id), board: result.board?.id ?? null }).toEqual(entry.expected)
    })
  }
})
