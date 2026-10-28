import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/composer-home.json" with { type: "json" }
import { composerAgentRow } from "./composer-agent-row"

describe(`composer-home.json agentRow`, () => {
  for (const c of fixture.agentRow.cases) {
    it(c.name, () => {
      const row = composerAgentRow({ agents: c.agents, me: c.me, picked: c.picked })
      expect({ options: row.options, selected: row.selected, label: row.label }).toEqual(c.expected)
    })
  }
})
