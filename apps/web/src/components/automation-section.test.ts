import { describe, expect, it } from "vitest"

import {
  accountPinOf,
  automationAccountKey,
  pickedAccountOption,
  seedAccountPin,
} from "@/components/automation-section"
import { ACCOUNT_FIXTURE } from "@/lib/accounts/account-option.test"
import { flattenAccounts } from "@/lib/accounts/account-option"
import type { SteerDevice } from "@/lib/steer-devices"

// EXP-995: the automation editor's ACCOUNT pin — the pure half of the web
// dialog (`seedAccountPin` / `pickedAccountOption`), which the desktop, iOS
// and Android editors mirror rule for rule: a bound machine seeds its LAST
// USED login (which names the agent), a stored profile the machine no longer
// reports reads back as that agent's first login, and every pick stores its
// profile id verbatim, the ambient `system` login included (EXP-1158).

const device: SteerDevice = {
  deviceId: `dev-1`,
  deviceLabel: `Build box`,
  agents: [`claude`, `codex`],
  caps: [`automations`],
  launchDefaults: ACCOUNT_FIXTURE.launchDefaults ?? undefined,
  agentAccounts: ACCOUNT_FIXTURE.agentAccounts ?? undefined,
}

describe(`automation account pin (EXP-995)`, () => {
  it(`keys the pin like every account picker`, () => {
    expect(automationAccountKey({ agent: `claude`, account: `work` })).toBe(`claude:work`)
    expect(automationAccountKey({ agent: `codex`, account: `system` })).toBe(`codex:system`)
  })

  it(`stores a picked option as its agent + profile, system included`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(accountPinOf(options[0]!)).toEqual({ agent: `codex`, account: `main` })
    expect(
      accountPinOf({
        id: `system`,
        agent: `claude`,
        email: `Claude Code`,
        isLastUsed: false,
        health: `unknown`,
      })
    ).toEqual({ agent: `claude`, account: `system` })
  })

  it(`seeds a bound machine's last used login and leaves a reported pin alone`, () => {
    // Nothing pinned yet: the machine's last used login, which names the agent.
    expect(seedAccountPin(device, { agent: ``, account: `` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // A pin the machine reports as-is is never disturbed (a manual pick sticks).
    expect(seedAccountPin(device, { agent: `claude`, account: `home` })).toBeUndefined()
    expect(seedAccountPin(device, { agent: `codex`, account: `main` })).toBeUndefined()
    // No machine bound: nothing to seed from.
    expect(seedAccountPin(undefined, { agent: ``, account: `` })).toBeUndefined()
  })

  // Profile ids are device-LOCAL: after a device switch (or a stale stored
  // id) the pin lands on a login the bound machine REPORTS, so the Account
  // row never shows a fallback it would not save.
  it(`re-seeds a pin the machine does not report to that agent's login there`, () => {
    // Same agent, a profile this machine never had (another machine's id, or
    // a deleted one): that agent's first login here, its last used one.
    expect(seedAccountPin(device, { agent: `claude`, account: `gone` })).toEqual({
      agent: `claude`,
      account: `work`,
    })
    expect(seedAccountPin(device, { agent: `codex`, account: `other-box` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // An unpinned row (stored NULL): that agent's last used login, so the
    // Account row shows what Save stores.
    expect(seedAccountPin(device, { agent: `codex`, account: `` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // An agent this machine does not run at all: the machine's last used login.
    const claudeOnly: SteerDevice = {
      ...device,
      deviceId: `dev-2`,
      agents: [`claude`],
      launchDefaults: { defaultAgent: `claude` },
      agentAccounts: { claude: ACCOUNT_FIXTURE.agentAccounts!.claude! },
    }
    expect(seedAccountPin(claudeOnly, { agent: `codex`, account: `main` })).toEqual({
      agent: `claude`,
      account: `work`,
    })
    // A machine reporting no login offers the ambient row per agent: a
    // profile pin there lands on the agent's ambient login.
    const bare: SteerDevice = { ...claudeOnly, agentAccounts: undefined }
    expect(seedAccountPin(bare, { agent: `claude`, account: `work` })).toEqual({
      agent: `claude`,
      account: `system`,
    })
  })

  it(`reads a stored pin back as its row, else the agent's first login`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(pickedAccountOption(options, { agent: `claude`, account: `home` })?.id).toBe(`home`)
    // A profile the machine no longer reports falls to that agent's first row.
    expect(pickedAccountOption(options, { agent: `claude`, account: `gone` })?.id).toBe(`work`)
    expect(pickedAccountOption(options, { agent: ``, account: `` })).toBeUndefined()
  })
})
