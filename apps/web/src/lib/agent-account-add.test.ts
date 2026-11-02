import { describe, expect, it } from "vitest"
import type { Device } from "@/db/schema"
import {
  addAccountTarget,
  addableAgents,
  agentInstalledOn,
  agentLoginName,
} from "@/lib/agent-account-add"

// EXP-827: the Add-account rules — which agents a machine offers (the login
// itself lands by email on the machine, so there is no target to pick). EXP-909 dropped the
// cross-device half (`addAccountDevices` / `addAccountBlockReason`): the flow
// is opened from a row under ONE machine now, so there is no machine to pick
// and no reason to explain.

const now = new Date(`2026-09-11T10:00:00Z`)

function device(overrides: Partial<Device>): Device {
  return {
    id: `row-${overrides.deviceId ?? `x`}`,
    deviceId: `dev-1`,
    userId: `me`,
    label: `Studio`,
    kind: `desktop`,
    agents: [`claude`],
    unauthedAgents: [],
    caps: [`agent-login`],
    lastSeenAt: now,
    agentAccounts: {},
    ...overrides,
  } as unknown as Device
}

describe(`agentInstalledOn / addableAgents`, () => {
  it(`counts runnable and signed-out agents, deduped`, () => {
    const row = device({ agents: [`claude`, `codex`], unauthedAgents: [`codex`] })
    expect(agentInstalledOn(row, `claude`)).toBe(true)
    expect(agentInstalledOn(row, `codex`)).toBe(true)
    expect(agentInstalledOn(row, `nope`)).toBe(false)
    // EXP-849: every installed agent is addable — each one has a device-code
    // sign-in now.
    expect(addableAgents(row)).toEqual([`claude`, `codex`])
  })
})

// EXP-1626: a long-lived Add names the profile it makes (a setup token cannot
// name its account); nothing else carries a name.
describe(`agentLoginName`, () => {
  it(`keeps a long-lived add's trimmed name, capped at 64`, () => {
    expect(agentLoginName({ longLived: true, name: `  Work ` })).toBe(`Work`)
    expect(agentLoginName({ longLived: true, name: `x`.repeat(80) })).toHaveLength(64)
  })

  it(`drops it for a blank name, a re-login or the browser login`, () => {
    expect(agentLoginName({ longLived: true, name: `   ` })).toBeUndefined()
    expect(agentLoginName({ longLived: true, profileId: `p`, name: `Work` })).toBeUndefined()
    expect(agentLoginName({ longLived: false, name: `Work` })).toBeUndefined()
    expect(agentLoginName({ name: `Work` })).toBeUndefined()
  })
})

describe(`addAccountTarget`, () => {
  it(`carries claude's Name field, blank sends none`, () => {
    expect(addAccountTarget(`claude`, ` Work `)).toEqual({ agent: `claude`, name: `Work` })
    expect(addAccountTarget(`claude`, `  `)).toEqual({ agent: `claude` })
    expect(addAccountTarget(`codex`, `Work`)).toEqual({ agent: `codex` })
  })
})
