import { describe, expect, it } from "vitest"

import {
  accountOptionKey,
  carriedAccountOption,
  lastUsedAccountOption,
  flattenAccounts,
  parseAccountOptionKey,
  type AccountSource,
} from "@/lib/accounts/account-option"

// EXP-988 contract tests for `flattenAccounts` (lib/accounts/account-option.ts),
// implemented by EXP-872. The SAME table is mirrored in the Rust, Swift and
// Kotlin copies (see the header of account-option.ts).

/** The fixture ×4: two claude logins (work = active, home = a dead
 * credential), one codex login, and a signed-out codex profile. The last used
 * agent is codex. */
export const ACCOUNT_FIXTURE: AccountSource = {
  launchDefaults: { defaultAgent: `codex` },
  agentAccounts: {
    claude: {
      signedIn: true,
      email: `work@x.test`,
      profiles: [
        {
          id: `work`,
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
        { id: `old`, signedIn: false, email: `old@x.test` },
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

  it(`labels every option by email, never "default"`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options.map((option) => option.email)).toEqual([
      `codex@x.test`,
      `work@x.test`,
      `home@x.test`,
    ])
    for (const option of options) {
      expect(option.email.toLowerCase()).not.toContain(`default`)
    }
  })

  it(`puts the last used login first and marks exactly one option`, () => {
    // launchDefaults.defaultAgent = 'codex' with an active codex login →
    // that login is options[0] and the only isLastUsed: true.
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options[0]).toMatchObject({ agent: `codex`, id: `main`, isLastUsed: true })
    expect(options.filter((option) => option.isLastUsed)).toHaveLength(1)
    expect(lastUsedAccountOption(options)).toBe(options[0])
  })

  it(`falls back to the first contract agent's active login when no last used agent is set`, () => {
    const options = flattenAccounts({ ...ACCOUNT_FIXTURE, launchDefaults: null })
    expect(options[0]).toMatchObject({ agent: `claude`, id: `work`, isLastUsed: true })
    expect(options.filter((option) => option.isLastUsed)).toHaveLength(1)
    // A last used agent with no active login falls back the same way.
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
            { id: `p1`, signedIn: true, plan: `Max`, active: true },
            { id: `p2`, signedIn: true },
          ],
        },
      },
    })
    expect(options.map((option) => option.email)).toEqual([`Max`, `No email`])
  })

  it(`never synthesizes the ambient login for a device that reports no profiles`, () => {
    const options = flattenAccounts({
      agentAccounts: { claude: { signedIn: true, email: `solo@x.test` } },
      agentUsage: {
        claude: { windows: [{ key: `session`, label: `5h`, percent: 20 }] },
      },
    })
    expect(options).toEqual([])
  })

  it(`skips signed-out logins and retired agents`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(options.some((option) => option.id === `old`)).toBe(false)
    expect(options.some((option) => (option.agent as string) === `pi`)).toBe(false)
    expect(flattenAccounts({})).toEqual([])
    expect(lastUsedAccountOption([])).toBeUndefined()
  })

  it(`carries a pick to another device by agent and email`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(
      carriedAccountOption(options, { agent: `claude`, email: ` HOME@x.test` })?.id
    ).toBe(`home`)
    // Same address, other agent: a different login.
    expect(
      carriedAccountOption(options, { agent: `codex`, email: `work@x.test` })
    ).toBeUndefined()
    // No address, nothing to match on.
    expect(
      carriedAccountOption(options, { agent: `claude`, email: `No email` })
    ).toBeUndefined()
    expect(carriedAccountOption(options, undefined)).toBeUndefined()
  })
})
