// EXP-420: the machine-row Update button must render only when a newer CLI
// version actually exists (or an update is already in flight) — not for
// every online server.
import { describe, expect, it } from "vitest"

import {
  deviceAgentLaunchDefaults,
  deviceDefaultAgent,
  deviceIsMine,
  deviceUpdateAvailable,
  showDeviceUpdateButton,
  type SteerDevice,
} from "./steer-devices"
import { agentSeed } from "./coding-launch-prefs"

function server(overrides: Partial<SteerDevice> = {}): SteerDevice {
  return {
    deviceId: `dev-1`,
    deviceLabel: `homelab`,
    kind: `server`,
    online: true,
    registered: true,
    version: `0.14.1`,
    ...overrides,
  }
}

describe(`deviceUpdateAvailable`, () => {
  it(`compares numerically, not lexicographically`, () => {
    expect(deviceUpdateAvailable(`0.9.9`, `0.10.0`)).toBe(true)
    expect(deviceUpdateAvailable(`0.14.1`, `0.14.2`)).toBe(true)
    expect(deviceUpdateAvailable(`0.14.2`, `0.14.2`)).toBe(false)
    expect(deviceUpdateAvailable(`0.15.0`, `0.14.2`)).toBe(false)
  })

  it(`fails closed on unknown or unparsable versions`, () => {
    expect(deviceUpdateAvailable(null, `0.14.2`)).toBe(false)
    expect(deviceUpdateAvailable(`0.14.1`, null)).toBe(false)
    expect(deviceUpdateAvailable(undefined, undefined)).toBe(false)
    expect(deviceUpdateAvailable(`not-a-version`, `0.14.2`)).toBe(false)
  })
})

describe(`showDeviceUpdateButton`, () => {
  it(`shows only for an outdated online registered server`, () => {
    expect(showDeviceUpdateButton(server(), `0.14.2`)).toBe(true)
  })

  it(`hides when the device is current`, () => {
    expect(showDeviceUpdateButton(server({ version: `0.14.2` }), `0.14.2`)).toBe(
      false
    )
  })

  it(`hides when the latest version is unknown (env unset)`, () => {
    expect(showDeviceUpdateButton(server(), null)).toBe(false)
    expect(showDeviceUpdateButton(server(), undefined)).toBe(false)
  })

  it(`hides for desktops, offline and unregistered rows`, () => {
    expect(showDeviceUpdateButton(server({ kind: `desktop` }), `0.14.2`)).toBe(
      false
    )
    expect(showDeviceUpdateButton(server({ online: false }), `0.14.2`)).toBe(
      false
    )
    expect(
      showDeviceUpdateButton(server({ registered: false }), `0.14.2`)
    ).toBe(false)
    expect(
      showDeviceUpdateButton(server({ registered: undefined }), `0.14.2`)
    ).toBe(false)
  })

  it(`keeps an in-flight update visible even once versions look current`, () => {
    // The daemon's re-register clears updateRequested and carries the new
    // version — until then the row must keep saying "Updating…"/"Queued".
    expect(
      showDeviceUpdateButton(
        server({ version: `0.14.2`, updateRequested: true }),
        `0.14.2`
      )
    ).toBe(true)
    expect(
      showDeviceUpdateButton(server({ updateRequested: true }), null)
    ).toBe(true)
  })
})

// EXP-437: the Start-coding dialog seeds from the selected device's
// advertised per-agent launch defaults.
describe(`device launch defaults`, () => {
  const advertising = server({
    agents: [`claude`, `codex`],
    launchDefaults: {
      defaultAgent: `claude`,
      agents: {
        claude: { model: `opus`, effort: ``, planMode: true },
        codex: { model: ``, effort: `high`, skipPermissions: true },
      },
    },
  })

  it(`resolves the default agent only when the device can run it`, () => {
    expect(deviceDefaultAgent(advertising)).toBe(`claude`)
    // Configured default not runnable there → null (caller falls back).
    expect(
      deviceDefaultAgent(
        server({
          agents: [`codex`],
          launchDefaults: { defaultAgent: `pi`, agents: {} },
        })
      )
    ).toBe(null)
    // No advertisement (old desktop) → null.
    expect(deviceDefaultAgent(server())).toBe(null)
    expect(deviceDefaultAgent(undefined)).toBe(null)
  })

  it(`returns the per-agent entry or null`, () => {
    expect(deviceAgentLaunchDefaults(advertising, `codex`)).toEqual({
      model: ``,
      effort: `high`,
      skipPermissions: true,
    })
    expect(deviceAgentLaunchDefaults(advertising, `pi`)).toBe(null)
    expect(deviceAgentLaunchDefaults(server(), `claude`)).toBe(null)
  })

  it(`agentSeed validates against the contract and capability-clamps`, () => {
    // The advertised values ride through; blank effort stays blank.
    expect(agentSeed(`claude`, { model: `opus`, effort: ``, planMode: true })).toEqual(
      { model: `opus`, effort: ``, ultracode: false, planMode: true, skipPermissions: false }
    )
    // Blank model is valid for codex; skip rides through.
    expect(
      agentSeed(`codex`, { model: ``, effort: `high`, skipPermissions: true })
    ).toEqual({
      model: ``,
      effort: `high`,
      ultracode: false,
      planMode: false,
      skipPermissions: true,
    })
    // A foreign/unknown value falls back to the static default; claude never
    // takes a blank model.
    expect(agentSeed(`claude`, { model: ``, effort: `warp9` })).toEqual({
      model: `fable`,
      effort: ``,
      ultracode: false,
      planMode: false,
      skipPermissions: false,
    })
    // Capability masking beats a lying advertisement: pi never skips or
    // ultracodes, but its advertised plan default rides through (EXP-441).
    expect(
      agentSeed(`pi`, { skipPermissions: true, planMode: true, ultracode: true })
    ).toEqual({
      model: ``,
      effort: ``,
      ultracode: false,
      planMode: true,
      skipPermissions: false,
    })
    // Codex never plans.
    expect(agentSeed(`codex`, { planMode: true })).toMatchObject({
      planMode: false,
    })
    // `null` = the static fallback for devices that advertise nothing.
    expect(agentSeed(`claude`, null)).toEqual({
      model: `fable`,
      effort: ``,
      ultracode: false,
      planMode: false,
      skipPermissions: false,
    })
  })
})

// EXP-432: teammates' shared rows carry `owner`; own rows never do.
describe(`deviceIsMine`, () => {
  it(`is true for own rows (owner absent) and false for shared rows`, () => {
    expect(deviceIsMine(server())).toBe(true)
    expect(deviceIsMine(server({ sharedTeamId: `team-1` }))).toBe(true)
    expect(
      deviceIsMine(
        server({
          sharedTeamId: `team-1`,
          owner: { id: `owner-1`, name: `Tessa` },
        })
      )
    ).toBe(false)
  })
})

// EXP-481: synced-row mapping — online-ness from last_seen_at freshness.
import {
  composeDeviceList,
  defaultDeviceId,
  deviceCanResume,
  deviceRowIsOnline,
  resumeWorktree,
  steerDeviceFromRow,
} from "./steer-devices"
import type { Device, SyncedDeviceWorktree } from "@/db/schema"

const NOW = new Date(`2026-08-11T12:00:00Z`)

function deviceRow(overrides: Partial<Device> = {}): Device {
  return {
    id: `row-1`,
    userId: `me`,
    deviceId: `dev-1`,
    label: `buildbox`,
    kind: `server`,
    platform: `linux`,
    version: `0.14.1`,
    updateRequestedAt: null,
    activeSessions: 0,
    agents: [`claude`],
    caps: [`actions`, `resume`, `worktrees`, `launch-defaults`],
    unauthedAgents: [],
    launchDefaults: null,
    launchDefaultsUpdatedAt: null,
    lastSeenAt: NOW,
    sharedTeamId: null,
    isDefault: false,
    createdAt: NOW,
    updatedAt: NOW,
    ...overrides,
  } as Device
}

describe(`deviceRowIsOnline`, () => {
  it(`is online within the 90s window inclusive, offline beyond`, () => {
    expect(deviceRowIsOnline(new Date(NOW.getTime() - 89_000), NOW)).toBe(true)
    expect(deviceRowIsOnline(new Date(NOW.getTime() - 90_000), NOW)).toBe(true)
    expect(deviceRowIsOnline(new Date(NOW.getTime() - 91_000), NOW)).toBe(
      false
    )
  })

  it(`clamps a negative age (server clock ahead) to online`, () => {
    expect(deviceRowIsOnline(new Date(NOW.getTime() + 30_000), NOW)).toBe(true)
  })

  it(`fails closed on an unparseable stamp`, () => {
    expect(deviceRowIsOnline(`not-a-date`, NOW)).toBe(false)
  })
})

describe(`steerDeviceFromRow`, () => {
  it(`stamps online, registered and the update fields`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({
        updateRequestedAt: NOW,
        activeSessions: 2,
      }),
      { now: NOW, currentUserId: `me` }
    )
    expect(mapped).toMatchObject({
      rowId: `row-1`,
      deviceId: `dev-1`,
      deviceLabel: `buildbox`,
      kind: `server`,
      online: true,
      registered: true,
      updateRequested: true,
      updateBlocked: true,
    })
    expect(mapped.owner).toBeUndefined()
    expect(deviceCanResume(mapped)).toBe(true)
  })

  it(`marks a stale row offline and never blocks without an update request`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({
        lastSeenAt: new Date(NOW.getTime() - 10 * 60_000),
        activeSessions: 3,
      }),
      { now: NOW, currentUserId: `me` }
    )
    expect(mapped.online).toBe(false)
    expect(mapped.updateBlocked).toBe(false)
  })

  it(`stamps owner on someone else's row (shared server)`, () => {
    const mapped = steerDeviceFromRow(deviceRow({ userId: `them` }), {
      now: NOW,
      currentUserId: `me`,
      ownerName: `Tessa`,
    })
    expect(mapped.owner).toEqual({ id: `them`, name: `Tessa` })
  })

  // EXP-622: the flag is the ROW OWNER's preference. Reading a teammate's
  // shared server must never prefill the caller's picker with it.
  it(`carries isDefault on an own row and drops it on a teammate's`, () => {
    expect(
      steerDeviceFromRow(deviceRow({ isDefault: true }), {
        now: NOW,
        currentUserId: `me`,
      }).isDefault
    ).toBe(true)
    expect(
      steerDeviceFromRow(deviceRow({ userId: `them`, isDefault: true }), {
        now: NOW,
        currentUserId: `me`,
        ownerName: `Tessa`,
      }).isDefault
    ).toBe(false)
  })
})

describe(`defaultDeviceId`, () => {
  const candidate = (
    deviceId: string,
    isDefault?: boolean
  ): SteerDevice => ({ deviceId, deviceLabel: deviceId, isDefault })

  it(`prefers the flagged candidate over the first one`, () => {
    expect(
      defaultDeviceId([candidate(`a`), candidate(`b`, true), candidate(`c`)])
    ).toBe(`b`)
  })

  it(`is null when nothing in the candidate list is flagged`, () => {
    expect(defaultDeviceId([candidate(`a`), candidate(`b`, false)])).toBe(null)
    expect(defaultDeviceId([])).toBe(null)
  })
})

describe(`composeDeviceList`, () => {
  const users = new Map([
    [`me`, { id: `me`, name: `Me` }],
    [`them`, { id: `them`, name: `Tessa` }],
  ])

  it(`orders own rows online-first, then this team's shared servers`, () => {
    const rows = [
      deviceRow({
        id: `r-old`,
        deviceId: `d-old`,
        lastSeenAt: new Date(NOW.getTime() - 60 * 60_000),
      }),
      deviceRow({
        id: `r-shared`,
        deviceId: `d-shared`,
        userId: `them`,
        sharedTeamId: `team-1`,
      }),
      deviceRow({ id: `r-new`, deviceId: `d-new` }),
    ]
    const list = composeDeviceList(rows, users, NOW, `me`, `team-1`)
    expect(list.map((d) => d.deviceId)).toEqual([`d-new`, `d-old`, `d-shared`])
    expect(list[2]?.owner).toEqual({ id: `them`, name: `Tessa` })
  })

  it(`EXP-623: online rows sort by label so heartbeats can't reorder them`, () => {
    const rows = [
      deviceRow({
        id: `r-z`,
        deviceId: `d-z`,
        label: `Zeta`,
        // Freshest beat — would lead under last-seen ordering.
        lastSeenAt: NOW,
      }),
      deviceRow({
        id: `r-a`,
        deviceId: `d-a`,
        label: `alpha`,
        lastSeenAt: new Date(NOW.getTime() - 80_000),
      }),
    ]
    const list = composeDeviceList(rows, users, NOW, `me`)
    expect(list.map((d) => d.deviceId)).toEqual([`d-a`, `d-z`])
  })

  it(`EXP-623: offline rows sort below online ones, most recently seen first`, () => {
    const rows = [
      deviceRow({
        id: `r-off-old`,
        deviceId: `d-off-old`,
        label: `aaa`,
        lastSeenAt: new Date(NOW.getTime() - 2 * 60 * 60_000),
      }),
      deviceRow({
        id: `r-off-new`,
        deviceId: `d-off-new`,
        label: `zzz`,
        lastSeenAt: new Date(NOW.getTime() - 10 * 60_000),
      }),
      deviceRow({ id: `r-on`, deviceId: `d-on`, label: `mid` }),
    ]
    const list = composeDeviceList(rows, users, NOW, `me`)
    expect(list.map((d) => d.deviceId)).toEqual([
      `d-on`,
      `d-off-new`,
      `d-off-old`,
    ])
  })

  it(`drops other teams' shares, desktop shares, and everything shared without a teamId`, () => {
    const rows = [
      deviceRow({
        id: `r-other`,
        deviceId: `d-other`,
        userId: `them`,
        sharedTeamId: `team-2`,
      }),
      deviceRow({
        id: `r-desktop`,
        deviceId: `d-desktop`,
        userId: `them`,
        kind: `desktop`,
        sharedTeamId: `team-1`,
      }),
    ]
    expect(composeDeviceList(rows, users, NOW, `me`, `team-1`)).toEqual([])
    expect(
      composeDeviceList(
        [
          deviceRow({
            id: `r-shared`,
            deviceId: `d-shared`,
            userId: `them`,
            sharedTeamId: `team-1`,
          }),
        ],
        users,
        NOW,
        `me`
      )
    ).toEqual([])
  })
})

describe(`resumeWorktree`, () => {
  const worktree = (
    overrides: Partial<SyncedDeviceWorktree> = {}
  ): SyncedDeviceWorktree =>
    ({
      id: `wt-1`,
      deviceRowId: `row-1`,
      repoFullName: `acme/api`,
      branch: `exp/EXP-42`,
      issueIdentifier: `EXP-42`,
      agents: [`claude`],
      dirty: `clean`,
      busy: false,
      reportedAt: NOW,
      createdAt: NOW,
      updatedAt: NOW,
      ...overrides,
    }) as SyncedDeviceWorktree

  it(`matches on device row + identifier (case-insensitive) + agent marker`, () => {
    const rows = [worktree()]
    expect(resumeWorktree(rows, `row-1`, `EXP-42`, `claude`)).not.toBeNull()
    expect(resumeWorktree(rows, `row-1`, `exp-42`, `claude`)).not.toBeNull()
    expect(resumeWorktree(rows, `row-1`, `EXP-42`, `codex`)).toBeNull()
    expect(resumeWorktree(rows, `row-2`, `EXP-42`, `claude`)).toBeNull()
    expect(resumeWorktree(rows, undefined, `EXP-42`, `claude`)).toBeNull()
    expect(resumeWorktree(rows, `row-1`, `EXP-43`, `claude`)).toBeNull()
  })

  it(`treats a missing agents marker as any-agent (pre-marker worktree)`, () => {
    const rows = [worktree({ agents: null })]
    expect(resumeWorktree(rows, `row-1`, `EXP-42`, `codex`)).not.toBeNull()
  })

  it(`never matches identifier-less rows (batch/foreign branches)`, () => {
    const rows = [worktree({ issueIdentifier: null })]
    expect(resumeWorktree(rows, `row-1`, `EXP-42`, `claude`)).toBeNull()
  })
})

describe(`deviceCanResume`, () => {
  it(`is strict on the cap`, () => {
    expect(deviceCanResume(server())).toBe(false)
    expect(deviceCanResume(server({ caps: [`resume`] }))).toBe(true)
  })
})
