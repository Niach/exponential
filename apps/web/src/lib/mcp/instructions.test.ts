import { describe, expect, it } from "vitest"
import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"
import { mcpServerInstructions } from "./instructions"

// EXP-1280: the ask sentence and the plan-mode gate name tools a host that
// cannot feed a card's answer back never registers (`cards === false`): an
// agent told to ask with a tool it does not have would search for it, one
// that has it on such a host would wait forever.

const base = {
  inSession: true,
  unattended: false,
  agent: false,
  reportBug: false,
}
const ask = `Ask a person with exponential_session_self {op: "ask"}.`

describe(`mcpServerInstructions — the cards gate`, () => {
  it(`says the ask (and, planning, the gate) by default and when the host feeds cards`, () => {
    for (const cards of [undefined, true]) {
      const plain = mcpServerInstructions({ ...base, cards })
      expect(plain).toContain(ask)
      expect(plain).not.toContain(PLAN_CARD_COPY.planModeGate)
      const planning = mcpServerInstructions({ ...base, cards, planMode: true })
      expect(planning).toContain(`${PLAN_CARD_COPY.planModeGate} ${ask}`)
    }
  })

  it(`says neither on a host that cannot deliver a card's answer`, () => {
    for (const planMode of [false, true]) {
      const text = mcpServerInstructions({ ...base, cards: false, planMode })
      expect(text).not.toContain(`exponential_session_self {op: "ask"}`)
      expect(text).not.toContain(PLAN_CARD_COPY.planModeGate)
      // Everything else stays.
      expect(text).toContain(`exponential_sessions {op: "end"} ends this session`)
      expect(text).toContain(`exponential_guide_write`)
    }
  })

  it(`outside a session there is nothing to gate`, () => {
    const text = mcpServerInstructions({
      ...base,
      inSession: false,
      cards: false,
    })
    expect(text).not.toContain(`exponential_session_self {op: "ask"}`)
    expect(text).toBe(mcpServerInstructions({ ...base, inSession: false }))
  })
})
