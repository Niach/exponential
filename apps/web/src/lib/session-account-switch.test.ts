// EXP-849: the mid-session account switch. Gated here: the refusal RULES
// (pure, order included), the option list (THE composer list narrowed to the
// session's agent) and the COPY, byte-identical ×4: this test reads the
// native sources off disk and asserts every shared sentence appears in them
// as a string literal.
import { describe, expect, it } from "vitest"
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"
import type { AccountSource } from "@/lib/accounts/account-option"
import {
  canSteerSession,
  REASON_AGENT,
  REASON_ALREADY,
  REASON_BUSY,
  REASON_NEEDS_RELOGIN,
  REASON_OFFLINE,
  REASON_SIGNED_OUT,
  sessionAccountSwitchOptions,
  sessionSwitchAgent,
  SWITCH_COST_NOTE,
  SWITCH_LABEL,
  switchBlockedReason,
  switchOptionBlockedReason,
  SWITCHABLE_AGENT,
  WALL_SWITCH_LABEL,
} from "@/lib/session-account-switch"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)

const ANDROID = `apps/android/app/src/main/java/com/exponential/app/domain/SessionAccountSwitch.kt`
const IOS = `apps/ios/ExpCore/Sources/Domain/SessionAccountSwitch.swift`
const DESKTOP = `apps/desktop/crates/ui/src/account_switch.rs`

const SHARED = [
  SWITCH_LABEL,
  WALL_SWITCH_LABEL,
  SWITCH_COST_NOTE,
  REASON_AGENT,
  REASON_OFFLINE,
  REASON_BUSY,
  REASON_SIGNED_OUT,
  REASON_NEEDS_RELOGIN,
  REASON_ALREADY,
]

/** Kotlin and Swift wrap long notes over adjacent literals joined by `+`,
 *  Rust over a `\`-continued line or a literal on the next line: fold both. */
function literals(relative: string): string {
  return readFileSync(join(repoRoot, relative), `utf8`)
    .replace(/\\\s*\n\s*/g, ``)
    .replace(/"\s*\+?\s*"/g, ``)
}

const device = (overrides: Partial<AccountSource> = {}): AccountSource => ({
  agentAccounts: {
    claude: {
      signedIn: true,
      profiles: [
        { id: `home`, label: `home`, email: `home@acme.test`, signedIn: true, active: true },
        { id: `work`, label: `work`, email: `work@acme.test`, signedIn: true, active: false },
        { id: `old`, label: `old`, email: `old@acme.test`, signedIn: false, active: false },
      ],
    },
    codex: {
      signedIn: true,
      profiles: [
        { id: `cx`, label: `cx`, email: `cx@acme.test`, signedIn: true, active: true },
      ],
    },
  } as unknown as AccountSource[`agentAccounts`],
  launchDefaults: { defaultAgent: `claude` } as AccountSource[`launchDefaults`],
  ...overrides,
})

describe(`switchBlockedReason (EXP-849)`, () => {
  it(`allows an online, idle claude session`, () => {
    expect(
      switchBlockedReason({ agent: SWITCHABLE_AGENT, deviceOnline: true, busy: false })
    ).toBeNull()
  })

  // The ORDER is the contract: a codex session never reads "offline".
  it(`refuses in the ×4 order`, () => {
    expect(switchBlockedReason({ agent: `codex`, deviceOnline: false, busy: true })).toBe(
      REASON_AGENT
    )
    expect(switchBlockedReason({ agent: null, deviceOnline: true, busy: false })).toBe(
      REASON_AGENT
    )
    expect(switchBlockedReason({ agent: `claude`, deviceOnline: false, busy: true })).toBe(
      REASON_OFFLINE
    )
    expect(switchBlockedReason({ agent: `claude`, deviceOnline: true, busy: true })).toBe(
      REASON_BUSY
    )
  })

  it(`refuses a dead, a missing or the current login`, () => {
    expect(switchOptionBlockedReason({ id: `a`, health: `needs_relogin` }, `a`)).toBe(
      REASON_NEEDS_RELOGIN
    )
    expect(switchOptionBlockedReason({ id: `a`, health: `signed_out` }, null)).toBe(
      REASON_SIGNED_OUT
    )
    expect(switchOptionBlockedReason({ id: `a`, health: `ok` }, `a`)).toBe(REASON_ALREADY)
    expect(switchOptionBlockedReason({ id: `a`, health: `unknown` }, `b`)).toBeNull()
    expect(switchOptionBlockedReason({ id: `a`, health: `ok` }, null)).toBeNull()
  })
})

describe(`sessionAccountSwitchOptions`, () => {
  it(`lists the device's signed-in logins for the session's agent`, () => {
    const options = sessionAccountSwitchOptions(device(), { cli: `claude`, account: `home` })
    expect(options.map((option) => option.id)).toEqual([`home`, `work`])
    expect(options.map((option) => option.current)).toEqual([true, false])
    expect(options.map((option) => option.blockedReason)).toEqual([REASON_ALREADY, null])
  })

  it(`falls back to the device's default agent when the row names none`, () => {
    expect(sessionSwitchAgent(null, device())).toBe(`claude`)
    expect(
      sessionAccountSwitchOptions(device(), { cli: null, account: null }).map((o) => o.id)
    ).toEqual([`home`, `work`])
    expect(
      sessionAccountSwitchOptions(
        device({ launchDefaults: null }),
        { cli: null, account: null }
      )
    ).toEqual([])
  })

  it(`an unknown current account marks nothing`, () => {
    const options = sessionAccountSwitchOptions(device(), { cli: `claude`, account: null })
    expect(options.every((option) => !option.current && option.blockedReason === null)).toBe(
      true
    )
  })

  it(`no device, no options`, () => {
    expect(sessionAccountSwitchOptions(null, { cli: `claude`, account: null })).toEqual([])
  })
})

describe(`canSteerSession`, () => {
  it(`a shared session = every member, else the starter`, () => {
    expect(canSteerSession({ shared: true, userId: `u1` }, `u2`)).toBe(true)
    expect(canSteerSession({ shared: false, userId: `u1` }, `u1`)).toBe(true)
    expect(canSteerSession({ shared: false, userId: `u1` }, `u2`)).toBe(false)
    expect(canSteerSession({ shared: false, userId: `u1` }, null)).toBe(false)
  })
})

describe(`account-switch copy`, () => {
  const expectIn = (file: string, values: readonly string[]) => {
    const src = literals(file)
    for (const value of values) {
      expect(src.includes(`"${value}"`) ? value : `${file} is missing: ${value}`).toBe(value)
    }
  }

  it(`is byte-identical in the Android source`, () => {
    expectIn(ANDROID, SHARED)
  })

  // The desktop never refuses "already on it": the current login is not a row.
  it(`is byte-identical in the desktop source`, () => {
    expectIn(
      DESKTOP,
      SHARED.filter((value) => value !== REASON_ALREADY)
    )
  })

  it.skipIf(!existsSync(join(repoRoot, IOS)))(`is byte-identical in the iOS source`, () => {
    expectIn(IOS, SHARED)
  })

  it(`keeps every sentence quotable in Swift, Kotlin and Rust source`, () => {
    for (const value of SHARED) {
      expect(value).not.toContain(`"`)
      expect(value).not.toContain(`\\`)
      expect(value.trim()).toBe(value)
    }
  })
})
