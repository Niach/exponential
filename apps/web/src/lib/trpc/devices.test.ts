import { describe, expect, it, vi, beforeEach } from "vitest"

const h = vi.hoisted(() => {
  const state = {
    selectRows: [] as unknown[],
    inserted: [] as unknown[],
    upserts: [] as unknown[],
    updates: [] as { set: unknown; returningRows: unknown[] }[],
    updateReturning: [[{ id: `row-1` }]] as unknown[][],
    deletes: 0,
  }
  const db = {
    select: () => ({
      from: () => ({
        where: () => ({
          orderBy: () => Promise.resolve(state.selectRows),
        }),
      }),
    }),
    insert: () => ({
      values: (values: unknown) => {
        state.inserted.push(values)
        return {
          onConflictDoUpdate: (upsert: unknown) => {
            state.upserts.push(upsert)
            return Promise.resolve()
          },
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
  }
  return {
    state,
    db,
    getSteerRelayConfig: vi.fn(),
    relayGetDevices: vi.fn(),
  }
})

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayGetDevices: h.relayGetDevices,
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
  createdAt: new Date(`2026-07-01T10:00:00Z`),
  updatedAt: new Date(`2026-08-01T10:00:00Z`),
  ...over,
})

beforeEach(() => {
  vi.clearAllMocks()
  h.state.selectRows = []
  h.state.inserted = []
  h.state.upserts = []
  h.state.updates = []
  h.state.updateReturning = [[{ id: `row-1` }]]
  h.state.deletes = 0
  h.getSteerRelayConfig.mockReturnValue({
    url: `ws://relay`,
    secret: `s`,
    enabled: true,
  })
  h.relayGetDevices.mockResolvedValue({ devices: [] })
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
    expect(result).toEqual({ ok: true })
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
    expect(await caller.heartbeat({ deviceId: `dev-1` })).toEqual({
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
    expect(result).toEqual({ ok: true, updateRequested: false })
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

  it(`synthesizes an entry for a connected device that never registered`, async () => {
    h.state.selectRows = []
    h.relayGetDevices.mockResolvedValue({
      devices: [
        { deviceId: `old-desktop`, deviceLabel: `Old Mac`, connectedAt: 1 },
      ],
    })
    const { devices } = await caller.list()
    expect(devices).toHaveLength(1)
    expect(devices[0]).toMatchObject({
      deviceId: `old-desktop`,
      kind: `desktop`,
      online: true,
      registered: false,
      lastSeenAt: null,
      agents: [`claude`],
      caps: [],
    })
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
    expect(result).toEqual({ ok: true })
    expect(h.state.deletes).toBe(1)
  })
})
