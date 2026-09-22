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

  it(`seeds a bound machine's default account and leaves a runnable pin alone`, () => {
    // Nothing pinned yet: the machine's default login, which names the agent.
    expect(seedAccountPin(device, { agent: ``, account: `` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // A pin the machine still runs is never disturbed (a manual pick sticks).
    expect(seedAccountPin(device, { agent: `claude`, account: `home` })).toBeUndefined()
    // No machine bound: nothing to seed from.
    expect(seedAccountPin(undefined, { agent: ``, account: `` })).toBeUndefined()
  })

  it(`reads a stored pin back as its row, else the agent's first login`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(pickedAccountOption(options, { agent: `claude`, account: `home` })?.id).toBe(`home`)
    // A profile the machine no longer reports falls to that agent's first row.
    expect(pickedAccountOption(options, { agent: `claude`, account: `gone` })?.id).toBe(`work`)
    expect(pickedAccountOption(options, { agent: ``, account: `` })).toBeUndefined()
  })
})
