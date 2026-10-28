import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import { agentReuseRefusal } from "./agent-reuse"

const NOW = new Date(`2026-10-28T12:00:00.000Z`)
const hoursAgo = (h: number) => new Date(NOW.getTime() - h * 60 * 60 * 1000)
const root = (over: Partial<Parameters<typeof agentReuseRefusal>[0]> = {}) => ({
  status: `idle`,
  endedAt: null,
  lastTurnAt: hoursAgo(1),
  lastActivityAt: hoursAgo(1),
  updatedAt: hoursAgo(1),
  ...over,
})

describe(`S-3 agent session reuse`, () => {
  const rules = contract.sessionsSend
  it(`reuses a live, recent, small thread`, () => {
    expect(agentReuseRefusal(root(), 0, NOW)).toBeNull()
  })
  it(`reuses a thread ended less than reuseMaxEndedHours ago`, () => {
    expect(agentReuseRefusal(root({ status: `ended`, endedAt: hoursAgo(rules.reuseMaxEndedHours - 1) }), 0, NOW)).toBeNull()
  })
  it(`refuses one ended longer ago`, () => {
    expect(agentReuseRefusal(root({ status: `ended`, endedAt: hoursAgo(rules.reuseMaxEndedHours) }), 0, NOW)).toMatch(/ended/)
  })
  it(`refuses one whose last turn is older than reuseMaxIdleHours`, () => {
    expect(agentReuseRefusal(root({ lastTurnAt: hoursAgo(rules.reuseMaxIdleHours + 1) }), 0, NOW)).toMatch(/last turn/)
  })
  it(`falls back to the last activity without a turn`, () => {
    expect(agentReuseRefusal(root({ lastTurnAt: null, lastActivityAt: hoursAgo(rules.reuseMaxIdleHours + 1) }), 0, NOW)).toMatch(/last turn/)
  })
  it(`refuses a tree with reuseMaxChildren sub-sessions`, () => {
    expect(agentReuseRefusal(root(), rules.reuseMaxChildren - 1, NOW)).toBeNull()
    expect(agentReuseRefusal(root(), rules.reuseMaxChildren, NOW)).toMatch(/sub-sessions/)
  })
})
