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

import {
  clampAgentAccounts,
  clampAgentUsage,
  devicesRouter,
} from "@/lib/trpc/devices"

const caller = devicesRouter.createCaller({
  session: { user: { id: `actor`, name: `Actor`, email: `a@example.com` } },
  db: h.db,
  request: new Request(`http://localhost/`),
} as never)

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
    const wire = {
      defaultAgent: `claude`,
      agents: {
        claude: {
          model: `fable`,
          effort: ``,
          ultracode: false,
          planMode: true,
        },
        codex: {
          model: ``,
          effort: ``,
          ultracode: null,
          planMode: null,
        },
        pi: {
          model: ``,
          effort: ``,
          ultracode: null,
          planMode: true,
        },
      },
    }
    const result = await caller.register({
      deviceId: `dev-1`,
      label: `unraid-runner`,
      kind: `server`,
      launchDefaults: wire,
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
        },
        codex: { model: ``, effort: `` },
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
    expect(
      await caller.heartbeat({
        deviceId: `dev-1`,
        activeSessions: 0,
        defaultsSyncedAt: null,
      })
    ).toMatchObject({
      ok: true,
      updateRequested: true,
    })
  })
})

describe(`devices.heartbeat`, () => {
  it(`reports ok: false when the row was removed, so the daemon re-registers`, async () => {
    h.state.updateReturning = [[]]
    const result = await caller.heartbeat({
      deviceId: `dev-gone`,
      activeSessions: 0,
      defaultsSyncedAt: null,
    })
    expect(result).toEqual({ ok: false, updateRequested: false })
  })

  it(`bumps last_seen_at for a live row`, async () => {
    const result = await caller.heartbeat({
      deviceId: `dev-1`,
      activeSessions: 0,
      defaultsSyncedAt: null,
    })
    expect(result).toMatchObject({ ok: true, updateRequested: false })
    expect(h.state.updates[0]?.set).toMatchObject({
      lastSeenAt: expect.any(Date),
    })
  })

  it(`stores the reported live-session count (EXP-411)`, async () => {
    await caller.heartbeat({
      deviceId: `dev-1`,
      activeSessions: 2,
      defaultsSyncedAt: null,
    })
    expect(h.state.updates[0]?.set).toMatchObject({ activeSessions: 2 })
  })

  it(`rejects a beat omitting the required fields (pre-EXP-481 daemons retired)`, async () => {
    await expect(
      caller.heartbeat({ deviceId: `dev-1` } as never)
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})

describe(`devices.latestVersions`, () => {
  it(`returns the desktop/cli latest hints`, async () => {
    const result = await caller.latestVersions()
    expect(result).toEqual({ desktop: `0.9.0`, cli: `0.9.0` })
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

// EXP-622: the caller's default machine — at most one true row per user.
describe(`devices.setDefault`, () => {
  it(`clears the caller's other defaults before flagging this row`, async () => {
    h.state.selectQueue = [[{ id: `row-1` }]]
    const result = await caller.setDefault({
      deviceId: `dev-1`,
      isDefault: true,
    })
    expect(result).toMatchObject({ ok: true })
    expect(h.state.updates).toHaveLength(2)
    expect(h.state.updates[0]?.set).toMatchObject({ isDefault: false })
    expect(h.state.updates[1]?.set).toMatchObject({ isDefault: true })
  })

  it(`clearing touches only this row`, async () => {
    h.state.selectQueue = [[{ id: `row-1` }]]
    const result = await caller.setDefault({
      deviceId: `dev-1`,
      isDefault: false,
    })
    expect(result).toMatchObject({ ok: true })
    expect(h.state.updates).toHaveLength(1)
    expect(h.state.updates[0]?.set).toMatchObject({ isDefault: false })
  })

  it(`rejects a deviceId the caller does not own`, async () => {
    h.state.selectQueue = [[]]
    await expect(
      caller.setDefault({ deviceId: `foreign-dev`, isDefault: true })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(h.state.updates).toHaveLength(0)
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

    expect(h.endForeignHostedSessions).toHaveBeenCalledWith(
      `actor`,
      TEAM_A,
      `dev-1`
    )
    expect(updatesWhenKilled).toBe(2)
  })

  it(`ends the OLD team's sessions when the device moves to another team`, async () => {
    h.state.selectQueue = sharedProbe(TEAM_A)

    await caller.setShared({ deviceId: `dev-1`, teamId: TEAM_B })

    expect(h.state.updates[0]?.set).toMatchObject({ sharedTeamId: TEAM_B })
    // Device-scoped since EXP-560 — only this machine's foreign runs die.
    expect(h.endForeignHostedSessions).toHaveBeenCalledWith(
      `actor`,
      TEAM_A,
      `dev-1`
    )
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
    const wire = {
      defaultAgent: `codex`,
      agents: {
        claude: { model: `fable`, ultracode: true },
        // Invalid model + unsupported toggle are dropped FIELD-wise; the
        // valid effort survives.
        codex: {
          model: `not-a-model`,
          effort: `high`,
          ultracode: true,
        },
      },
    }
    const result = await caller.setLaunchDefaults({
      deviceId: `dev-1`,
      launchDefaults: wire,
    })
    expect(result.ok).toBe(true)
    expect(result.conflict).toBeUndefined()
    expect(result.launchDefaults).toEqual({
      defaultAgent: `codex`,
      agents: {
        claude: { model: `fable`, ultracode: true },
        codex: { effort: `high` },
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
    const wire = {
      defaultAgent: `pi`,
      agents: {
        pi: { model: ``, effort: ``, ultracode: null, planMode: false },
      },
    }
    const result = await caller.setLaunchDefaults({
      deviceId: `dev-1`,
      launchDefaults: wire,
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
    const result = (await caller.heartbeat({
      deviceId: `dev-1`,
      activeSessions: 0,
      defaultsSyncedAt: `2026-08-10T10:00:00.000Z`,
    })) as WorkPull
    expect(result.ok).toBe(true)
    expect(result.commands).toEqual([
      { id: `cmd-1`, kind: `worktree_prune`, payload: {} },
    ])
  })

  it(`includes launch defaults ONLY when the device's stamp differs`, async () => {
    // Stale device stamp (null = never converged) → defaults included.
    h.state.updateReturning = heartbeatRow()
    const stale = (await caller.heartbeat({
      deviceId: `dev-1`,
      activeSessions: 0,
      defaultsSyncedAt: null,
    })) as WorkPull
    expect(stale.launchDefaults).toEqual({ defaultAgent: `codex` })
    expect(stale.launchDefaultsUpdatedAt).toBe(`2026-08-10T10:00:00.000Z`)

    // Converged stamp → steady-state response stays tiny.
    h.state.updateReturning = heartbeatRow()
    const converged = await caller.heartbeat({
      deviceId: `dev-1`,
      activeSessions: 0,
      defaultsSyncedAt: `2026-08-10T10:00:00.000Z`,
    })
    expect(converged).not.toHaveProperty(`launchDefaults`)
  })
})

// EXP-484: the machine's read-only per-agent auth + usage status.
describe(`agent status clamps (EXP-484)`, () => {
  const NOW = new Date(`2026-08-28T12:00:00.000Z`)

  it(`clamps accounts to contract agents and strips nulls`, () => {
    const out = clampAgentAccounts({
      claude: {
        signedIn: true,
        email: `danny@example.com`,
        plan: `Max`,
        checkedAt: `2026-08-28T11:59:00Z`,
      },
      // Explicit nulls (the EXP-495 shape) degrade field-wise.
      codex: { signedIn: false, email: null, plan: null, checkedAt: null },
      // Not a contract agent — dropped whole.
      aider: { signedIn: true, email: `x@y.z` },
    })
    expect(out).toEqual({
      claude: {
        signedIn: true,
        email: `danny@example.com`,
        plan: `Max`,
        checkedAt: `2026-08-28T11:59:00.000Z`,
      },
      codex: { signedIn: false },
    })
    expect(JSON.stringify(out)).not.toContain(`null`)
  })

  it(`drops an unparsable checkedAt instead of failing the write`, () => {
    expect(
      clampAgentAccounts({ pi: { signedIn: true, checkedAt: `yesterday` } })
    ).toEqual({ pi: { signedIn: true } })
  })

  it(`rounds and clamps percent, caps windows, truncates key and label`, () => {
    const out = clampAgentUsage(
      {
        claude: {
          fetchedAt: `2026-08-28T11:55:00Z`,
          stale: null,
          windows: [
            { key: `session`, label: `5h`, percent: 41.6, resetsAt: null },
            { key: `weekly`, label: `Week`, percent: 137 },
            { key: `credits`, label: `Credits`, percent: -3 },
            { key: `x`.repeat(80), label: `y`.repeat(50), percent: 1 },
            // No key and no label — nothing to render.
            { percent: 5 },
            ...Array.from({ length: 12 }, (_, i) => ({
              key: `w${i}`,
              label: `W${i}`,
              percent: i,
            })),
          ],
        },
      },
      NOW
    )
    const windows = out.claude.windows
    expect(windows).toHaveLength(10)
    expect(windows[0]).toEqual({
      key: `session`,
      label: `5h`,
      percent: 42,
      resetsAt: null,
    })
    expect(windows[1].percent).toBe(100)
    expect(windows[2].percent).toBe(0)
    expect(windows[3].key).toHaveLength(64)
    expect(windows[3].label).toHaveLength(32)
    expect(out.claude.fetchedAt).toBe(`2026-08-28T11:55:00.000Z`)
    expect(out.claude.stale).toBe(false)
  })

  it(`falls back to the write time when the device sends no fetchedAt`, () => {
    const out = clampAgentUsage({ codex: { windows: [] } }, NOW)
    expect(out.codex.fetchedAt).toBe(NOW.toISOString())
  })

  it(`drops agents outside the contract`, () => {
    expect(clampAgentUsage({ aider: { windows: [] } }, NOW)).toEqual({})
  })
})

describe(`devices.register / heartbeat agent status (EXP-484)`, () => {
  const heartbeatRow = () => [
    [
      {
        id: `row-1`,
        updateRequestedAt: null,
        launchDefaults: null,
        launchDefaultsUpdatedAt: null,
      },
    ],
  ]

  it(`register stores the reported accounts`, async () => {
    await caller.register({
      deviceId: `dev-1`,
      label: `buildbox`,
      kind: `desktop`,
      agentAccounts: { claude: { signedIn: true, plan: `Max` } },
    })
    expect(h.state.inserted[0]).toMatchObject({
      agentAccounts: { claude: { signedIn: true, plan: `Max` } },
    })
    const upsert = h.state.upserts[0] as { set: Record<string, unknown> }
    expect(upsert.set.agentAccounts).toEqual({
      claude: { signedIn: true, plan: `Max` },
    })
  })

  it(`a register without accounts leaves the column untouched`, async () => {
    await caller.register({
      deviceId: `dev-1`,
      label: `buildbox`,
      kind: `desktop`,
    })
    const upsert = h.state.upserts[0] as { set: Record<string, unknown> }
    expect(upsert.set).not.toHaveProperty(`agentAccounts`)
  })

  it(`heartbeat writes usage plus its stamp`, async () => {
    h.state.updateReturning = heartbeatRow()
    await caller.heartbeat({
      deviceId: `dev-1`,
      activeSessions: 0,
      defaultsSyncedAt: null,
      agentAccounts: { codex: { signedIn: false } },
      agentUsage: {
        codex: {
          fetchedAt: `2026-08-28T11:55:00Z`,
          windows: [{ key: `session`, label: `5h`, percent: 12 }],
        },
      },
    })
    const set = h.state.updates[0]?.set as Record<string, unknown>
    expect(set.agentAccounts).toEqual({ codex: { signedIn: false } })
    expect(set.agentUsage).toMatchObject({
      codex: { windows: [{ key: `session`, label: `5h`, percent: 12 }] },
    })
    expect(set.agentUsageAt).toBeInstanceOf(Date)
  })

  it(`a heartbeat without agent status touches neither column`, async () => {
    h.state.updateReturning = heartbeatRow()
    await caller.heartbeat({
      deviceId: `dev-1`,
      activeSessions: 0,
      defaultsSyncedAt: null,
    })
    const set = h.state.updates[0]?.set as Record<string, unknown>
    expect(set).not.toHaveProperty(`agentAccounts`)
    expect(set).not.toHaveProperty(`agentUsage`)
    expect(set).not.toHaveProperty(`agentUsageAt`)
  })
})

// EXP-484: the remote sign-in command.
describe(`devices.createCommand — agent_login`, () => {
  const capableProbe = () => [
    [{ id: `row-1`, caps: [`worktrees`, `agent-login`] }],
  ]

  it(`queues the agent and the switch flag as strings`, async () => {
    h.state.selectQueue = [...capableProbe(), []]
    h.state.insertReturning = [[{ id: `cmd-1` }]]
    const result = await caller.createCommand({
      deviceId: `dev-1`,
      kind: `agent_login`,
      agent: `claude`,
      switch: true,
    })
    expect(result).toEqual({ id: `cmd-1` })
    expect(h.state.inserted[0]).toMatchObject({
      deviceRowId: `row-1`,
      kind: `agent_login`,
      payload: { agent: `claude`, switch: `true` },
    })
    expect(h.relayPostNudge).toHaveBeenCalled()
  })

  it(`defaults switch to false`, async () => {
    h.state.selectQueue = [...capableProbe(), []]
    await caller.createCommand({
      deviceId: `dev-1`,
      kind: `agent_login`,
      agent: `codex`,
    })
    expect(h.state.inserted[0]).toMatchObject({
      payload: { agent: `codex`, switch: `false` },
    })
  })

  it(`needs an agent`, async () => {
    h.state.selectQueue = capableProbe()
    await expect(
      caller.createCommand({ deviceId: `dev-1`, kind: `agent_login` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`refuses pi — its sign-in has no device-code flow`, async () => {
    h.state.selectQueue = capableProbe()
    await expect(
      caller.createCommand({
        deviceId: `dev-1`,
        kind: `agent_login`,
        agent: `pi`,
      })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `pi has no remote sign-in`,
    })
  })

  it(`refuses a machine that does not advertise the cap`, async () => {
    h.state.selectQueue = [[{ id: `row-1`, caps: [`worktrees`] }]]
    await expect(
      caller.createCommand({
        deviceId: `dev-1`,
        kind: `agent_login`,
        agent: `claude`,
      })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    expect(h.state.inserted).toHaveLength(0)
  })

  it(`dedupes a second identical login while one is pending`, async () => {
    h.state.selectQueue = [...capableProbe(), [{ id: `cmd-1` }]]
    await expect(
      caller.createCommand({
        deviceId: `dev-1`,
        kind: `agent_login`,
        agent: `claude`,
      })
    ).rejects.toMatchObject({ code: `CONFLICT` })
  })
})
