import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/composer-home.json" with { type: "json" }
import { composerAgentRow, pickForSubject, seedPick, subjectAfterPick, type ComposerPick } from "./composer-agent-row"

describe(`composer-home.json agentRow`, () => {
  for (const c of fixture.agentRow.cases) {
    it(c.name, () => {
      const row = composerAgentRow({ agents: c.agents, me: c.me, picked: c.picked })
      expect({ options: row.options, selected: row.selected, label: row.label }).toEqual(c.expected)
    })
  }
  for (const c of fixture.agentRow.seedCases) {
    it(`seed: ${c.name}`, () => {
      expect(seedPick(c.seed)).toBe(c.expected)
    })
  }
  for (const c of fixture.agentRow.subjectCases) {
    it(`subject: ${c.name}`, () => {
      expect(pickForSubject(c.subject, c.current)).toBe(c.expected)
    })
  }
  for (const c of fixture.agentRow.pickCases) {
    it(`pick: ${c.name}`, () => {
      expect(subjectAfterPick(c.subject, c.pick as ComposerPick)).toBe(c.expected)
    })
  }
})
