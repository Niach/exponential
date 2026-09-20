import { describe, expect, it } from "vitest"

import {
  accountOptionKey,
  defaultAccountOption,
  flattenAccounts,
  parseAccountOptionKey,
  type AccountSource,
} from "@/lib/accounts/account-option"

// EXP-988 contract tests for `flattenAccounts` (lib/accounts/account-option.ts),
// implemented by EXP-872. The SAME table is mirrored in the Rust, Swift and
// Kotlin copies (see the header of account-option.ts).

/** The fixture ×4: two claude logins (work = active, home = a dead
 * credential), one codex login, and a `system` codex profile that is signed
 * out. The default agent is codex. */
export const ACCOUNT_FIXTURE: AccountSource = {
  launchDefaults: { defaultAgent: `codex` },
  agentAccounts: {
    claude: {
      signedIn: true,
      email: `work@x.test`,
      profiles: [
        {
          id: `work`,
          label: `Work laptop`,
          signedIn: true,
          email: `work@x.test`,
          active: true,
          health: `ok`,
          usage: {
            fetchedAt: `2026-09-19T10:00:00Z`,
            windows: [
              { key: `session`, label: `5h`, percent: 40 },
              { key: `weekly`, label: `Week`, percent: 85 },
              { key: `model:opus`, label: `Opus`, percent: 10 },
            ],
          },
        },
        {
          id: `home`,
          label: `Default`,
          signedIn: true,
          email: `home@x.test`,
          health: `needs_relogin`,
        },
      ],
    },
    codex: {
      signedIn: true,
      profiles: [
        {
          id: `main`,
          label: `Main`,
          signedIn: true,
          email: `codex@x.test`,
          active: true,
          usage: {
            windows: [
              { key: `session`, label: `5h`, percent: 5 },
              { key: `weekly`, label: `Week`, percent: 50 },
            ],
          },
        },
        { id: `system`, signedIn: false, active: false },
      ],
    },
    // A retired agent still sitting in a synced row (EXP-849).
    pi: { signedIn: true, email: `pi@x.test` },
  },
}

describe(`flattenAccounts (EXP-872)`, () => {
  it(`yields one option per signed-in login across both agents`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options.map((option) => `${option.agent}:${option.id}`)).toEqual([
      `codex:main`,
      `claude:work`,
      `claude:home`,
    ])
  })

  it(`labels every option by email, never by profile name and never "default"`, () => {
    // A profile { id: 'work', label: 'Work laptop', email: 'a@x.test' }
    // yields email 'a@x.test'; no option's email is 'Work laptop' or contains
    // the word 'default'.
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options.map((option) => option.email)).toEqual([
      `codex@x.test`,
      `work@x.test`,
      `home@x.test`,
    ])
    for (const option of options) {
      expect(option.email).not.toBe(`Work laptop`)
      expect(option.email.toLowerCase()).not.toContain(`default`)
    }
  })

  it(`puts the device default first and marks exactly one option`, () => {
    // launchDefaults.defaultAgent = 'codex' with an active codex login →
    // that login is options[0] and the only isDeviceDefault: true.
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options[0]).toMatchObject({ agent: `codex`, id: `main`, isDeviceDefault: true })
    expect(options.filter((option) => option.isDeviceDefault)).toHaveLength(1)
    expect(defaultAccountOption(options)).toBe(options[0])
  })

  it(`prefers the stored default account of the default agent`, () => {
    // EXP-872: "default agent" became "default account" — the device stores
    // the profile id, and it wins over the agent's ACTIVE login.
    const options = flattenAccounts({
      ...ACCOUNT_FIXTURE,
      launchDefaults: { defaultAgent: `claude`, defaultAccount: `home` },
    })
    expect(options[0]).toMatchObject({ agent: `claude`, id: `home`, isDeviceDefault: true })
    expect(options.filter((option) => option.isDeviceDefault)).toHaveLength(1)
    // A stored profile the device no longer reports falls back to the
    // active login.
    const gone = flattenAccounts({
      ...ACCOUNT_FIXTURE,
      launchDefaults: { defaultAgent: `claude`, defaultAccount: `retired` },
    })
    expect(gone[0]).toMatchObject({ agent: `claude`, id: `work` })
  })

  it(`falls back to the first contract agent's active login when no default agent is set`, () => {
    const options = flattenAccounts({ ...ACCOUNT_FIXTURE, launchDefaults: null })
    expect(options[0]).toMatchObject({ agent: `claude`, id: `work`, isDeviceDefault: true })
    expect(options.filter((option) => option.isDeviceDefault)).toHaveLength(1)
    // A default agent with no active login falls back the same way.
    const stale = flattenAccounts({
      ...ACCOUNT_FIXTURE,
      launchDefaults: { defaultAgent: `pi` },
    })
    expect(stale[0]).toMatchObject({ agent: `claude`, id: `work` })
  })

  it(`carries the agent on the option so a pick implies it`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options.find((option) => option.id === `main`)?.agent).toBe(`codex`)
    expect(options.find((option) => option.id === `home`)?.agent).toBe(`claude`)
    expect(options.find((option) => option.id === `home`)?.health).toBe(`needs_relogin`)
    expect(parseAccountOptionKey(accountOptionKey(options[0]!))).toEqual({
      agent: `codex`,
      id: `main`,
    })
    expect(parseAccountOptionKey(`nope`)).toBeNull()
  })

  it(`derives limits as 0..1 fractions from the session, weekly and first model windows`, () => {
    // windows [{key:'session', percent: 40}, {key:'weekly', percent: 85},
    // {key:'model:opus', label:'Opus', percent: 10}] →
    // limits { fiveHour: 0.4, week: 0.85, model: { label: 'Opus', used: 0.1 } }.
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options.find((option) => option.id === `work`)?.limits).toEqual({
      fiveHour: 0.4,
      week: 0.85,
      model: { label: `Opus`, used: 0.1 },
    })
    // Codex reports no per-model window: no `model` key at all.
    expect(options.find((option) => option.id === `main`)?.limits).toEqual({
      fiveHour: 0.05,
      week: 0.5,
    })
  })

  it(`omits limits for a login with no usage report`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options.find((option) => option.id === `home`)?.limits).toBeUndefined()
  })

  it(`shows the plan for a login the device reports without an address`, () => {
    const options = flattenAccounts({
      agentAccounts: {
        claude: {
          signedIn: true,
          profiles: [
            { id: `p1`, label: `Plan only`, signedIn: true, plan: `Max`, active: true },
            { id: `p2`, label: `Bare`, signedIn: true },
          ],
        },
      },
    })
    expect(options.map((option) => option.email)).toEqual([`Max`, `p2`])
  })

  it(`yields the ambient system login for a device that reports no profiles`, () => {
    const options = flattenAccounts({
      agentAccounts: { claude: { signedIn: true, email: `solo@x.test` } },
      agentUsage: {
        claude: { windows: [{ key: `session`, label: `5h`, percent: 20 }] },
      },
    })
    expect(options).toEqual([
      {
        id: `system`,
        agent: `claude`,
        email: `solo@x.test`,
        isDeviceDefault: true,
        health: `ok`,
        limits: { fiveHour: 0.2, week: 0 },
      },
    ])
  })

  it(`skips signed-out logins and retired agents`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options.some((option) => option.id === `system`)).toBe(false)
    expect(options.some((option) => (option.agent as string) === `pi`)).toBe(false)
    expect(flattenAccounts({})).toEqual([])
    expect(defaultAccountOption([])).toBeUndefined()
  })
})
