import { describe, expect, it } from "vitest"

import { deviceAccountOptions } from "@/lib/devices/account-options"
import { SYSTEM_PROFILE_ID } from "@/lib/agent-usage"

// EXP-1020: the device settings' "Default account" row. The rule is shared
// with iOS (`accountOptions`) and Android (`deviceAccountOptions`).

const claudeLogin = {
  agentAccounts: {
    claude: {
      signedIn: true,
      profiles: [{ id: `work`, email: `dev@acme.test`, active: true, signedIn: true }],
    },
  },
}

describe(`deviceAccountOptions`, () => {
  it(`adds an ambient option per agent the machine reports NO login for`, () => {
    const options = deviceAccountOptions(claudeLogin, [`claude`, `codex`])
    expect(options.map((option) => option.agent)).toEqual([`claude`, `codex`])
    // The reported claude login rides as itself; codex gets the sentinel.
    expect(options.find((option) => option.agent === `claude`)?.id).toBe(`work`)
    expect(options.find((option) => option.agent === `codex`)?.id).toBe(SYSTEM_PROFILE_ID)
  })

  it(`never duplicates an agent that already reports a login`, () => {
    const options = deviceAccountOptions(claudeLogin, [`claude`])
    expect(options).toHaveLength(1)
    expect(options[0]!.id).toBe(`work`)
  })

  it(`offers every editable agent for a machine that reports nothing`, () => {
    const options = deviceAccountOptions(null, [`claude`, `codex`])
    expect(options.map((option) => option.agent)).toEqual([`claude`, `codex`])
    expect(options.every((option) => option.id === SYSTEM_PROFILE_ID)).toBe(true)
  })

  it(`is what makes the row a CHOICE — a one-login machine still offers two`, () => {
    // The bug this replaced: the ambient fallback only fired when the
    // machine reported no login AT ALL, so a claude-only machine offered a
    // single row, which the picker renders as a plain label.
    expect(deviceAccountOptions(claudeLogin, [`claude`, `codex`]).length).toBeGreaterThan(1)
  })
})
