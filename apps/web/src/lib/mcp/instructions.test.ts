import { describe, expect, it } from "vitest"
import { mcpServerInstructions, SHOW_SENTENCE } from "./instructions"

// S-3: every in-session caller is told to show (one exponential_show card
// per turn: results, ask, plan); plan mode and its gate are gone; outside a
// session there is no one to show to.

const base = {
  inSession: true,
  unattended: false,
  agent: false,
  reportBug: false,
}

describe(`mcpServerInstructions — the show sentence`, () => {
  it(`says show in a session, never plan mode`, () => {
    const plain = mcpServerInstructions(base)
    expect(plain).toContain(SHOW_SENTENCE)
    expect(plain).not.toContain(`plan mode`)
    expect(plain).not.toContain(`exponential_session_self {op: "ask"}`)
  })

  it(`says nothing of it outside a session, where the verb is spelled out`, () => {
    const text = mcpServerInstructions({ ...base, inSession: false })
    expect(text).not.toContain(`exponential_show`)
    expect(text).toContain(`exponential_sessions_send is the one session verb`)
  })

  it(`keeps a launched session's variant short (the runbook says the rest)`, () => {
    expect(mcpServerInstructions(base)).not.toContain(`exponential_sessions_send is the one session verb`)
  })
})
