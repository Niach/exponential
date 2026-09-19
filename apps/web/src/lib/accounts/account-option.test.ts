import { describe, it } from "vitest"

// EXP-988 contract tests for `flattenAccounts` (lib/accounts/account-option.ts).
// Skipped until EXP-872 lands the implementation; that node un-skips them and
// mirrors the same table in its Rust, Swift and Kotlin copies.
describe.skip(`flattenAccounts (EXP-872)`, () => {
  it(`yields one option per signed-in login across both agents`, () => {})

  it(`labels every option by email, never by profile name and never "default"`, () => {
    // A profile { id: 'work', label: 'Work laptop', email: 'a@x.test' }
    // yields email 'a@x.test'; no option's email is 'Work laptop' or contains
    // the word 'default'.
  })

  it(`puts the device default first and marks exactly one option`, () => {
    // launchDefaults.defaultAgent = 'codex' with an active codex login →
    // that login is options[0] and the only isDeviceDefault: true.
  })

  it(`falls back to the first contract agent's active login when no default agent is set`, () => {})

  it(`carries the agent on the option so a pick implies it`, () => {})

  it(`derives limits as 0..1 fractions from the session, weekly and first model windows`, () => {
    // windows [{key:'session', percent: 40}, {key:'weekly', percent: 85},
    // {key:'model:opus', label:'Opus', percent: 10}] →
    // limits { fiveHour: 0.4, week: 0.85, model: { label: 'Opus', used: 0.1 } }.
  })

  it(`omits limits for a login with no usage report`, () => {})

  it(`shows the plan for a login the device reports without an address`, () => {})

  it(`yields the ambient system login for a device that reports no profiles`, () => {})

  it(`skips signed-out logins and retired agents`, () => {})
})
