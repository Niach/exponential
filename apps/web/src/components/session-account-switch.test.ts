// EXP-849 phase 3: the mid-session account switch. Two things are gated here —
// the refusal RULE (pure, order included) and the COPY, which is byte-identical
// on all four clients: this test reads the native sources off disk and asserts
// every shared sentence appears in them as a string literal, the way
// `onboarding-copy.test.ts` gates the wizard's.
import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  ACCOUNTS_SECTION_TITLE,
  CONTINUATION_COST_NOTE,
  CONTINUATION_NOTE,
  REASON_AGENT,
  REASON_ALREADY,
  REASON_BUSY,
  REASON_ENDED,
  REASON_NEEDS_RELOGIN,
  REASON_NO_CAP,
  REASON_NOT_MINE,
  REASON_OFFLINE,
  REASON_SIGNED_OUT,
  SWITCH_COST_NOTE,
  SWITCH_LABEL,
  switchBlockedReason,
  SWITCHABLE_AGENT,
  WALL_SWITCH_LABEL,
} from "@/components/session-account-switch"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)

const ANDROID = `apps/android/app/src/main/java/com/exponential/app/domain/SessionAccountSwitch.kt`
const IOS = `apps/ios/ExpCore/Sources/Domain/SessionAccountSwitch.swift`
const DESKTOP = `apps/desktop/crates/ui/src/account_switch.rs`

const SHARED = [
  ACCOUNTS_SECTION_TITLE,
  SWITCH_LABEL,
  WALL_SWITCH_LABEL,
  SWITCH_COST_NOTE,
  CONTINUATION_NOTE,
  CONTINUATION_COST_NOTE,
  REASON_AGENT,
  REASON_NOT_MINE,
  REASON_ENDED,
  REASON_OFFLINE,
  REASON_NO_CAP,
  REASON_BUSY,
  REASON_SIGNED_OUT,
  REASON_NEEDS_RELOGIN,
  REASON_ALREADY,
]

/** The three native sources each wrap the long notes: Kotlin and Swift over two
 * adjacent literals joined by `+`, Rust over a `\`-continued line. Fold both
 * back together before looking for the sentence. */
function literals(relative: string): string {
  return readFileSync(join(repoRoot, relative), `utf8`)
    .replace(/\\\s*\n\s*/g, ``)
    .replace(/"\s*\+?\s*"/g, ``)
}

// The row context a switchable option sits in: own live claude run, online
// machine that can resume, idle turn, healthy signed-in login.
const switchable = {
  agent: SWITCHABLE_AGENT,
  mine: true,
  sessionEnded: false,
  deviceOnline: true,
  canResume: true,
  turnEnded: true,
  option: { profileId: `work`, signedIn: true, health: `ok` },
} as const

describe(`switchBlockedReason (EXP-849)`, () => {
  it(`allows a healthy other login on an own, idle, live claude run`, () => {
    expect(switchBlockedReason({ ...switchable })).toBeNull()
    // `unknown` health is a login nobody has probed — not a refusal.
    expect(
      switchBlockedReason({
        ...switchable,
        option: { ...switchable.option, health: `unknown` },
      })
    ).toBeNull()
  })

  // The ORDER is the contract: the most fundamental refusal wins, so a codex
  // run never reads "the machine is offline" (and a stranger's run never leaks
  // which accounts the machine holds).
  it(`refuses in the ×4 order`, () => {
    expect(
      switchBlockedReason({
        ...switchable,
        agent: `codex`,
        mine: false,
        deviceOnline: false,
      })
    ).toBe(REASON_AGENT)
    expect(
      switchBlockedReason({
        ...switchable,
        mine: false,
        sessionEnded: true,
        deviceOnline: false,
      })
    ).toBe(REASON_NOT_MINE)
    expect(
      switchBlockedReason({
        ...switchable,
        sessionEnded: true,
        deviceOnline: false,
      })
    ).toBe(REASON_ENDED)
    expect(
      switchBlockedReason({
        ...switchable,
        deviceOnline: false,
        canResume: false,
      })
    ).toBe(REASON_OFFLINE)
    expect(
      switchBlockedReason({ ...switchable, canResume: false, turnEnded: false })
    ).toBe(REASON_NO_CAP)
    expect(
      switchBlockedReason({
        ...switchable,
        turnEnded: false,
        option: { ...switchable.option, signedIn: false },
      })
    ).toBe(REASON_BUSY)
  })

  it(`separates a dead credential from a missing one`, () => {
    expect(
      switchBlockedReason({
        ...switchable,
        // The CLI still claims signed in; the probe came back Unauthorized.
        option: { ...switchable.option, health: `needs_relogin` },
      })
    ).toBe(REASON_NEEDS_RELOGIN)
    expect(
      switchBlockedReason({
        ...switchable,
        option: { ...switchable.option, signedIn: false },
      })
    ).toBe(REASON_SIGNED_OUT)
    expect(
      switchBlockedReason({
        ...switchable,
        option: { ...switchable.option, signedIn: true, health: `signed_out` },
      })
    ).toBe(REASON_SIGNED_OUT)
  })

  // `coding_sessions.agent_account` is server-only, so the run's account is
  // usually UNKNOWN: only a known match refuses, and the machine's own active
  // login is never a stand-in.
  it(`only refuses the run's own account when the client knows it`, () => {
    expect(
      switchBlockedReason({ ...switchable, currentAccount: `work` })
    ).toBe(REASON_ALREADY)
    expect(
      switchBlockedReason({ ...switchable, currentAccount: `system` })
    ).toBeNull()
    expect(switchBlockedReason({ ...switchable, currentAccount: null })).toBeNull()
  })

  // A run with no recorded agent (pre-EXP-484 rows) cannot switch: the device
  // would have nothing to relaunch under.
  it(`refuses a run with no recorded agent`, () => {
    expect(switchBlockedReason({ ...switchable, agent: null })).toBe(REASON_AGENT)
  })
})

describe(`account-switch copy`, () => {
  it(`is byte-identical in the Android and iOS sources`, () => {
    for (const file of [ANDROID, IOS]) {
      const src = literals(file)
      for (const value of SHARED) {
        expect(
          src.includes(`"${value}"`) ? value : `${file} is missing: ${value}`
        ).toBe(value)
      }
    }
  })

  // The desktop only ever shows the OWNER their own LIVE runs, so three of the
  // refusals have no surface there (not mine, ended, already on it) — every
  // other sentence is the same byte string.
  it(`is byte-identical in the desktop source`, () => {
    const src = literals(DESKTOP)
    const absent = [REASON_NOT_MINE, REASON_ENDED, REASON_ALREADY]
    for (const value of SHARED.filter((value) => !absent.includes(value))) {
      expect(
        src.includes(`"${value}"`) ? value : `${DESKTOP} is missing: ${value}`
      ).toBe(value)
    }
  })

  it(`keeps every sentence quotable in Swift, Kotlin and Rust source`, () => {
    for (const value of SHARED) {
      expect(value).not.toContain(`"`)
      expect(value).not.toContain(`\\`)
      expect(value.trim()).toBe(value)
    }
  })
})
