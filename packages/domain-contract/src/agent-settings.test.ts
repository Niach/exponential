import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/agent-settings.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import { agentAccountValue, triggerSummary, validateAgentForm } from "./agent-settings"

describe(`agent settings fixtures (EXP-1291)`, () => {
  test(`every trigger event has a summary`, () => {
    for (const event of contract.agentTrigger.eventValues) {
      expect((fixture.copy.events as Record<string, string>)[event]).toBeTruthy()
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
      expect(agentAccountValue(entry.input as never)).toEqual(entry.expected as never)
    })
  }
})
