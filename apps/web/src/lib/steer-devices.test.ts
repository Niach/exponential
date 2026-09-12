// EXP-420: the machine-row Update button must render only when a newer CLI
// version actually exists (or an update is already in flight) — not for
// every online server.
import { describe, expect, it } from "vitest"

import {
  describeUpdateBlockers,
  deviceAcpAgentIds,
  deviceAgentLaunchDefaults,
  deviceAgentNotReady,
  deviceCanSwitchAccount,
  deviceCanUpdateNow,
  deviceDefaultAgent,
  deviceIsMine,
  deviceSupportsAcp,
  deviceProfileUsage,
  deviceUpdateAvailable,
  deviceUsageWallAt,
  liveUpdateBlockers,
  showDeviceUpdateButton,
  updateBlockerLabel,
  type SteerDevice,
  type UpdateBlockerSession,
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
        codex: { model: ``, effort: `high` },
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
          launchDefaults: { defaultAgent: `claude`, agents: {} },
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
    })
    // An agent the advertisement has no entry for (EXP-849: `pi` is retired,
    // historical rows still name it).
    expect(deviceAgentLaunchDefaults(advertising, `pi`)).toBe(null)
    expect(deviceAgentLaunchDefaults(server(), `claude`)).toBe(null)
  })

  it(`agentSeed validates against the contract and capability-clamps`, () => {
    // The advertised values ride through; blank effort stays blank.
    expect(agentSeed(`claude`, { model: `opus`, effort: ``, planMode: true })).toEqual(
      { model: `opus`, effort: ``, ultracode: false, planMode: true }
    )
    // Blank model is valid for codex.
    expect(agentSeed(`codex`, { model: ``, effort: `high` })).toEqual({
      model: ``,
      effort: `high`,
      ultracode: false,
      planMode: false,
    })
    // A foreign/unknown value falls back to the static default; claude never
    // takes a blank model.
    expect(agentSeed(`claude`, { model: ``, effort: `warp9` })).toEqual({
      model: `fable`,
      effort: ``,
      ultracode: false,
      planMode: false,
    })
    // Capability masking beats a lying advertisement: claude ultracodes and
    // plans, codex does neither.
    expect(
      agentSeed(`claude`, { model: `fable`, planMode: true, ultracode: true })
    ).toEqual({
      model: `fable`,
      effort: ``,
      ultracode: true,
      planMode: true,
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
    })
  })
})

// EXP-746: the engine capability.
describe(`ACP hosting (EXP-746)`, () => {
  it(`gates ACP hosting on the advertised cap`, () => {
    expect(deviceSupportsAcp({ caps: [`worktrees`, `acp`] })).toBe(true)
    expect(deviceSupportsAcp({ caps: [`worktrees`] })).toBe(false)
    expect(deviceSupportsAcp({})).toBe(false)
  })
})

// EXP-432: teammates' shared rows carry `owner`; own rows never do.
describe(`deviceIsMine`, () => {
  it(`is true for own rows (owner absent) and false for shared rows`, () => {
    expect(deviceIsMine(server())).toBe(true)
    expect(deviceIsMine(server({ sharedTeamIds: [`team-1`] }))).toBe(true)
    expect(
      deviceIsMine(
        server({
          sharedTeamIds: [`team-1`],
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
  deviceAgentIds,
  deviceCanAgentLogin,
  deviceHasRunnableAgent,
  deviceRowIsOnline,
  deviceUnauthedAgentIds,
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
    acpAgents: null,
    launchDefaults: null,
    launchDefaultsUpdatedAt: null,
    lastSeenAt: NOW,
    sharedTeamIds: [],
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
      platform: `linux`,
      online: true,
      registered: true,
      updateRequested: true,
      updateBlocked: true,
    })
    expect(mapped.owner).toBeUndefined()
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

  // FEED-33: the share is the SET, and only the set — a null column reads as
  // private rather than throwing on the pickers' array reads.
  it(`always emits sharedTeamIds as an array`, () => {
    const opts = { now: NOW, currentUserId: `me` }
    expect(
      steerDeviceFromRow(deviceRow({ sharedTeamIds: [`team-1`, `team-2`] }), opts)
        .sharedTeamIds
    ).toEqual([`team-1`, `team-2`])
    expect(
      steerDeviceFromRow(deviceRow({ sharedTeamIds: null as never }), opts)
        .sharedTeamIds
    ).toEqual([])
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
        sharedTeamIds: [`team-1`],
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
        sharedTeamIds: [`team-2`],
      }),
      deviceRow({
        id: `r-desktop`,
        deviceId: `d-desktop`,
        userId: `them`,
        kind: `desktop`,
        sharedTeamIds: [`team-1`],
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
            sharedTeamIds: [`team-1`],
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

// EXP-849: the mid-run account switch is its own capability — an older build
// resumes on the RECORDED account and drops the field, so requesters must be
// able to tell the two apart before offering the switch.
describe(`deviceCanSwitchAccount`, () => {
  it(`is true only when the machine advertises account-switch`, () => {
    expect(deviceCanSwitchAccount({ caps: [`account-switch`] })).toBe(true)
    expect(
      deviceCanSwitchAccount({ caps: [`resume-run`, `agent-login`] })
    ).toBe(false)
    expect(deviceCanSwitchAccount({ caps: [] })).toBe(false)
    expect(deviceCanSwitchAccount({ caps: undefined as never })).toBe(false)
  })
})

// EXP-639: the registered row is the ONE advertisement — an absent `agents`
// list is a row that advertises nothing, never the old claude-only fallback.
describe(`deviceAgentIds`, () => {
  it(`never falls back to claude and drops unknown agent ids`, () => {
    expect(deviceAgentIds(server())).toEqual([])
    expect(deviceAgentIds(undefined)).toEqual([])
    expect(deviceAgentIds(server({ agents: [] }))).toEqual([])
    expect(deviceAgentIds(server({ agents: [`codex`, `nope`] }))).toEqual([
      `codex`,
    ])
  })

  it(`gates runnability on the same list`, () => {
    expect(deviceHasRunnableAgent(server())).toBe(false)
    expect(deviceHasRunnableAgent(server({ agents: [`claude`] }))).toBe(true)
  })
})

// EXP-849 retired `pi`. The server clamps every register, but a row written
// before that clamp — or one served by a self-hosted instance on an older
// image — must still never reach a picker, and the COMPOSED shape is also
// what `exponential_devices_list` serialises.
describe(`retired agent ids (EXP-849)`, () => {
  it(`strips pi from the composed row's three agent lists`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({
        agents: [`claude`, `pi`],
        unauthedAgents: [`pi`],
        acpAgents: [`claude`, `pi`],
      }),
      { now: NOW, currentUserId: `me` }
    )
    expect(mapped.agents).toEqual([`claude`])
    expect(mapped.unauthedAgents).toEqual([])
    expect(mapped.acpAgents).toEqual([`claude`])
    expect(deviceAgentIds(mapped)).toEqual([`claude`])
    expect(deviceUnauthedAgentIds(mapped)).toEqual([])
    expect(deviceAcpAgentIds(mapped)).toEqual([`claude`])
  })

  it(`keeps an unreported acp list unknown, and a pi-only machine runs nothing`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({ agents: [`pi`], acpAgents: null }),
      { now: NOW, currentUserId: `me` }
    )
    expect(mapped.acpAgents).toBeNull()
    expect(deviceAcpAgentIds(mapped)).toBeNull()
    expect(deviceHasRunnableAgent(mapped)).toBe(false)
  })

  it(`never offers pi as the device default agent`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({
        agents: [`claude`, `pi`],
        launchDefaults: { defaultAgent: `pi`, agents: { pi: { model: `` } } },
      }),
      { now: NOW, currentUserId: `me` }
    )
    expect(deviceDefaultAgent(mapped)).toBeNull()
  })
})

// EXP-484: the machine's read-only per-agent status rides the synced row.
describe(`agent status mapping (EXP-484)`, () => {
  const accounts = {
    claude: { signedIn: true, email: `danny@example.com`, plan: `Max` },
  }
  const usage = {
    claude: {
      fetchedAt: `2026-08-11T11:58:00.000Z`,
      stale: false,
      windows: [
        { key: `session`, label: `5h`, percent: 42, resetsAt: null },
      ],
    },
  }

  it(`maps accounts, usage and the usage stamp`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({
        agentAccounts: accounts,
        agentUsage: usage,
        agentUsageAt: new Date(`2026-08-11T11:58:00Z`),
      }),
      { now: NOW, currentUserId: `me` }
    )
    expect(mapped.agentAccounts).toEqual(accounts)
    expect(mapped.agentUsage).toEqual(usage)
    expect(mapped.agentUsageAt).toBe(`2026-08-11T11:58:00.000Z`)
  })

  it(`leaves a machine without a collector undefined, never "signed out"`, () => {
    const mapped = steerDeviceFromRow(deviceRow(), {
      now: NOW,
      currentUserId: `me`,
    })
    expect(mapped.agentAccounts).toBeUndefined()
    expect(mapped.agentUsage).toBeUndefined()
    expect(mapped.agentUsageAt).toBeNull()
  })

  it(`gates remote sign-in on the advertised cap`, () => {
    expect(deviceCanAgentLogin({ caps: [`worktrees`, `agent-login`] })).toBe(
      true
    )
    expect(deviceCanAgentLogin({ caps: [`worktrees`] })).toBe(false)
    expect(deviceCanAgentLogin({})).toBe(false)
  })
})

// EXP-749: a machine reports WHICH of its runnable agents its ACP engine can
// drive. NULL is an older build that never reported — assume every runnable
// agent is ACP-ready (the old behaviour). EXP-773 deleted the PTY fallback, so
// an agent outside a REPORTED set cannot start on that machine at all.
describe(`acp agents (EXP-749)`, () => {
  it(`a row that never reported reads unknown, and blocks nothing`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({ agents: [`claude`, `codex`], acpAgents: null }),
      { now: NOW, currentUserId: `me` }
    )
    expect(deviceAcpAgentIds(mapped)).toBeNull()
    expect(deviceAgentNotReady(mapped, `codex`)).toBe(false)
    expect(deviceAgentNotReady(mapped, `claude`)).toBe(false)
  })

  it(`an agent outside the reported ACP set is not ready there`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({ agents: [`claude`, `codex`], acpAgents: [`claude`] }),
      { now: NOW, currentUserId: `me` }
    )
    expect(deviceAcpAgentIds(mapped)).toEqual([`claude`])
    expect(deviceAgentNotReady(mapped, `codex`)).toBe(true)
    expect(deviceAgentNotReady(mapped, `claude`)).toBe(false)
    // Not runnable there at all is equally unstartable (EXP-849: a retired
    // agent id from a historical row).
    expect(deviceAgentNotReady(mapped, `pi`)).toBe(true)
    // No agent picked yet is never a claim.
    expect(deviceAgentNotReady(mapped, ``)).toBe(false)
  })

  it(`an empty reported set blocks every agent`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({ agents: [`claude`], acpAgents: [] }),
      { now: NOW, currentUserId: `me` }
    )
    expect(deviceAcpAgentIds(mapped)).toEqual([])
    expect(deviceAgentNotReady(mapped, `claude`)).toBe(true)
  })

  it(`filters values outside the contract, like deviceAgentIds`, () => {
    const mapped = steerDeviceFromRow(
      deviceRow({ agents: [`claude`], acpAgents: [`claude`, `bogus`] }),
      { now: NOW, currentUserId: `me` }
    )
    expect(deviceAcpAgentIds(mapped)).toEqual([`claude`])
  })
})

// EXP-804: the START-time half of the usage wall. Getting this wrong in
// either direction is expensive — refusing a healthy machine blocks work,
// and letting a spent one through produces the silent run the whole issue
// exists to prevent — so both directions are pinned here.
describe(`deviceUsageWallAt`, () => {
  const NOW = new Date(`2026-09-09T12:00:00.000Z`)
  const RESET = `2026-09-09T14:30:00.000Z`
  const fresh = `2026-09-09T11:58:00.000Z`

  const usage = (
    windows: { key: string; percent: number; resetsAt?: string | null }[],
    fetchedAt = fresh
  ) => ({
    fetchedAt,
    stale: false,
    windows: windows.map((w) => ({
      key: w.key,
      label: w.key,
      percent: w.percent,
      resetsAt: w.resetsAt ?? null,
    })),
  })

  const device = (over: Partial<SteerDevice> = {}): SteerDevice => ({
    deviceId: `dev-1`,
    deviceLabel: `mint`,
    online: true,
    agentUsage: { claude: usage([{ key: `session`, percent: 100, resetsAt: RESET }]) },
    ...over,
  })

  it(`is the reset of a fresh, fully spent window`, () => {
    expect(deviceUsageWallAt(device(), `claude`, undefined, NOW)).toEqual(
      new Date(RESET)
    )
  })

  it(`binds on the window that resets LAST`, () => {
    // Both spent: the agent cannot work again until the later one resets.
    const later = `2026-09-10T09:00:00.000Z`
    const row = device({
      agentUsage: {
        claude: usage([
          { key: `session`, percent: 100, resetsAt: RESET },
          { key: `weekly`, percent: 100, resetsAt: later },
        ]),
      },
    })
    expect(deviceUsageWallAt(row, `claude`, undefined, NOW)).toEqual(
      new Date(later)
    )
  })

  it(`fails open on a half-spent window`, () => {
    const row = device({
      agentUsage: {
        claude: usage([{ key: `session`, percent: 99, resetsAt: RESET }]),
      },
    })
    expect(deviceUsageWallAt(row, `claude`, undefined, NOW)).toBeNull()
  })

  it(`fails open on STALE numbers`, () => {
    // A machine that stopped reporting is not a machine out of credit.
    const row = device({
      agentUsage: {
        claude: usage(
          [{ key: `session`, percent: 100, resetsAt: RESET }],
          `2026-09-09T11:00:00.000Z`
        ),
      },
    })
    expect(deviceUsageWallAt(row, `claude`, undefined, NOW)).toBeNull()
  })

  it(`fails open with no reset, a past reset, or no report at all`, () => {
    const noReset = device({
      agentUsage: { claude: usage([{ key: `session`, percent: 100 }]) },
    })
    expect(deviceUsageWallAt(noReset, `claude`, undefined, NOW)).toBeNull()
    const past = device({
      agentUsage: {
        claude: usage([
          { key: `session`, percent: 100, resetsAt: `2026-09-09T11:00:00.000Z` },
        ]),
      },
    })
    expect(deviceUsageWallAt(past, `claude`, undefined, NOW)).toBeNull()
    expect(
      deviceUsageWallAt(device({ agentUsage: undefined }), `claude`, undefined, NOW)
    ).toBeNull()
    // A different agent on the same machine is unaffected.
    expect(deviceUsageWallAt(device(), `codex`, undefined, NOW)).toBeNull()
  })
})

describe(`deviceProfileUsage`, () => {
  const NOW = new Date(`2026-09-09T12:00:00.000Z`)
  const window = (percent: number) => ({
    fetchedAt: `2026-09-09T11:58:00.000Z`,
    stale: false,
    windows: [
      {
        key: `session`,
        label: `5h`,
        percent,
        resetsAt: `2026-09-09T14:30:00.000Z`,
      },
    ],
  })

  it(`prefers a profile's OWN numbers over the top-level slot`, () => {
    const row: SteerDevice = {
      deviceId: `dev-1`,
      deviceLabel: `mint`,
      agentUsage: { claude: window(100) },
      agentAccounts: {
        claude: {
          signedIn: true,
          profiles: [
            { id: `work`, signedIn: true, active: true, usage: window(10) },
          ],
        },
      },
    }
    expect(deviceProfileUsage(row, `claude`, undefined)?.windows[0].percent).toBe(10)
    expect(deviceUsageWallAt(row, `claude`, undefined, NOW)).toBeNull()
  })

  it(`falls the ACTIVE profile back to the pre-profile slot`, () => {
    const row: SteerDevice = {
      deviceId: `dev-1`,
      deviceLabel: `mint`,
      agentUsage: { claude: window(100) },
      agentAccounts: {
        claude: { signedIn: true, profiles: [{ id: `work`, signedIn: true, active: true }] },
      },
    }
    expect(deviceProfileUsage(row, `claude`, undefined)?.windows[0].percent).toBe(100)
  })

  it(`treats a profile this device does not have as UNKNOWN, not spent`, () => {
    const row: SteerDevice = {
      deviceId: `dev-1`,
      deviceLabel: `mint`,
      agentUsage: { claude: window(100) },
      agentAccounts: {
        claude: { signedIn: true, profiles: [{ id: `work`, signedIn: true, active: true }] },
      },
    }
    expect(deviceProfileUsage(row, `claude`, `other`)).toBeNull()
    expect(deviceUsageWallAt(row, `claude`, `other`, NOW)).toBeNull()
  })
})

// FEED-36: a queued CLI update parked behind live sessions names what holds
// it, and "Update now" is offered only to daemons that run `update_now`.
describe(`update blockers (FEED-36)`, () => {
  const NOW = new Date(`2026-09-09T16:30:00Z`)
  const session = (
    over: Partial<UpdateBlockerSession> = {}
  ): UpdateBlockerSession => ({
    issueIdentifier: null,
    actionName: null,
    userId: `u-danny`,
    startedAt: `2026-09-09T13:12:00Z`,
    updatedAt: `2026-09-09T16:29:00Z`,
    ...over,
  })
  const users = new Map([
    [`u-danny`, { name: `Danny Straehhuber`, email: `danny@example.com` }],
    [`u-lisa`, { name: `Lisa`, email: `lisa@example.com` }],
  ])
  // The stamp formats in the LOCAL zone; derive the expected text the same
  // way so the lock holds on any CI machine.
  const local = (iso: string) => {
    const d = new Date(iso)
    const day = d.getDate()
    const month = [`Jan`, `Feb`, `Mar`, `Apr`, `May`, `Jun`, `Jul`, `Aug`, `Sep`, `Oct`, `Nov`, `Dec`][d.getMonth()]
    const hh = String(d.getHours()).padStart(2, `0`)
    const mm = String(d.getMinutes()).padStart(2, `0`)
    return `${day} ${month} ${hh}:${mm}`
  }

  it(`labels a row by issue, then action, then Chat`, () => {
    expect(
      updateBlockerLabel({ issueIdentifier: `EXP-12`, actionName: `Nightly` })
    ).toBe(`EXP-12`)
    expect(
      updateBlockerLabel({ issueIdentifier: null, actionName: `Nightly` })
    ).toBe(`Nightly`)
    expect(updateBlockerLabel({ issueIdentifier: null, actionName: null })).toBe(
      `Chat`
    )
  })

  it(`drops heartbeat-dead rows and orders oldest first`, () => {
    const live = liveUpdateBlockers(
      [
        session({ startedAt: `2026-09-09T15:00:00Z` }),
        session({ startedAt: `2026-09-09T09:00:00Z`, updatedAt: `2026-09-09T10:00:00Z` }),
        session({ startedAt: `2026-09-09T14:00:00Z` }),
      ],
      NOW
    )
    expect(live.map((s) => s.startedAt)).toEqual([
      `2026-09-09T14:00:00Z`,
      `2026-09-09T15:00:00Z`,
    ])
  })

  it(`names every live session with its first name and start time`, () => {
    const text = describeUpdateBlockers(
      [
        session(),
        session({
          issueIdentifier: `EXP-12`,
          userId: `u-lisa`,
          startedAt: `2026-09-09T14:02:00Z`,
        }),
      ],
      users,
      NOW
    )
    expect(text).toBe(
      `Update queued behind 2 live sessions: Chat · Danny · started ${local(`2026-09-09T13:12:00Z`)}; EXP-12 · Lisa · started ${local(`2026-09-09T14:02:00Z`)}`
    )
  })

  it(`uses the singular for one session and a member fallback for an unsynced user`, () => {
    const text = describeUpdateBlockers(
      [session({ actionName: `Nightly build`, userId: `u-ghost-ABCD` })],
      users,
      NOW
    )
    expect(text).toBe(
      `Update queued behind 1 live session: Nightly build · Member · started ${local(`2026-09-09T13:12:00Z`)}`
    )
  })

  it(`still says the update waits when no live row is visible`, () => {
    expect(describeUpdateBlockers([], users, NOW)).toBe(
      `Update queued behind a live session on this machine.`
    )
    expect(
      describeUpdateBlockers(
        [session({ updatedAt: `2026-09-09T10:00:00Z` })],
        users,
        NOW
      )
    ).toBe(`Update queued behind a live session on this machine.`)
  })

  it(`offers Update now only behind the update-now cap`, () => {
    expect(deviceCanUpdateNow(server({ caps: [`update-now`] }))).toBe(true)
    expect(deviceCanUpdateNow(server({ caps: [`agent-login`] }))).toBe(false)
    expect(deviceCanUpdateNow(server())).toBe(false)
  })
})
