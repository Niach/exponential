import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/agent-settings.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import { agentAccountValue, triggerSummary, validateAgentForm } from "./agent-settings"

describe(`agent settings fixtures (EXP-1291)`, () => {
  test(`every builtin event name has a summary`, () => {
    for (const name of contract.events.builtinNames) {
      expect((fixture.copy.events as Record<string, string>)[name]).toBeTruthy()
    }
  })
  for (const entry of fixture.summaryCases) {
    test(`summary: ${entry.expected}`, () => {
      expect(triggerSummary(entry.trigger as never)).toBe(entry.expected)
    })
  }
  for (const entry of fixture.validationCases) {
    test(`validation: ${entry.name}`, () => {
      expect(validateAgentForm(entry.form as never)).toEqual(entry.expected)
    })
  }
  for (const entry of fixture.accountValueCases) {
    test(`account value: ${entry.name}`, () => {
      const picked = agentAccountValue(entry.input as never) as { agent: string; email: string } | null
      expect(picked ? { agent: picked.agent, email: picked.email } : null).toEqual(entry.expected as never)
    })
  }
})
