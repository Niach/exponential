import { describe, expect, it, test } from "bun:test"

import fixture from "../fixtures/agent-settings.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }
import {
  assistantModelValue,
  agentAccountValue,
  ASSISTANT_SCHEDULES,
  assigneeOptions,
  assistantScheduleOf,
  assistantScheduleTriggers,
  issueActor,
  orderedAgents,
  triggerSummary,
  validateAgentForm,
  visibleAgent,
  type AgentRoleLike,
  type AgentTriggerLike,
} from "./agent-settings"

describe(`agent settings fixtures (EXP-1291)`, () => {
  test(`every builtin event name has a summary`, () => {
    for (const name of contract.events.builtinNames) {
      expect((fixture.copy.events as Record<string, string>)[name]).toBeTruthy()
    }
  })
  for (const entry of fixture.actor.actorCases) {
    test(`actor: ${entry.name}`, () => {
      expect(issueActor(entry.input, entry.agents)).toEqual(entry.expected as never)
    })
  }
  for (const entry of fixture.assignee.optionCases) {
    test(`assignee options: ${entry.name}`, () => {
      expect(assigneeOptions(entry.input)).toEqual(entry.expected)
    })
  }
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

describe(`agent-settings.json assistant`, () => {
  for (const c of fixture.assistant.scheduleCases) {
    it(`scheduleCases: ${c.name}`, () => {
      expect(assistantScheduleOf(c.triggers as AgentTriggerLike[])).toBe(c.expected as never)
    })
  }
  for (const c of fixture.assistant.modelValueCases) {
    it(`modelValueCases: ${c.name}`, () => {
      expect(assistantModelValue(c.input)).toBe(c.expected)
    })
  }
  for (const c of fixture.assistant.visibleCases) {
    it(`visibleCases: ${c.name}`, () => {
      expect(visibleAgent(c.agent as AgentRoleLike, c.me)).toBe(c.expected)
    })
  }
  for (const c of fixture.assistant.orderCases) {
    it(`orderCases: ${c.name}`, () => {
      expect(orderedAgents(c.agents, c.me).map((agent) => agent.id)).toEqual(c.expected)
    })
  }
  it(`every preset round-trips`, () => {
    for (const schedule of ASSISTANT_SCHEDULES) {
      expect(assistantScheduleOf(assistantScheduleTriggers(schedule.id))).toBe(schedule.id)
    }
  })
})
