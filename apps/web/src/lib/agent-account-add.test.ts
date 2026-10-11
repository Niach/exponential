import { describe, expect, it } from "vitest"
import type { Device } from "@/db/schema"
import { addableAgents, agentInstalledOn } from "@/lib/agent-account-add"

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
