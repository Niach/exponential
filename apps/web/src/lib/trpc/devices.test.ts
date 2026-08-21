import { describe, expect, it, vi, beforeEach } from "vitest"

const h = vi.hoisted(() => {
  const state = {
    selectRows: [] as unknown[],
    // EXP-432: queue for tests exercising MULTIPLE selects in one call
    // (own rows, then the team-shared join, or setShared's row probe).
    // Empty = every select resolves `selectRows` (the legacy single-select
    // behavior the older tests rely on).
    selectQueue: [] as unknown[][],
    inserted: [] as unknown[],
    upserts: [] as unknown[],
    // EXP-481: rows returned by insert(...).returning() /
    // onConflictDoUpdate(...).returning() — queue like updateReturning.
    insertReturning: [] as unknown[][],
    updates: [] as { set: unknown; returningRows: unknown[] }[],
    updateReturning: [[{ id: `row-1` }]] as unknown[][],
    deletes: 0,
  }
  const nextSelect = () =>
    state.selectQueue.length > 0 ? state.selectQueue.shift() : state.selectRows
  const selectBuilder = () => {
    // Lazily consuming thenable so `.orderBy(...)` can BOTH terminate a
    // chain (await) and continue into `.limit(...)` without double-spending
    // the select queue (EXP-481: the heartbeat command pickup chains both).
    const lazy = () => {
      let cached: unknown
      let resolved = false
      const get = () => {
        if (!resolved) {
          resolved = true
          cached = nextSelect()
        }
        return cached
      }
      return {
        then: (
          onFulfilled?: (value: unknown) => unknown,
          onRejected?: (reason: unknown) => unknown
        ) => Promise.resolve(get()).then(onFulfilled, onRejected),
        limit: () => Promise.resolve(get()),
      }
    }
    const terminal = {
      orderBy: () => lazy(),
      limit: () => Promise.resolve(nextSelect()),
    }
    const b = {
      from: () => b,
      innerJoin: () => b,
      where: () => terminal,
    }
    return b
  }
  const insertResult = () =>
    Promise.resolve(
      state.insertReturning.shift() ?? [
        { id: `row-1`, launchDefaults: null, launchDefaultsUpdatedAt: null },
      ]
    )
  const db = {
    select: () => selectBuilder(),
    insert: () => ({
      values: (values: unknown) => {
        state.inserted.push(values)
        return {
          onConflictDoUpdate: (upsert: unknown) => {
            state.upserts.push(upsert)
            return Object.assign(Promise.resolve(), {
              returning: insertResult,
            })
          },
          returning: insertResult,
        }
      },
    }),
    update: () => ({
      set: (set: unknown) => ({
        where: () => {
          const returningRows = state.updateReturning.shift() ?? []
          state.updates.push({ set, returningRows })
          return Object.assign(Promise.resolve(), {
            returning: () => Promise.resolve(returningRows),
          })
        },
      }),
    }),
    delete: () => ({
      where: () => {
        state.deletes += 1
        return Promise.resolve()
      },
    }),
    // EXP-481: txid-wrapped mutations run their body against the same mock;
    // generateTxId calls tx.execute for pg_current_xact_id.
    transaction: async (fn: (tx: unknown) => Promise<unknown>) =>
      fn(
        Object.assign(Object.create(db), {
          execute: async () => ({ rows: [{ txid: `42` }] }),
        })
      ),
  }
  return {
    state,
    db,
    getSteerRelayConfig: vi.fn(),
    relayGetDevices: vi.fn(),
    relayPostNudge: vi.fn(async () => ({ delivered: true })),
    assertTeamMember: vi.fn(),
    getTeamMember: vi.fn(async () => ({ role: `member` }) as unknown),
    endForeignHostedSessions: vi.fn(async () => [] as string[]),
  }
})

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayGetDevices: h.relayGetDevices,
  relayPostNudge: h.relayPostNudge,
}))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  getTeamMember: h.getTeamMember,
}))
vi.mock(`@/lib/coding-session-kill`, () => ({
  endForeignHostedSessions: h.endForeignHostedSessions,
}))
vi.mock(`@/lib/client-version`, () => ({
  versionPayload: () => ({
    android: { min: null, latest: null },
    ios: { min: null, latest: null },
    desktop: { min: null, latest: `0.9.0` },
    cli: { min: null, latest: `0.9.0` },
  }),
}))

import { devicesRouter } from "@/lib/trpc/devices"

const caller = devicesRouter.createCaller({
  session: { user: { id: `actor`, name: `Actor`, email: `a@example.com` } },
  db: h.db,
  request: new Request(`http://localhost/`),
} as never)

const registryRow = (over: Record<string, unknown> = {}) => ({
  id: `row-1`,
  userId: `actor`,
  deviceId: `dev-1`,
  label: `buildbox`,
  kind: `server`,
  platform: `linux`,
  agents: [`claude`],
  caps: [`actions`],
  version: `0.8.52`,
  updateRequestedAt: null,
  activeSessions: 0,
  lastSeenAt: new Date(`2026-08-01T10:00:00Z`),
  sharedTeamId: null,
  unauthedAgents: [],
  launchDefaults: null,
  launchDefaultsUpdatedAt: null,
  createdAt: new Date(`2026-07-01T10:00:00Z`),
  updatedAt: new Date(`2026-08-01T10:00:00Z`),
  ...over,
})

// What setShared's ownership probe selects (id, kind, shared_team_id).
const sharedProbe = (sharedTeamId: string | null) => [
  [{ id: `row-1`, kind: `server`, sharedTeamId }],
]

beforeEach(() => {
  vi.clearAllMocks()
  h.state.selectRows = []
  h.state.selectQueue = []
  h.state.inserted = []
  h.state.upserts = []
  h.state.insertReturning = []
  h.state.updates = []
  h.state.updateReturning = [[{ id: `row-1` }]]
  h.state.deletes = 0
  h.getSteerRelayConfig.mockReturnValue({
    url: `ws://relay`,
    secret: `s`,
    enabled: true,
  })
  h.relayGetDevices.mockResolvedValue({ devices: [] })
  h.endForeignHostedSessions.mockResolvedValue([])
  h.getTeamMember.mockResolvedValue({ role: `member` })
})

describe(`devices.register`, () => {
  it(`upserts on (user, deviceId) with the advertised agents and caps`, async () => {
    const result = await caller.register({
      deviceId: `dev-1`,
      label: `buildbox`,
      kind: `server`,
      platform: `linux`,
      agents: [`claude`, `codex`],
      caps: [`actions`],
    })
    expect(result).toMatchObject({ ok: true })
    expect(h.state.inserted).toHaveLength(1)
    expect(h.state.inserted[0]).toMatchObject({
      userId: `actor`,
      deviceId: `dev-1`,
      label: `buildbox`,
      kind: `server`,
      agents: [`claude`, `codex`],
    })
    expect(h.state.upserts).toHaveLength(1)
  })

  it(`never overwrites an existing row's label — renames must survive re-register`, async () => {
    await caller.register({
      deviceId: `dev-1`,
      label: `buildbox`,
      kind: `server`,
    })
    const upsert = h.state.upserts[0] as { set: Record<string, unknown> }
    expect(upsert.set.label).toBeUndefined()
    expect(upsert.set).toMatchObject({ kind: `server` })
  })

  it(`carries the version and consumes a pending update request`, async () => {
    await caller.register({
      deviceId: `dev-1`,
      label: `buildbox`,
      kind: `server`,
      version: `0.9.0`,
    })
    const upsert = h.state.upserts[0] as { set: Record<string, unknown> }
    expect(upsert.set.version).toBe(`0.9.0`)
    expect(upsert.set.updateRequestedAt).toBeNull()
  })

  it(`accepts 0.14.10's explicit-null launch-default toggles and strips them (EXP-495)`, async () => {
    // The exact defaults_wire shape 0.14.10 clients sent: capability-masked
    // toggles as explicit null instead of absent. This used to 400 the whole
    // register, leaving the machine invisible with no self-heal path.
    const result = await caller.register({
      deviceId: `dev-1`,
      label: `unraid-runner`,
      kind: `server`,
      launchDefaults: {
        defaultAgent: `claude`,
        agents: {
          claude: {
            model: `fable`,
            effort: ``,
            ultracode: false,
            planMode: true,
            skipPermissions: false,
          },
          codex: {
            model: ``,
            effort: ``,
            ultracode: null,
            planMode: null,
            skipPermissions: false,
          },
          pi: {
            model: ``,
            effort: ``,
            ultracode: null,
            planMode: true,
            skipPermissions: null,
          },
        },
      },
    })
    expect(result).toMatchObject({ ok: true })
    const seeded = (h.state.inserted[0] as { launchDefaults: unknown })
      .launchDefaults
    // Stored jsonb stays null-free — native clients parse it off the shape.
    expect(JSON.stringify(seeded)).not.toContain(`null`)
    expect(seeded).toEqual({
      defaultAgent: `claude`,
      agents: {
        claude: {
          model: `fable`,
          effort: ``,
          ultracode: false,
          planMode: true,
          skipPermissions: false,
        },
        codex: { model: ``, effort: ``, skipPermissions: false },
        pi: { model: ``, effort: ``, planMode: true },
      },
    })
  })
})

describe(`devices.requestUpdate + heartbeat`, () => {
  it(`flags the row and the next heartbeat reports it`, async () => {
    h.state.updateReturning = [
      [{ id: `row-1` }],
      [{ id: `row-1`, updateRequestedAt: new Date() }],
    ]
    expect(await caller.requestUpdate({ deviceId: `dev-1` })).toEqual({
      ok: true,
    })
    expect(await caller.heartbeat({ deviceId: `dev-1` })).toMatchObject({
      ok: true,
      updateRequested: true,
    })
  })
})

describe(`devices.heartbeat`, () => {
  it(`reports ok: false when the row was removed, so the daemon re-registers`, async () => {
    h.state.updateReturning = [[]]
    const result = await caller.heartbeat({ deviceId: `dev-gone` })
    expect(result).toEqual({ ok: false, updateRequested: false })
  })

  it(`bumps last_seen_at for a live row`, async () => {
    const result = await caller.heartbeat({ deviceId: `dev-1` })
    expect(result).toMatchObject({ ok: true, updateRequested: false })
    expect(h.state.updates[0]?.set).toMatchObject({
      lastSeenAt: expect.any(Date),
    })
  })

  it(`stores the reported live-session count (EXP-411)`, async () => {
    await caller.heartbeat({ deviceId: `dev-1`, activeSessions: 2 })
    expect(h.state.updates[0]?.set).toMatchObject({ activeSessions: 2 })
  })

  it(`leaves the stored count alone when a pre-EXP-411 daemon omits it`, async () => {
    await caller.heartbeat({ deviceId: `dev-1` })
    const set = h.state.updates[0]?.set as Record<string, unknown>
    expect(`activeSessions` in set).toBe(false)
  })
})

describe(`devices.list`, () => {
  it(`marks a registered row online when the relay reports it connected`, async () => {
    h.state.selectRows = [registryRow()]
    h.relayGetDevices.mockResolvedValue({
      devices: [
        {
          deviceId: `dev-1`,
          deviceLabel: `buildbox`,
          connectedAt: 1,
          agents: [`claude`, `pi`],
          caps: [`actions`, `fix-conflicts`],
        },
      ],
    })
    const { devices } = await caller.list()
    expect(devices).toHaveLength(1)
    expect(devices[0]).toMatchObject({
      deviceId: `dev-1`,
      kind: `server`,
      online: true,
      registered: true,
      // The live advertisement wins over the registered snapshot...
      agents: [`claude`, `pi`],
      caps: [`actions`, `fix-conflicts`],
      // ...but the REGISTRY label is authoritative (renames stay visible).
      deviceLabel: `buildbox`,
    })
  })

  it(`passes signed-out agents through from live presence (EXP-409)`, async () => {
    h.state.selectRows = [registryRow()]
    h.relayGetDevices.mockResolvedValue({
      devices: [
        {
          deviceId: `dev-1`,
          deviceLabel: `buildbox`,
          connectedAt: 1,
          // The daemon's only agent is signed out: runnable list explicitly
          // empty, the unauthed list names it.
          agents: [],
          unauthedAgents: [`claude`],
          caps: [],
        },
      ],
    })
    const { devices } = await caller.list()
    expect(devices[0]).toMatchObject({
      online: true,
      agents: [],
      unauthedAgents: [`claude`],
    })
  })

  it(`passes launch defaults through from live presence; offline rows carry none (EXP-437)`, async () => {
    const launchDefaults = {
      defaultAgent: `claude`,
      agents: { claude: { model: `opus`, effort: ``, planMode: true } },
    }
    h.state.selectRows = [registryRow()]
    h.relayGetDevices.mockResolvedValue({
      devices: [
        {
          deviceId: `dev-1`,
          deviceLabel: `buildbox`,
          connectedAt: 1,
          agents: [`claude`],
          caps: [`actions`],
          launchDefaults,
        },
      ],
    })
    const { devices } = await caller.list()
    expect(devices[0]?.launchDefaults).toEqual(launchDefaults)

    // Offline (relay down): no defaults — you cannot start there anyway.
    h.state.selectRows = [registryRow()]
    h.relayGetDevices.mockRejectedValue(new Error(`relay down`))
    const { devices: offline } = await caller.list()
    expect(offline[0]?.launchDefaults).toBeUndefined()
  })

  it(`defaults unauthedAgents to empty for offline registry rows`, async () => {
    h.state.selectRows = [registryRow()]
    h.relayGetDevices.mockRejectedValue(new Error(`relay down`))
    const { devices } = await caller.list()
    expect(devices[0]?.unauthedAgents).toEqual([])
  })

  it(`shows the renamed registry label even while the relay holds the old one`, async () => {
    h.state.selectRows = [registryRow({ label: `renamed-box` })]
    h.relayGetDevices.mockResolvedValue({
      devices: [
        { deviceId: `dev-1`, deviceLabel: `old-hostname`, connectedAt: 1 },
      ],
    })
    const { devices } = await caller.list()
    expect(devices[0]?.deviceLabel).toBe(`renamed-box`)
  })

  it(`keeps registry rows (offline) when the relay is unreachable`, async () => {
    h.state.selectRows = [registryRow()]
    h.relayGetDevices.mockRejectedValue(new Error(`relay down`))
    const { devices } = await caller.list()
    expect(devices).toHaveLength(1)
    expect(devices[0]).toMatchObject({
      online: false,
      lastSeenAt: `2026-08-01T10:00:00.000Z`,
      agents: [`claude`],
    })
  })

  it(`omits relay-connected devices that never registered (pre-registry builds retired)`, async () => {
    h.state.selectRows = []
    h.relayGetDevices.mockResolvedValue({
      devices: [
        { deviceId: `old-desktop`, deviceLabel: `Old Mac`, connectedAt: 1 },
      ],
    })
    const { devices } = await caller.list()
    expect(devices).toHaveLength(0)
  })

  it(`marks a pending update blocked while the daemon reports live sessions (EXP-411)`, async () => {
    h.state.selectRows = [
      registryRow({
        updateRequestedAt: new Date(`2026-08-03T10:00:00Z`),
        activeSessions: 1,
      }),
    ]
    h.relayGetDevices.mockRejectedValue(new Error(`relay down`))
    const { devices } = await caller.list()
    expect(devices[0]).toMatchObject({
      updateRequested: true,
      updateBlocked: true,
    })
  })

  it(`clears the blocked state once the machine reads idle`, async () => {
    h.state.selectRows = [
      registryRow({
        updateRequestedAt: new Date(`2026-08-03T10:00:00Z`),
        activeSessions: 0,
      }),
    ]
    h.relayGetDevices.mockRejectedValue(new Error(`relay down`))
    const { devices } = await caller.list()
    expect(devices[0]).toMatchObject({
      updateRequested: true,
      updateBlocked: false,
    })
  })

  it(`works with the relay unconfigured — everything reads offline`, async () => {
    h.getSteerRelayConfig.mockReturnValue(null)
    h.state.selectRows = [registryRow()]
    const { devices } = await caller.list()
    expect(devices[0]?.online).toBe(false)
    expect(h.relayGetDevices).not.toHaveBeenCalled()
  })
})

describe(`devices.remove`, () => {
  it(`deletes the row`, async () => {
    const result = await caller.remove({ deviceId: `dev-1` })
    expect(result).toMatchObject({ ok: true })
    expect(h.state.deletes).toBe(1)
  })
})

// EXP-432: sharing a server device with a team.
describe(`devices.setShared`, () => {
  it(`shares an own server device with a team the caller belongs to`, async () => {
    h.state.selectQueue = sharedProbe(null)
    const result = await caller.setShared({
      deviceId: `dev-1`,
      teamId: `11111111-1111-4111-8111-111111111111`,
    })
    expect(result).toMatchObject({ ok: true })
    expect(h.assertTeamMember).toHaveBeenCalledWith(
      `actor`,
      `11111111-1111-4111-8111-111111111111`
    )
    expect(h.state.updates[0]?.set).toMatchObject({
      sharedTeamId: `11111111-1111-4111-8111-111111111111`,
    })
  })

  it(`clears the share with teamId: null without a membership check`, async () => {
    h.state.selectQueue = sharedProbe(null)
    const result = await caller.setShared({ deviceId: `dev-1`, teamId: null })
    expect(result).toMatchObject({ ok: true })
    expect(h.assertTeamMember).not.toHaveBeenCalled()
    expect(h.state.updates[0]?.set).toMatchObject({ sharedTeamId: null })
  })

  it(`rejects desktop devices — only servers are shareable`, async () => {
    h.state.selectQueue = [
      [{ id: `row-1`, kind: `desktop`, sharedTeamId: null }],
    ]
    await expect(
      caller.setShared({
        deviceId: `dev-1`,
        teamId: `11111111-1111-4111-8111-111111111111`,
      })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    expect(h.state.updates).toHaveLength(0)
  })

  it(`rejects a deviceId the caller does not own`, async () => {
    h.state.selectQueue = [[]]
    await expect(
      caller.setShared({
        deviceId: `foreign-dev`,
        teamId: `11111111-1111-4111-8111-111111111111`,
      })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})

// EXP-445: withdrawing a share ends the teammate runs it was the consent for.
describe(`devices.setShared — kill fan-out`, () => {
  const TEAM_A = `11111111-1111-4111-8111-111111111111`
  const TEAM_B = `22222222-2222-4222-8222-222222222222`

  it(`ends the old team's hosted sessions when the share is cleared`, async () => {
    h.state.selectQueue = sharedProbe(TEAM_A)
    // The column write must land FIRST — once shared_team_id has moved, no
    // new foreign attribution can slip in behind the fan-out. Two updates by
    // then: the share column plus the automation disarm that rides the same
    // transaction.
    let updatesWhenKilled = -1
    h.endForeignHostedSessions.mockImplementation(async () => {
      updatesWhenKilled = h.state.updates.length
      return []
    })

    await caller.setShared({ deviceId: `dev-1`, teamId: null })

    expect(h.endForeignHostedSessions).toHaveBeenCalledWith(`actor`, TEAM_A)
    expect(updatesWhenKilled).toBe(2)
  })

  it(`ends the OLD team's sessions when the device moves to another team`, async () => {
    h.state.selectQueue = sharedProbe(TEAM_A)

    await caller.setShared({ deviceId: `dev-1`, teamId: TEAM_B })

    expect(h.state.updates[0]?.set).toMatchObject({ sharedTeamId: TEAM_B })
    expect(h.endForeignHostedSessions).toHaveBeenCalledWith(`actor`, TEAM_A)
  })

  it(`ends nothing on a first share (null → team)`, async () => {
    h.state.selectQueue = sharedProbe(null)

    await caller.setShared({ deviceId: `dev-1`, teamId: TEAM_A })

    expect(h.endForeignHostedSessions).not.toHaveBeenCalled()
  })

  it(`ends nothing on a same-team re-share`, async () => {
    h.state.selectQueue = sharedProbe(TEAM_A)

    await caller.setShared({ deviceId: `dev-1`, teamId: TEAM_A })

    expect(h.endForeignHostedSessions).not.toHaveBeenCalled()
  })
})

// EXP-530 follow-up (EXP-583: automations rows): withdrawing a share must
// also stop the teammate-created automations bound to this device — the
// device self-selects automations off Electric, and the toggle is owner-only.
describe(`devices.setShared — automation disarm`, () => {
  const TEAM_A = `11111111-1111-4111-8111-111111111111`
  const TEAM_B = `22222222-2222-4222-8222-222222222222`

  // The trigger-disabling UPDATE, if it ran (the device row write is first).
  const automationUpdate = () => h.state.updates[1]

  it(`disables the old team's triggers when the share is cleared`, async () => {
    h.state.selectQueue = sharedProbe(TEAM_A)

    await caller.setShared({ deviceId: `dev-1`, teamId: null })

    expect(h.getTeamMember).toHaveBeenCalledWith(`actor`, TEAM_A)
    expect(h.state.updates).toHaveLength(2)
    expect(automationUpdate()!.set).toMatchObject({ enabled: false })
  })

  it(`disables the old team's triggers when the device moves teams`, async () => {
    h.state.selectQueue = sharedProbe(TEAM_A)

    await caller.setShared({ deviceId: `dev-1`, teamId: TEAM_B })

    expect(h.getTeamMember).toHaveBeenCalledWith(`actor`, TEAM_A)
    expect(h.state.updates).toHaveLength(2)
  })

  it(`leaves triggers alone when the device owner OWNS the old team`, async () => {
    h.state.selectQueue = sharedProbe(TEAM_A)
    h.getTeamMember.mockResolvedValue({ role: `owner` })

    await caller.setShared({ deviceId: `dev-1`, teamId: null })

    expect(h.state.updates).toHaveLength(1)
  })

  it(`touches nothing on a first share or a same-team re-share`, async () => {
    h.state.selectQueue = sharedProbe(null)
    await caller.setShared({ deviceId: `dev-1`, teamId: TEAM_A })
    expect(h.state.updates).toHaveLength(1)

    h.state.updates = []
    h.state.updateReturning = [[{ id: `row-1` }]]
    h.state.selectQueue = sharedProbe(TEAM_A)
    await caller.setShared({ deviceId: `dev-1`, teamId: TEAM_A })
    expect(h.state.updates).toHaveLength(1)
    expect(h.getTeamMember).not.toHaveBeenCalled()
  })
})

// EXP-432: the team-scoped list — teammates' shared servers.
describe(`devices.list({teamId})`, () => {
  const TEAM = `11111111-1111-4111-8111-111111111111`
  const sharedJoinRow = (over: Record<string, unknown> = {}) => ({
    device: registryRow({
      id: `row-2`,
      userId: `teammate`,
      deviceId: `dev-2`,
      label: `teambox`,
      sharedTeamId: TEAM,
      ...over,
    }),
    ownerName: `Tessa Teammate`,
  })

  it(`asserts membership and appends teammates' shared rows with owner attribution`, async () => {
    h.state.selectQueue = [[registryRow()], [sharedJoinRow()]]
    h.relayGetDevices.mockResolvedValue({ devices: [] })
    const { devices } = await caller.list({ teamId: TEAM })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM)
    expect(devices).toHaveLength(2)
    // Own rows first, teammate rows appended after.
    expect(devices[0]).toMatchObject({ deviceId: `dev-1`, sharedTeamId: null })
    expect(devices[0]?.owner).toBeUndefined()
    expect(devices[1]).toMatchObject({
      deviceId: `dev-2`,
      kind: `server`,
      deviceLabel: `teambox`,
      sharedTeamId: TEAM,
      owner: { id: `teammate`, name: `Tessa Teammate` },
      registered: true,
    })
  })

  it(`merges the OWNER's relay presence onto a shared row`, async () => {
    h.state.selectQueue = [[], [sharedJoinRow()]]
    h.relayGetDevices.mockImplementation((_config, userId: string) =>
      userId === `teammate`
        ? Promise.resolve({
            devices: [
              {
                deviceId: `dev-2`,
                deviceLabel: `teambox`,
                connectedAt: 1,
                agents: [`claude`, `codex`],
                caps: [`actions`, `action-inputs`],
              },
            ],
          })
        : Promise.resolve({ devices: [] })
    )
    const { devices } = await caller.list({ teamId: TEAM })
    expect(devices[0]).toMatchObject({
      deviceId: `dev-2`,
      online: true,
      // Live advertisement wins over the registered snapshot.
      agents: [`claude`, `codex`],
      caps: [`actions`, `action-inputs`],
    })
  })

  it(`reads a shared row offline when its owner's presence fetch fails`, async () => {
    h.state.selectQueue = [[], [sharedJoinRow()]]
    h.relayGetDevices.mockImplementation((_config, userId: string) =>
      userId === `teammate`
        ? Promise.reject(new Error(`relay down`))
        : Promise.resolve({ devices: [] })
    )
    const { devices } = await caller.list({ teamId: TEAM })
    expect(devices[0]).toMatchObject({ deviceId: `dev-2`, online: false })
  })

  it(`never writes other users' rows on observe — only own rows bump last_seen_at`, async () => {
    h.state.selectQueue = [[], [sharedJoinRow()]]
    h.relayGetDevices.mockResolvedValue({
      devices: [{ deviceId: `dev-2`, deviceLabel: `teambox`, connectedAt: 1 }],
    })
    await caller.list({ teamId: TEAM })
    expect(h.state.updates).toHaveLength(0)
  })

  it(`carries the caller's own sharedTeamId so the UI can badge it`, async () => {
    h.state.selectQueue = [[registryRow({ sharedTeamId: TEAM })], []]
    const { devices } = await caller.list({ teamId: TEAM })
    expect(devices[0]).toMatchObject({ deviceId: `dev-1`, sharedTeamId: TEAM })
    expect(devices[0]?.owner).toBeUndefined()
  })

  it(`no-input list stays byte-identical to the pre-EXP-432 behavior`, async () => {
    h.state.selectRows = [registryRow()]
    const { devices } = await caller.list()
    expect(h.assertTeamMember).not.toHaveBeenCalled()
    expect(devices).toHaveLength(1)
    // Exactly one relay fetch — never a per-owner sweep without teamId.
    expect(h.relayGetDevices).toHaveBeenCalledTimes(1)
  })
})

// EXP-481: server-authoritative launch defaults.
describe(`devices.setLaunchDefaults`, () => {
  const deviceRow = (over: Record<string, unknown> = {}) => [
    [
      {
        id: `row-1`,
        caps: [`actions`, `worktrees`, `launch-defaults`, `resume`],
        launchDefaults: null,
        launchDefaultsUpdatedAt: null,
        ...over,
      },
    ],
  ]

  it(`404s an unknown device`, async () => {
    h.state.selectQueue = [[]]
    await expect(
      caller.setLaunchDefaults({
        deviceId: `dev-x`,
        launchDefaults: { defaultAgent: `claude` },
      })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`UI edit (no expectedUpdatedAt) writes unconditionally, clamps vocab, nudges`, async () => {
    h.state.selectQueue = deviceRow()
    const result = await caller.setLaunchDefaults({
      deviceId: `dev-1`,
      launchDefaults: {
        defaultAgent: `codex`,
        agents: {
          claude: { model: `fable`, ultracode: true },
          // Invalid model + unsupported toggle are dropped FIELD-wise, and
          // codex keeps its valid skipPermissions.
          codex: { model: `not-a-model`, ultracode: true, skipPermissions: true },
        },
      },
    })
    expect(result.ok).toBe(true)
    expect(result.conflict).toBeUndefined()
    expect(result.launchDefaults).toEqual({
      defaultAgent: `codex`,
      agents: {
        claude: { model: `fable`, ultracode: true },
        codex: { skipPermissions: true },
      },
    })
    expect(result.launchDefaultsUpdatedAt).toEqual(expect.any(String))
    expect(result.txid).toBe(42)
    expect(h.state.updates[0]?.set).toMatchObject({
      launchDefaults: result.launchDefaults,
    })
    expect(h.relayPostNudge).toHaveBeenCalledWith(
      expect.anything(),
      `actor`,
      `dev-1`
    )
  })

  it(`device push with a stale stamp gets conflict + the server copy`, async () => {
    const serverCopy = { defaultAgent: `claude` }
    h.state.selectQueue = deviceRow({
      launchDefaults: serverCopy,
      launchDefaultsUpdatedAt: new Date(`2026-08-10T10:00:00Z`),
    })
    const result = await caller.setLaunchDefaults({
      deviceId: `dev-1`,
      launchDefaults: { defaultAgent: `pi` },
      expectedUpdatedAt: null,
    })
    expect(result).toMatchObject({
      ok: false,
      conflict: true,
      launchDefaults: serverCopy,
      launchDefaultsUpdatedAt: `2026-08-10T10:00:00.000Z`,
    })
    expect(h.state.updates).toHaveLength(0)
    expect(h.relayPostNudge).not.toHaveBeenCalled()
  })

  it(`device push with the matching stamp wins`, async () => {
    h.state.selectQueue = deviceRow({
      launchDefaults: { defaultAgent: `claude` },
      launchDefaultsUpdatedAt: new Date(`2026-08-10T10:00:00Z`),
    })
    const result = await caller.setLaunchDefaults({
      deviceId: `dev-1`,
      launchDefaults: { defaultAgent: `pi` },
      expectedUpdatedAt: `2026-08-10T10:00:00.000Z`,
    })
    expect(result.ok).toBe(true)
    expect(result.launchDefaults).toEqual({ defaultAgent: `pi` })
  })

  it(`tolerates 0.14.10's explicit-null toggles on a device push (EXP-495)`, async () => {
    h.state.selectQueue = deviceRow()
    const result = await caller.setLaunchDefaults({
      deviceId: `dev-1`,
      launchDefaults: {
        defaultAgent: `pi`,
        agents: {
          pi: { model: ``, effort: ``, ultracode: null, planMode: false, skipPermissions: null },
        },
      },
      expectedUpdatedAt: null,
    })
    expect(result.ok).toBe(true)
    expect(JSON.stringify(result.launchDefaults)).not.toContain(`null`)
    expect(result.launchDefaults).toEqual({
      defaultAgent: `pi`,
      agents: { pi: { model: ``, effort: ``, planMode: false } },
    })
  })

  it(`nudges regardless of registered caps (pre-EXP-481 frame parsers retired)`, async () => {
    h.state.selectQueue = deviceRow({ caps: [`actions`] })
    const result = await caller.setLaunchDefaults({
      deviceId: `dev-1`,
      launchDefaults: { defaultAgent: `claude` },
    })
    expect(result.ok).toBe(true)
    expect(h.relayPostNudge).toHaveBeenCalled()
  })
})

// EXP-481: worktree inventory reporting.
describe(`devices.reportWorktrees`, () => {
  const wt = (branch: string, over: Record<string, unknown> = {}) => ({
    repoFullName: `acme/api`,
    branch,
    dirty: `clean` as const,
    busy: false,
    ...over,
  })

  it(`404s an unregistered device`, async () => {
    h.state.selectQueue = [[]]
    await expect(
      caller.reportWorktrees({ deviceId: `dev-x`, worktrees: [] })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`dedupes, upserts sorted, and deletes rows missing from the report`, async () => {
    h.state.selectQueue = [[{ id: `row-1` }]]
    const result = await caller.reportWorktrees({
      deviceId: `dev-1`,
      worktrees: [
        wt(`exp/EXP-2`, { issueIdentifier: `EXP-2`, agents: [`claude`] }),
        wt(`exp/EXP-1`),
        // Duplicate key — last one wins, single upsert.
        wt(`exp/EXP-2`, { dirty: `tracked`, busy: true }),
      ],
    })
    expect(result).toEqual({ ok: true })
    expect(h.state.inserted).toHaveLength(2)
    // Sorted by (repo, branch): EXP-1 before EXP-2.
    expect(h.state.inserted[0]).toMatchObject({ branch: `exp/EXP-1` })
    expect(h.state.inserted[1]).toMatchObject({
      branch: `exp/EXP-2`,
      dirty: `tracked`,
      busy: true,
    })
    // The not-in-report delete ran.
    expect(h.state.deletes).toBe(1)
  })

  it(`an empty report clears the device's rows`, async () => {
    h.state.selectQueue = [[{ id: `row-1` }]]
    await caller.reportWorktrees({ deviceId: `dev-1`, worktrees: [] })
    expect(h.state.inserted).toHaveLength(0)
    expect(h.state.deletes).toBe(1)
  })

  it(`bounds the report at 256 rows`, async () => {
    const rows = Array.from({ length: 257 }, (_, i) => wt(`exp/EXP-${i}`))
    await expect(
      caller.reportWorktrees({ deviceId: `dev-1`, worktrees: rows })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})

// EXP-481: the owner→device command queue.
describe(`devices.createCommand / completeCommand / getCommand`, () => {
  const deviceProbe = () => [
    [{ id: `row-1`, caps: [`worktrees`, `launch-defaults`, `resume`] }],
  ]

  it(`queues a prune, nudges, and returns the id`, async () => {
    h.state.selectQueue = [...deviceProbe(), []]
    h.state.insertReturning = [[{ id: `cmd-9` }]]
    const result = await caller.createCommand({
      deviceId: `dev-1`,
      kind: `worktree_prune`,
    })
    expect(result).toEqual({ id: `cmd-9` })
    expect(h.state.inserted[0]).toMatchObject({
      deviceRowId: `row-1`,
      userId: `actor`,
      kind: `worktree_prune`,
      payload: {},
    })
    expect(h.relayPostNudge).toHaveBeenCalled()
  })

  it(`worktree_remove requires repo + branch`, async () => {
    h.state.selectQueue = deviceProbe()
    await expect(
      caller.createCommand({ deviceId: `dev-1`, kind: `worktree_remove` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`worktree_remove 404s when the target is no longer reported`, async () => {
    h.state.selectQueue = [...deviceProbe(), []]
    await expect(
      caller.createCommand({
        deviceId: `dev-1`,
        kind: `worktree_remove`,
        repoFullName: `acme/api`,
        branch: `exp/EXP-1`,
      })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`refuses a duplicate pending command`, async () => {
    h.state.selectQueue = [...deviceProbe(), [{ id: `dup-1` }]]
    await expect(
      caller.createCommand({ deviceId: `dev-1`, kind: `worktree_prune` })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    expect(h.state.inserted).toHaveLength(0)
  })

  it(`completeCommand transitions pending → done once; duplicates are tolerated`, async () => {
    h.state.updateReturning = [[{ id: `cmd-9` }], []]
    const first = await caller.completeCommand({
      commandId: `33333333-3333-4333-8333-333333333333`,
      ok: true,
      message: `Removed exp/EXP-1`,
    })
    expect(first).toEqual({ ok: true })
    expect(h.state.updates[0]?.set).toMatchObject({
      status: `done`,
      result: `Removed exp/EXP-1`,
    })
    // Redelivery races the first completion — second write finds no pending
    // row and reports ok:false instead of erroring.
    const second = await caller.completeCommand({
      commandId: `33333333-3333-4333-8333-333333333333`,
      ok: false,
    })
    expect(second).toEqual({ ok: false })
  })

  it(`getCommand is an owner-scoped point read`, async () => {
    h.state.selectQueue = [
      [
        {
          id: `cmd-9`,
          kind: `worktree_prune`,
          payload: {},
          status: `done`,
          result: `Pruned 2 worktrees`,
          completedAt: new Date(`2026-08-11T10:00:00Z`),
          createdAt: new Date(`2026-08-11T09:59:00Z`),
        },
      ],
    ]
    const command = await caller.getCommand({
      commandId: `33333333-3333-4333-8333-333333333333`,
    })
    expect(command).toMatchObject({ id: `cmd-9`, status: `done` })

    h.state.selectQueue = [[]]
    await expect(
      caller.getCommand({ commandId: `33333333-3333-4333-8333-333333333333` })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})

// EXP-481: the heartbeat is the device's work pull.
describe(`devices.heartbeat — work pull (EXP-481)`, () => {
  const heartbeatRow = (over: Record<string, unknown> = {}) => [
    [
      {
        id: `row-1`,
        updateRequestedAt: null,
        launchDefaults: { defaultAgent: `codex` },
        launchDefaultsUpdatedAt: new Date(`2026-08-10T10:00:00Z`),
        ...over,
      },
    ],
  ]
  // The live-row response arm (the gone-row arm carries neither commands nor
  // defaults) — narrows the union for direct property assertions.
  type WorkPull = Extract<
    Awaited<ReturnType<typeof caller.heartbeat>>,
    { commands: unknown }
  >

  it(`delivers pending commands with every beat`, async () => {
    h.state.updateReturning = heartbeatRow()
    h.state.selectRows = [
      { id: `cmd-1`, kind: `worktree_prune`, payload: {} },
    ]
    const result = (await caller.heartbeat({ deviceId: `dev-1` })) as WorkPull
    expect(result.ok).toBe(true)
    expect(result.commands).toEqual([
      { id: `cmd-1`, kind: `worktree_prune`, payload: {} },
    ])
  })

  it(`includes launch defaults ONLY when the device's stamp differs`, async () => {
    // Old daemon: no defaultsSyncedAt — no defaults in the response.
    h.state.updateReturning = heartbeatRow()
    const legacy = await caller.heartbeat({ deviceId: `dev-1` })
    expect(legacy).not.toHaveProperty(`launchDefaults`)

    // Stale device stamp (null = never converged) → defaults included.
    h.state.updateReturning = heartbeatRow()
    const stale = (await caller.heartbeat({
      deviceId: `dev-1`,
      defaultsSyncedAt: null,
    })) as WorkPull
    expect(stale.launchDefaults).toEqual({ defaultAgent: `codex` })
    expect(stale.launchDefaultsUpdatedAt).toBe(`2026-08-10T10:00:00.000Z`)

    // Converged stamp → steady-state response stays tiny.
    h.state.updateReturning = heartbeatRow()
    const converged = await caller.heartbeat({
      deviceId: `dev-1`,
      defaultsSyncedAt: `2026-08-10T10:00:00.000Z`,
    })
    expect(converged).not.toHaveProperty(`launchDefaults`)
  })
})
