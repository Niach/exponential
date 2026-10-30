// EXP-1252: moving a session to another device. Gated here: the refusal
// RULES (pure, order included), the rows (THE launch composer's machines,
// each with its refusal), what a pick does, and the COPY, byte-identical ×4:
// this test reads the native sources off disk and asserts every shared
// sentence appears in them as a string literal.
import { describe, expect, it } from "vitest"
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"
import type { SteerDevice } from "@/lib/steer-devices"
import {
  canSteerSession,
  deviceMoveAgent,
  deviceMoveBlockedReason,
  deviceMoveHiddenReason,
  deviceMovePick,
  deviceMoveTargetReason,
  deviceMoveVisible,
  deviceUsableInTeam,
  MOVE_FAILED_TOAST,
  NOTE,
  PICKER_TITLE,
  REASON_AGENT,
  REASON_ENDED,
  REASON_NO_LOGIN,
  REASON_OFFLINE,
  REASON_STEER,
  REASON_VISIBILITY,
  sessionDeviceMoveRows,
} from "@/lib/session-device-move"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)

const ANDROID = `apps/android/app/src/main/java/com/exponential/app/domain/SessionDeviceMove.kt`
const IOS = `apps/ios/ExpCore/Sources/Domain/SessionDeviceMove.swift`
const DESKTOP = `apps/desktop/crates/ui/src/device_move.rs`

const SHARED = [
  PICKER_TITLE,
  NOTE,
  REASON_STEER,
  REASON_ENDED,
  REASON_AGENT,
  REASON_OFFLINE,
  REASON_VISIBILITY,
  REASON_NO_LOGIN,
  MOVE_FAILED_TOAST,
]

/** Kotlin and Swift wrap long notes over adjacent literals joined by `+`,
 *  Rust over a `\`-continued line or a literal on the next line: fold both. */
function literals(relative: string): string {
  return readFileSync(join(repoRoot, relative), `utf8`)
    .replace(/\\\s*\n\s*/g, ``)
    .replace(/"\s*\+?\s*"/g, ``)
}

const TEAM = `team-1`

/** A signed-in login per named agent. */
const logins = (...agents: string[]) =>
  Object.fromEntries(
    agents.map((agent) => [
      agent,
      {
        signedIn: true,
        profiles: [
          { id: `p-${agent}`, label: agent, email: `${agent}@acme.test`, signedIn: true, active: true },
        ],
      },
    ])
  ) as unknown as SteerDevice[`agentAccounts`]

const device = (
  overrides: Partial<SteerDevice> & { logins?: string[] } = {}
): SteerDevice => {
  const { logins: agents = [`claude`, `codex`], ...rest } = overrides
  return {
    deviceId: `mac`,
    deviceLabel: `Mac`,
    online: true,
    sharedTeamIds: [],
    agentAccounts: logins(...agents),
    ...rest,
  }
}

const rowSession = (
  overrides: Partial<{ teamId: string; shared: boolean; cli: string | null }> = {}
) => ({ teamId: TEAM, shared: false, cli: `claude`, ...overrides })

const session = (
  overrides: Partial<{ teamId: string; shared: boolean; agent: string }> = {}
) => ({ teamId: TEAM, shared: false, agent: `claude`, ...overrides })

describe(`the pin's visibility and refusals (EXP-1252)`, () => {
  it(`hides for agents`, () => {
    expect(deviceMoveVisible({ agentId: null })).toBe(true)
    expect(deviceMoveHiddenReason({ agentId: `agent-1` })).toBe(REASON_AGENT)
    expect(deviceMoveVisible({ agentId: `agent-1` })).toBe(false)
  })

  it(`refuses in the ×4 order: steer, then ended`, () => {
    expect(deviceMoveBlockedReason({ canSteer: true, ended: false })).toBeNull()
    expect(deviceMoveBlockedReason({ canSteer: false, ended: true })).toBe(REASON_STEER)
    expect(deviceMoveBlockedReason({ canSteer: true, ended: true })).toBe(REASON_ENDED)
  })

  it(`reuses the account switch's steer rule`, () => {
    expect(canSteerSession({ shared: false, userId: `u1` }, `u2`)).toBe(false)
    expect(canSteerSession({ shared: true, userId: `u1` }, `u2`)).toBe(true)
  })
})

describe(`deviceUsableInTeam (#EXP-1440)`, () => {
  it(`a personal machine runs every team; a shared one only the teams it is shared with`, () => {
    expect(deviceUsableInTeam({ sharedTeamIds: [] }, `team-a`)).toBe(true)
    expect(deviceUsableInTeam({ sharedTeamIds: null }, `team-a`)).toBe(true)
    expect(deviceUsableInTeam({}, `team-a`)).toBe(true)
    expect(deviceUsableInTeam({ sharedTeamIds: [`team-a`] }, `team-a`)).toBe(true)
    expect(deviceUsableInTeam({ sharedTeamIds: [`team-b`, `team-a`] }, `team-a`)).toBe(true)
    expect(deviceUsableInTeam({ sharedTeamIds: [`team-b`] }, `team-a`)).toBe(false)
  })
})

describe(`deviceMoveTargetReason`, () => {
  it(`allows an online device with the same visibility and a login`, () => {
    expect(deviceMoveTargetReason(device(), session())).toBeNull()
    expect(
      deviceMoveTargetReason(device({ sharedTeamIds: [TEAM] }), session({ shared: true }))
    ).toBeNull()
  })

  // The ORDER is the contract: an offline device never reads "shared differently".
  it(`refuses in the ×4 order: offline, visibility, no login`, () => {
    expect(
      deviceMoveTargetReason(
        device({
          online: false,
          sharedTeamIds: [TEAM],
          version: null,
          logins: [],
        }),
        session()
      )
    ).toBe(REASON_OFFLINE)
    expect(
      deviceMoveTargetReason(
        device({ sharedTeamIds: [TEAM], version: null, logins: [] }),
        session()
      )
    ).toBe(REASON_VISIBILITY)
    expect(deviceMoveTargetReason(device(), session({ shared: true }))).toBe(
      REASON_VISIBILITY
    )
    expect(
      deviceMoveTargetReason(device({ sharedTeamIds: [`other-team`] }), session())
    ).toBeNull()
    // No version gate: an old or unreported build is never refused for it
    // (the CLIENT_MIN_VERSION floor does that job).
    expect(
      deviceMoveTargetReason(device({ version: `0.14.72` }), session())
    ).toBeNull()
    expect(deviceMoveTargetReason(device({ version: null }), session())).toBeNull()
    expect(deviceMoveTargetReason(device({ logins: [`codex`] }), session())).toBe(
      REASON_NO_LOGIN
    )
    expect(
      deviceMoveTargetReason(device({ logins: [`codex`] }), session({ agent: `codex` }))
    ).toBeNull()
  })

  it(`a session with no recorded agent runs its device's default, else claude`, () => {
    expect(deviceMoveAgent(`codex`, null)).toBe(`codex`)
    expect(
      deviceMoveAgent(null, { launchDefaults: { defaultAgent: `codex` } } as SteerDevice)
    ).toBe(`codex`)
    expect(deviceMoveAgent(null, null)).toBe(`claude`)
  })
})

describe(`sessionDeviceMoveRows`, () => {
  const devices = [
    device({ deviceId: `mac`, deviceLabel: `Mac` }),
    device({ deviceId: `box`, deviceLabel: `Box`, kind: `server` }),
    device({ deviceId: `old`, deviceLabel: `Old`, online: false }),
    device({
      deviceId: `srv`,
      deviceLabel: `Build`,
      sharedTeamIds: [TEAM],
      owner: { id: `u2`, name: `Ada` },
    }),
    device({ deviceId: `stale`, deviceLabel: `Stale`, version: `0.14.72` }),
  ]

  it(`lists every machine, the current one selected and never refused`, () => {
    const rows = sessionDeviceMoveRows(devices, {
      ...rowSession(),
      deviceId: `mac`,
    })
    expect(rows.map((row) => [row.id, row.name, row.current, row.reason])).toEqual([
      [`mac`, `Mac`, true, null],
      [`box`, `Box`, false, null],
      [`old`, `Old`, false, REASON_OFFLINE],
      [`srv`, `Build — Ada`, false, REASON_VISIBILITY],
      [`stale`, `Stale`, false, null],
    ])
  })

  it(`the current device stays listed even when offline or missing`, () => {
    const offline = sessionDeviceMoveRows(devices, {
      ...rowSession(),
      deviceId: `old`,
    })
    expect(offline.find((row) => row.current)?.reason).toBeNull()
    const missing = sessionDeviceMoveRows(devices, { ...rowSession(), deviceId: `gone` })
    expect(missing[0]).toMatchObject({ id: `gone`, current: true, reason: null })
  })

})

describe(`deviceMovePick`, () => {
  it(`another device = move, the current one = nothing`, () => {
    expect(deviceMovePick(`box`, { deviceId: `mac` })).toBe(`move`)
    expect(deviceMovePick(`mac`, { deviceId: `mac` })).toBeNull()
  })
})

describe(`device-move copy`, () => {
  const expectIn = (file: string, values: readonly string[]) => {
    const src = literals(file)
    for (const value of values) {
      expect(src.includes(`"${value}"`) ? value : `${file} is missing: ${value}`).toBe(value)
    }
  }

  it.skipIf(!existsSync(join(repoRoot, ANDROID)))(`is byte-identical in the Android source`, () => {
    expectIn(ANDROID, SHARED)
  })

  it.skipIf(!existsSync(join(repoRoot, DESKTOP)))(`is byte-identical in the desktop source`, () => {
    expectIn(DESKTOP, SHARED)
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
