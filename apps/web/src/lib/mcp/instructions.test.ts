import { describe, expect, it } from "vitest"
import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"
import { mcpServerInstructions } from "./instructions"

// EXP-1280: every in-session caller is told how to ask (a planning one gets
// the plan-mode gate first); outside a session there is no one to ask.

const base = {
  inSession: true,
  unattended: false,
  agent: false,
  reportBug: false,
}
const ask = `Ask a person with exponential_session_self {op: "ask"}.`

describe(`mcpServerInstructions — the ask sentence`, () => {
  it(`says the ask in a session, and the gate while planning`, () => {
    const plain = mcpServerInstructions(base)
    expect(plain).toContain(ask)
    expect(plain).not.toContain(PLAN_CARD_COPY.planModeGate)
    const planning = mcpServerInstructions({ ...base, planMode: true })
    expect(planning).toContain(`${PLAN_CARD_COPY.planModeGate} ${ask}`)
  })

  it(`says neither outside a session`, () => {
    const text = mcpServerInstructions({ ...base, inSession: false, planMode: true })
    expect(text).not.toContain(`exponential_session_self {op: "ask"}`)
    expect(text).not.toContain(PLAN_CARD_COPY.planModeGate)
  })
})
