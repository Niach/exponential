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
// and Android editors mirror rule for rule: a bound machine seeds its DEFAULT
// account (which names the agent), a stored profile the machine no longer
// reports reads back as that agent's first login, and the ambient `system`
// login stores as a blank (NULL on the row).

const device: SteerDevice = {
  deviceId: `dev-1`,
  deviceLabel: `Build box`,
  agents: [`claude`, `codex`],
  caps: [`automations`],
  launchDefaults: ACCOUNT_FIXTURE.launchDefaults ?? undefined,
  agentAccounts: ACCOUNT_FIXTURE.agentAccounts ?? undefined,
}

describe(`automation account pin (EXP-995)`, () => {
  it(`keys the pin like every account picker, the ambient login as system`, () => {
    expect(automationAccountKey({ agent: `claude`, account: `work` })).toBe(`claude:work`)
    expect(automationAccountKey({ agent: `codex`, account: `` })).toBe(`codex:system`)
  })

  it(`stores a picked option as its agent + profile, system as a blank`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(accountPinOf(options[0]!)).toEqual({ agent: `codex`, account: `main` })
    expect(
      accountPinOf({
        id: `system`,
        agent: `claude`,
        email: `Claude Code`,
        isDeviceDefault: false,
        health: `unknown`,
      })
    ).toEqual({ agent: `claude`, account: `` })
  })

  it(`seeds a bound machine's default account and leaves a reported pin alone`, () => {
    // Nothing pinned yet: the machine's default login, which names the agent.
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
    // a deleted one): that agent's first login here, the device default when
    // that is the agent.
    expect(seedAccountPin(device, { agent: `claude`, account: `gone` })).toEqual({
      agent: `claude`,
      account: `work`,
    })
    expect(seedAccountPin(device, { agent: `codex`, account: `other-box` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // The ambient login of an agent this machine runs through PROFILES.
    expect(seedAccountPin(device, { agent: `codex`, account: `` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // An agent this machine does not run at all: the machine's default.
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
      account: ``,
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
