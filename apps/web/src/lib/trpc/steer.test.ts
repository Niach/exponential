import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// steer.startSession accepts EITHER a single issueId (wire-unchanged) or
// issueIds (2..30 → batch). It resolves every issue's team + repo
// server-side, enforces one-team / one-repo, and routes a legacy body
// for a single (or duplicate-collapsed) id vs a "fat" batch body (issueIds +
// teamId + repo, installationId stripped) for 2+. The relay call is
// mocked, so a caller + a handful of stubs is enough.

const h = vi.hoisted(() => {
  // Minimal drizzle select-chain: each awaited query pops the next row set
  // off the queue (the action branch runs its selects in a fixed order).
  // The popped rows are additionally FILTERED against the query's own eq()
  // conditions (EXP-432: the shared-device lookup's team/kind scoping lives
  // in the where clause, so a queued row that the real query would not match
  // must not resolve here either). A row is only checked on keys it actually
  // carries, so narrow projections stay unaffected.
  const dbQueue: unknown[][] = []
  type WherePart = { column?: string; value?: unknown }
  const walkWhere = (node: unknown, out: WherePart[] = []): WherePart[] => {
    if (!node || typeof node !== `object`) return out
    if (Array.isArray(node)) {
      for (const child of node) walkWhere(child, out)
      return out
    }
    const rec = node as Record<string, unknown>
    if (Array.isArray(rec.queryChunks)) return walkWhere(rec.queryChunks, out)
    if (`value` in rec && `encoder` in rec) {
      out.push({ value: rec.value })
      return out
    }
    if (typeof rec.name === `string` && rec.table) {
      out.push({ column: rec.name })
      return out
    }
    return out
  }
  const camel = (name: string) =>
    name.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase())
  const matches = (row: Record<string, unknown>, parts: WherePart[]) =>
    parts.every((part, i) => {
      const next = parts[i + 1]
      if (part.column === undefined || !next || next.column !== undefined) {
        return true
      }
      const key = camel(part.column)
      return !(key in row) || row[key] === next.value
    })
  const makeChain = () => {
    let cond: unknown
    const chain = {
      from: () => chain,
      innerJoin: () => chain,
      where: (value: unknown) => {
        cond = value
        return chain
      },
      limit: () => {
        const rows = (dbQueue.shift() ?? []) as Record<string, unknown>[]
        const parts = walkWhere(cond)
        return Promise.resolve(rows.filter((row) => matches(row, parts)))
      },
    }
    return chain
  }
  return {
    getSteerRelayConfig: vi.fn(),
    relayPostStart: vi.fn(),
    relayGetDevices: vi.fn(),
    mintSteerTicket: vi.fn(),
    relayPostKill: vi.fn(),
    assertTeamMember: vi.fn(),
    getIssueTeamContext: vi.fn(),
    resolveBoardRepository: vi.fn(),
    dbQueue,
    db: { select: () => makeChain() },
  }
})

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist (the action branch additionally runs queued
// selects — see h.dbQueue).
vi.mock(`@/db/connection`, () => ({ db: h.db }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  getIssueTeamContext: h.getIssueTeamContext,
}))
vi.mock(`@/lib/trpc/repositories`, () => ({
  resolveBoardRepository: h.resolveBoardRepository,
  // Mirrors the real helper (EXP-462): override wins over GitHub's default.
  effectiveDefaultBranch: (repo: {
    defaultBranch: string
    defaultBranchOverride: string | null
  }) => repo.defaultBranchOverride ?? repo.defaultBranch,
}))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayPostStart: h.relayPostStart,
  relayGetDevices: h.relayGetDevices,
  mintSteerTicket: h.mintSteerTicket,
  relayPostKill: h.relayPostKill,
}))

import { steerRouter } from "@/lib/trpc/steer"

const ISSUE_A = `11111111-1111-4111-8111-111111111111`
const ISSUE_B = `22222222-2222-4222-8222-222222222222`

function uuid(n: number): string {
  return `00000000-0000-4000-8000-${n.toString(16).padStart(12, `0`)}`
}

const caller = steerRouter.createCaller({
  session: { user: { id: `actor`, name: `Actor`, email: `a@example.com` } },
  db: {},
  request: new Request(`http://localhost/`),
} as never)

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}

function lastStartBody(): Record<string, unknown> {
  return h.relayPostStart.mock.calls.at(-1)![1] as Record<string, unknown>
}

beforeEach(() => {
  h.getSteerRelayConfig.mockReset()
  h.getSteerRelayConfig.mockReturnValue({
    url: `https://steer.example.com`,
    secret: `s`,
  })
  h.relayPostStart.mockReset()
  h.relayPostStart.mockResolvedValue({ ok: true })
  h.mintSteerTicket.mockReset()
  h.mintSteerTicket.mockResolvedValue({
    ticket: `tkt`,
    url: `wss://steer.example.com/ws?t=tkt`,
  })
  h.relayPostKill.mockReset()
  h.relayPostKill.mockResolvedValue(undefined)
  h.relayGetDevices.mockReset()
  // Default: the target device is online and advertises all three agents.
  h.relayGetDevices.mockResolvedValue({
    devices: [
      {
        deviceId: `dev-1`,
        deviceLabel: `MacBook`,
        connectedAt: 0,
        agents: [`claude`, `codex`, `pi`],
      },
    ],
  })
  h.assertTeamMember.mockReset()
  h.assertTeamMember.mockResolvedValue({ role: `member` })
  h.getIssueTeamContext.mockReset()
  h.getIssueTeamContext.mockImplementation(async (id: string) => ({
    issueId: id,
    boardId: `proj-${id}`,
    teamId: `ws-1`,
  }))
  h.resolveBoardRepository.mockReset()
  h.resolveBoardRepository.mockResolvedValue({
    repositoryId: `repo-1`,
    fullName: `acme/api`,
    defaultBranch: `main`,
    installationId: 42,
  })
  h.dbQueue.length = 0
})

describe(`steer.startSession — subject XOR`, () => {
  it(`rejects both issueId and issueIds as BAD_REQUEST`, async () => {
    const error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        issueIds: [ISSUE_B],
        deviceId: `dev-1`,
      })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`rejects neither issueId nor issueIds as BAD_REQUEST`, async () => {
    const error = await rejectionOf(caller.startSession({ deviceId: `dev-1` }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`rejects more than 30 issueIds as BAD_REQUEST`, async () => {
    const ids = Array.from({ length: 31 }, (_, i) => uuid(i))
    const error = await rejectionOf(
      caller.startSession({ issueIds: ids, deviceId: `dev-1` })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.getIssueTeamContext).not.toHaveBeenCalled()
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })
})

describe(`steer.startSession — server-side validation`, () => {
  it(`rejects issues spanning multiple teams`, async () => {
    h.getIssueTeamContext.mockImplementation(async (id: string) => ({
      issueId: id,
      boardId: `proj-${id}`,
      teamId: id === ISSUE_A ? `ws-1` : `ws-2`,
    }))
    const error = await rejectionOf(
      caller.startSession({ issueIds: [ISSUE_A, ISSUE_B], deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`one team`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`rejects issues spanning multiple repositories, naming both`, async () => {
    h.resolveBoardRepository.mockImplementation(
      async (boardId: string) => ({
        repositoryId: boardId === `proj-${ISSUE_A}` ? `repo-a` : `repo-b`,
        fullName: boardId === `proj-${ISSUE_A}` ? `acme/api` : `acme/web`,
        defaultBranch: `main`,
        installationId: 1,
      })
    )
    const error = await rejectionOf(
      caller.startSession({ issueIds: [ISSUE_A, ISSUE_B], deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`(acme/api vs acme/web)`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`rejects when a board has no linked repository`, async () => {
    h.resolveBoardRepository.mockResolvedValue(null)
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`No repository linked`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`maps a relay 404 to PRECONDITION_FAILED carrying the relay reason`, async () => {
    h.relayPostStart.mockResolvedValue({
      ok: false,
      status: 404,
      reason: `device_offline`,
    })
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toBe(`device_offline`)
  })
})

describe(`steer.startSession — routed body shape`, () => {
  it(`routes a single issueId as the legacy single-issue body`, async () => {
    await caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })
    const body = lastStartBody()
    expect(body).toMatchObject({
      userId: `actor`,
      deviceId: `dev-1`,
      issueId: ISSUE_A,
    })
    expect(`issueIds` in body).toBe(false)
    expect(`teamId` in body).toBe(false)
    expect(`repo` in body).toBe(false)
  })

  it(`routes a single-element issueIds as the legacy single-issue body`, async () => {
    await caller.startSession({ issueIds: [ISSUE_A], deviceId: `dev-1` })
    const body = lastStartBody()
    expect(body.issueId).toBe(ISSUE_A)
    expect(`issueIds` in body).toBe(false)
  })

  it(`collapses duplicate issueIds to one → legacy single-issue body`, async () => {
    await caller.startSession({
      issueIds: [ISSUE_A, ISSUE_A],
      deviceId: `dev-1`,
    })
    expect(h.getIssueTeamContext).toHaveBeenCalledTimes(1)
    const body = lastStartBody()
    expect(body.issueId).toBe(ISSUE_A)
    expect(`issueIds` in body).toBe(false)
  })

  it(`routes 2+ issues as a batch body with the repo group and no installationId`, async () => {
    await caller.startSession({
      issueIds: [ISSUE_A, ISSUE_B],
      deviceId: `dev-1`,
      ultracode: true,
    })
    const body = lastStartBody()
    expect(body).toMatchObject({
      userId: `actor`,
      deviceId: `dev-1`,
      issueIds: [ISSUE_A, ISSUE_B],
      teamId: `ws-1`,
      repo: {
        repositoryId: `repo-1`,
        fullName: `acme/api`,
        defaultBranch: `main`,
      },
      ultracode: true,
    })
    expect(`issueId` in body).toBe(false)
    expect(`installationId` in (body.repo as Record<string, unknown>)).toBe(
      false
    )
  })
})

describe(`steer.startSession — agent selection (EXP-201)`, () => {
  it(`forwards agent + skipPermissions to the relay body`, async () => {
    await caller.startSession({
      issueId: ISSUE_A,
      deviceId: `dev-1`,
      agent: `codex`,
      model: `gpt-5.6-sol`,
      effort: `xhigh`,
      skipPermissions: true,
    })
    expect(lastStartBody()).toMatchObject({
      issueId: ISSUE_A,
      agent: `codex`,
      model: `gpt-5.6-sol`,
      effort: `xhigh`,
      skipPermissions: true,
    })
  })

  it(`rejects an agent the device did not advertise`, async () => {
    h.relayGetDevices.mockResolvedValue({
      devices: [
        { deviceId: `dev-1`, deviceLabel: `Mac`, connectedAt: 0, agents: [`claude`] },
      ],
    })
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1`, agent: `codex` })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`codex is not installed`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`treats a device without an advertisement as claude-only`, async () => {
    h.relayGetDevices.mockResolvedValue({
      devices: [{ deviceId: `dev-1`, deviceLabel: `Mac`, connectedAt: 0 }],
    })
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1`, agent: `pi` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)

    // Claude (explicit or absent) still routes.
    await caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })
    expect(h.relayPostStart).toHaveBeenCalledTimes(1)
  })

  it(`validates model/effort against the AGENT's contract lists`, async () => {
    // A claude model on a codex start is unknown vocabulary.
    let error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        agent: `codex`,
        model: `fable`,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)

    // codex has no `max` effort.
    error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        agent: `codex`,
        effort: `max`,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)

    // Blank model is the codex/pi "CLI default" — valid.
    await caller.startSession({
      issueId: ISSUE_A,
      deviceId: `dev-1`,
      agent: `pi`,
      model: ``,
      effort: `max`,
    })
    expect(lastStartBody()).toMatchObject({ agent: `pi`, model: ``, effort: `max` })
  })

  it(`rejects claude-only toggles on a non-claude start`, async () => {
    let error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        agent: `codex`,
        ultracode: true,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)

    // Plan mode is claude/pi-only (EXP-441) — codex rejects it, pi carries
    // it through to the relay.
    error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        agent: `codex`,
        planMode: true,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)

    await caller.startSession({
      issueId: ISSUE_A,
      deviceId: `dev-1`,
      agent: `pi`,
      planMode: true,
    })
    expect(lastStartBody()).toMatchObject({ agent: `pi`, planMode: true })

    // pi has no permission system to skip.
    error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        agent: `pi`,
        skipPermissions: true,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
  })
})

// ── Action runs (EXP-253 base + EXP-257 full options / inputs / builtin) ──────

const ACTION_ID = `33333333-3333-4333-8333-333333333333`
const REPO_INPUT_ID = `44444444-4444-4444-8444-444444444444`
const BUILTIN_ID = `builtin:create-action`
const BUILTIN_TEAM_ID = `55555555-5555-4555-8555-555555555555`

function actionsCapableDevice(caps: string[], agents?: string[]) {
  h.relayGetDevices.mockResolvedValue({
    devices: [
      {
        deviceId: `dev-1`,
        deviceLabel: `MacBook`,
        connectedAt: 0,
        agents: agents ?? [`claude`, `codex`, `pi`],
        caps,
      },
    ],
  })
}

describe(`steer.startSession — action runs (EXP-257)`, () => {
  it(`forwards the FULL option set on an action start (no more Claude-only clamp)`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`])
    h.dbQueue.push([
      {
        id: ACTION_ID,
        teamId: `ws-1`,
        repositoryId: null,
        name: `Code review`,
        inputs: [],
      },
    ])
    await caller.startSession({
      actionId: ACTION_ID,
      deviceId: `dev-1`,
      agent: `codex`,
      model: `gpt-5.6-sol`,
      effort: `xhigh`,
      skipPermissions: true,
    })
    expect(lastStartBody()).toMatchObject({
      userId: `actor`,
      deviceId: `dev-1`,
      actionId: ACTION_ID,
      actionName: `Code review`,
      teamId: `ws-1`,
      agent: `codex`,
      model: `gpt-5.6-sol`,
      effort: `xhigh`,
      skipPermissions: true,
    })
    expect(`inputs` in lastStartBody()).toBe(false)
  })

  it(`still validates per-agent vocabulary on action starts`, async () => {
    const error = await rejectionOf(
      caller.startSession({
        actionId: ACTION_ID,
        deviceId: `dev-1`,
        agent: `codex`,
        ultracode: true,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`rejects an agent the device did not advertise on action starts`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`], [`claude`])
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: [] },
    ])
    const error = await rejectionOf(
      caller.startSession({
        actionId: ACTION_ID,
        deviceId: `dev-1`,
        agent: `codex`,
      })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`codex is not installed`)
  })

  it(`treats an explicitly empty agents list as nothing-runnable (EXP-409)`, async () => {
    // Every installed agent signed out: agents=[] must NOT fall back to
    // claude, and a signed-out agent gets the sign-in message.
    h.relayGetDevices.mockResolvedValue({
      devices: [
        {
          deviceId: `dev-1`,
          deviceLabel: `MacBook`,
          connectedAt: 0,
          agents: [],
          unauthedAgents: [`claude`],
          caps: [`actions`, `action-inputs`],
        },
      ],
    })
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: [] },
    ])
    const error = await rejectionOf(
      caller.startSession({ actionId: ACTION_ID, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(
      `claude is installed on that device but not signed in`
    )
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`refuses a device without the actions cap`, async () => {
    // Default beforeEach device advertises agents but NO caps.
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: [] },
    ])
    const error = await rejectionOf(
      caller.startSession({ actionId: ACTION_ID, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`can't run actions`)
  })

  it(`input-less runs stay allowed on an actions-only (pre-inputs) desktop`, async () => {
    actionsCapableDevice([`actions`])
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: [] },
    ])
    await caller.startSession({ actionId: ACTION_ID, deviceId: `dev-1` })
    expect(h.relayPostStart).toHaveBeenCalledTimes(1)
  })

  it(`non-claude runs require the action-inputs cap (pre-EXP-257 desktops clamp to claude)`, async () => {
    // A pre-EXP-257 desktop advertises `actions` + its full agent list, but
    // its action runner forces claude while honoring the model string — the
    // start would silently launch claude with a codex model.
    actionsCapableDevice([`actions`], [`claude`, `codex`])
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: [] },
    ])
    const error = await rejectionOf(
      caller.startSession({
        actionId: ACTION_ID,
        deviceId: `dev-1`,
        agent: `codex`,
        model: `gpt-5.6-sol`,
      })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`only run actions on claude`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`claude runs stay allowed on an actions-only desktop (options it ignores are not a regression)`, async () => {
    actionsCapableDevice([`actions`], [`claude`])
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: [] },
    ])
    await caller.startSession({
      actionId: ACTION_ID,
      deviceId: `dev-1`,
      agent: `claude`,
      model: `opus`,
      skipPermissions: true,
    })
    expect(h.relayPostStart).toHaveBeenCalledTimes(1)
  })

  it(`inputs-carrying runs require the action-inputs cap`, async () => {
    actionsCapableDevice([`actions`])
    h.dbQueue.push([
      {
        id: ACTION_ID,
        teamId: `ws-1`,
        repositoryId: null,
        name: `A`,
        inputs: [{ key: `topic`, label: `Topic`, type: `text`, required: false }],
      },
    ])
    const error = await rejectionOf(
      caller.startSession({
        actionId: ACTION_ID,
        deviceId: `dev-1`,
        inputs: { topic: `perf` },
      })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`action inputs`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`validates values against the schema — missing required, unknown key`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`])
    const defs = [{ key: `topic`, label: `Topic`, type: `text`, required: true }]
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: defs },
    ])
    let error = await rejectionOf(
      caller.startSession({ actionId: ACTION_ID, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`Missing required input`)

    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: defs },
    ])
    error = await rejectionOf(
      caller.startSession({
        actionId: ACTION_ID,
        deviceId: `dev-1`,
        inputs: { topic: `x`, bogus: `y` },
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`Unknown input`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`resolves repo-typed inputs to display names, team-scoped`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`])
    h.dbQueue.push([
      {
        id: ACTION_ID,
        teamId: `ws-1`,
        repositoryId: null,
        name: `A`,
        inputs: [{ key: `repo`, label: `Repository`, type: `repo`, required: true }],
      },
    ])
    // The repo lookup select.
    h.dbQueue.push([{ teamId: `ws-1`, fullName: `acme/api` }])
    await caller.startSession({
      actionId: ACTION_ID,
      deviceId: `dev-1`,
      inputs: { repo: REPO_INPUT_ID },
    })
    expect(lastStartBody().inputs).toEqual([
      {
        key: `repo`,
        label: `Repository`,
        type: `repo`,
        value: REPO_INPUT_ID,
        display: `acme/api`,
      },
    ])
  })

  it(`rejects a cross-team repo input value`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`])
    h.dbQueue.push([
      {
        id: ACTION_ID,
        teamId: `ws-1`,
        repositoryId: null,
        name: `A`,
        inputs: [{ key: `repo`, label: `Repository`, type: `repo`, required: true }],
      },
    ])
    h.dbQueue.push([{ teamId: `ws-OTHER`, fullName: `evil/repo` }])
    const error = await rejectionOf(
      caller.startSession({
        actionId: ACTION_ID,
        deviceId: `dev-1`,
        inputs: { repo: REPO_INPUT_ID },
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })
})

describe(`steer.startSession — builtin create-action (EXP-257)`, () => {
  it(`requires teamId for the builtin (and rejects teamId elsewhere)`, async () => {
    let error = await rejectionOf(
      caller.startSession({ actionId: BUILTIN_ID, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)

    error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        teamId: `11111111-1111-4111-8111-111111111111`,
        deviceId: `dev-1`,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`routes a builtin start with no DB action load and the resolved inputs`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`])
    const teamId = `55555555-5555-4555-8555-555555555555`
    await caller.startSession({
      actionId: BUILTIN_ID,
      teamId,
      deviceId: `dev-1`,
      inputs: { description: `Review PRs weekly` },
    })
    // No queued rows were consumed — the builtin never loads a DB action.
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, teamId)
    expect(lastStartBody()).toMatchObject({
      actionId: BUILTIN_ID,
      actionName: `Create action`,
      teamId,
      inputs: [
        {
          key: `description`,
          label: `Description`,
          type: `text`,
          value: `Review PRs weekly`,
          display: `Review PRs weekly`,
        },
      ],
    })
    expect(`repo` in lastStartBody()).toBe(false)
  })

  it(`enforces the builtin's required description`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`])
    const error = await rejectionOf(
      caller.startSession({
        actionId: BUILTIN_ID,
        teamId: `55555555-5555-4555-8555-555555555555`,
        deviceId: `dev-1`,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(
      `Missing required input "description"`
    )
  })

  it(`builtin starts require the action-inputs cap`, async () => {
    actionsCapableDevice([`actions`])
    const error = await rejectionOf(
      caller.startSession({
        actionId: BUILTIN_ID,
        teamId: `55555555-5555-4555-8555-555555555555`,
        deviceId: `dev-1`,
        inputs: { description: `x` },
      })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`action inputs`)
  })

  it(`rejects inputs riding a non-action start`, async () => {
    const error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        inputs: { description: `x` },
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
  })
})

// ── The hidden chat builtin (EXP-615) ────────────────────────────────────────

const CHAT_ID = `builtin:chat`

describe(`steer.startSession — builtin chat (EXP-615)`, () => {
  it(`routes a chat start with the override-aware repo group`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`, `chat`])
    // resolveActionInputs' repo resolver, then the chat repo-group lookup.
    h.dbQueue.push([{ teamId: BUILTIN_TEAM_ID, fullName: `acme/api` }])
    h.dbQueue.push([
      {
        id: REPO_INPUT_ID,
        fullName: `acme/api`,
        defaultBranch: `main`,
        defaultBranchOverride: `develop`,
      },
    ])
    await caller.startSession({
      actionId: CHAT_ID,
      teamId: BUILTIN_TEAM_ID,
      deviceId: `dev-1`,
      inputs: { prompt: `Fix the flaky retry test`, repo: REPO_INPUT_ID },
    })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, BUILTIN_TEAM_ID)
    expect(lastStartBody()).toMatchObject({
      actionId: CHAT_ID,
      actionName: `Chat`,
      teamId: BUILTIN_TEAM_ID,
      repo: {
        repositoryId: REPO_INPUT_ID,
        fullName: `acme/api`,
        defaultBranch: `develop`,
      },
    })
    expect(lastStartBody().inputs).toEqual([
      {
        key: `prompt`,
        label: `Prompt`,
        type: `textarea`,
        value: `Fix the flaky retry test`,
        display: `Fix the flaky retry test`,
      },
      {
        key: `repo`,
        label: `Repository`,
        type: `repo`,
        value: REPO_INPUT_ID,
        display: `acme/api`,
      },
    ])
  })

  it(`enforces the required prompt and repo inputs`, async () => {
    actionsCapableDevice([`actions`, `action-inputs`, `chat`])
    const error = await rejectionOf(
      caller.startSession({
        actionId: CHAT_ID,
        teamId: BUILTIN_TEAM_ID,
        deviceId: `dev-1`,
        inputs: { prompt: `hello` },
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(
      `Missing required input "repo"`
    )
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`chat starts require the chat cap`, async () => {
    // An EXP-257-era desktop advertises actions + action-inputs but has no
    // chat launch path — the dedicated cap must block it.
    actionsCapableDevice([`actions`, `action-inputs`])
    h.dbQueue.push([{ teamId: BUILTIN_TEAM_ID, fullName: `acme/api` }])
    h.dbQueue.push([
      {
        id: REPO_INPUT_ID,
        fullName: `acme/api`,
        defaultBranch: `main`,
        defaultBranchOverride: null,
      },
    ])
    const error = await rejectionOf(
      caller.startSession({
        actionId: CHAT_ID,
        teamId: BUILTIN_TEAM_ID,
        deviceId: `dev-1`,
        inputs: { prompt: `hello`, repo: REPO_INPUT_ID },
      })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`can't run chat`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })
})

// ── Shared server devices (EXP-432) ──────────────────────────────────────────
// A deviceId the caller doesn't own resolves through the `devices` table: a
// SERVER device shared with the subject's team routes to its OWNER's presence
// bucket, and the start carries `startedBy` so the hosting daemon can attribute
// the session row back to the requester. Anything else keeps the pre-EXP-432
// lenient shape (post with the caller's own userId; the relay 404s).

const SHARED_DEVICE = `srv-1`

// The caller has nothing online; `owner-1` hosts the shared server device.
function ownerHostsSharedDevice(caps?: string[], agents?: string[]) {
  h.relayGetDevices.mockImplementation(
    async (_config: unknown, forUserId: string) =>
      forUserId === `owner-1`
        ? {
            devices: [
              {
                deviceId: SHARED_DEVICE,
                deviceLabel: `build-box`,
                connectedAt: 0,
                agents: agents ?? [`claude`, `codex`, `pi`],
                ...(caps ? { caps } : {}),
              },
            ],
          }
        : { devices: [] }
  )
}

// The row the devices lookup finds. It carries the scoping columns so the
// harness's where-filter can reject a row the real query would not match.
function sharedDeviceRow(overrides: Record<string, unknown> = {}) {
  return {
    userId: `owner-1`,
    deviceId: SHARED_DEVICE,
    sharedTeamId: `ws-1`,
    kind: `server`,
    ...overrides,
  }
}

describe(`steer.startSession — shared devices (EXP-432)`, () => {
  it(`routes an issue start to the sharing owner's bucket with startedBy`, async () => {
    ownerHostsSharedDevice()
    h.dbQueue.push([sharedDeviceRow()])

    await caller.startSession({ issueId: ISSUE_A, deviceId: SHARED_DEVICE })

    // Presence was read for the caller first, then for the resolved owner.
    expect(h.relayGetDevices.mock.calls.map((c) => c[1])).toEqual([
      `actor`,
      `owner-1`,
    ])
    expect(lastStartBody()).toMatchObject({
      userId: `owner-1`,
      startedBy: `actor`,
      deviceId: SHARED_DEVICE,
      issueId: ISSUE_A,
    })
  })

  it(`routes a batch start to the sharing owner's bucket with startedBy`, async () => {
    ownerHostsSharedDevice()
    h.dbQueue.push([sharedDeviceRow()])

    await caller.startSession({
      issueIds: [ISSUE_A, ISSUE_B],
      deviceId: SHARED_DEVICE,
    })

    expect(lastStartBody()).toMatchObject({
      userId: `owner-1`,
      startedBy: `actor`,
      issueIds: [ISSUE_A, ISSUE_B],
      teamId: `ws-1`,
    })
  })

  it(`gates the agent list against the OWNER's presence, not the caller's`, async () => {
    ownerHostsSharedDevice(undefined, [`claude`])
    h.dbQueue.push([sharedDeviceRow()])

    const error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: SHARED_DEVICE,
        agent: `codex`,
      })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`codex is not installed`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`omits startedBy entirely on an own-device start`, async () => {
    await caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })

    const body = lastStartBody()
    expect(body.userId).toBe(`actor`)
    expect(`startedBy` in body).toBe(false)
    // The caller's own presence answered — no devices lookup, no second
    // relay round-trip.
    expect(h.relayGetDevices).toHaveBeenCalledTimes(1)
  })

  it(`keeps the lenient path for a device nobody shared (relay 404 ⇒ PRECONDITION_FAILED)`, async () => {
    h.relayGetDevices.mockResolvedValue({ devices: [] })
    h.relayPostStart.mockResolvedValue({
      ok: false,
      status: 404,
      reason: `device_offline`,
    })

    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `ghost` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toBe(`device_offline`)
    const body = lastStartBody()
    expect(body.userId).toBe(`actor`)
    expect(`startedBy` in body).toBe(false)
  })

  it(`does NOT resolve a device shared with a DIFFERENT team`, async () => {
    ownerHostsSharedDevice()
    // The issue lives in ws-1; the share targets another team, so the
    // team-scoped lookup matches nothing.
    h.dbQueue.push([sharedDeviceRow({ sharedTeamId: `ws-OTHER` })])

    await caller.startSession({ issueId: ISSUE_A, deviceId: SHARED_DEVICE })

    const body = lastStartBody()
    expect(body.userId).toBe(`actor`)
    expect(`startedBy` in body).toBe(false)
    // The owner's presence was never fetched.
    expect(h.relayGetDevices.mock.calls.map((c) => c[1])).toEqual([`actor`])
  })

  it(`does NOT resolve a non-server (desktop) device row`, async () => {
    ownerHostsSharedDevice()
    h.dbQueue.push([sharedDeviceRow({ kind: `desktop` })])

    await caller.startSession({ issueId: ISSUE_A, deviceId: SHARED_DEVICE })

    const body = lastStartBody()
    expect(body.userId).toBe(`actor`)
    expect(`startedBy` in body).toBe(false)
  })

  it(`never marks the caller's OWN shared device as a shared start`, async () => {
    // The caller shared their own server device with the team but its daemon
    // is offline — the row resolves to themselves, so no startedBy rides.
    h.relayGetDevices.mockResolvedValue({ devices: [] })
    h.dbQueue.push([sharedDeviceRow({ userId: `actor` })])

    await caller.startSession({ issueId: ISSUE_A, deviceId: SHARED_DEVICE })

    const body = lastStartBody()
    expect(body.userId).toBe(`actor`)
    expect(`startedBy` in body).toBe(false)
    expect(h.relayGetDevices).toHaveBeenCalledTimes(1)
  })

  it(`routes an action start to the owner's bucket with startedBy`, async () => {
    ownerHostsSharedDevice([`actions`, `action-inputs`])
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: [] },
    ])
    h.dbQueue.push([sharedDeviceRow()])

    await caller.startSession({ actionId: ACTION_ID, deviceId: SHARED_DEVICE })

    expect(lastStartBody()).toMatchObject({
      userId: `owner-1`,
      startedBy: `actor`,
      actionId: ACTION_ID,
      teamId: `ws-1`,
    })
  })

  it(`checks the action caps against the OWNER's presence`, async () => {
    // The owner's daemon is old: it advertises no caps at all.
    ownerHostsSharedDevice()
    h.dbQueue.push([
      { id: ACTION_ID, teamId: `ws-1`, repositoryId: null, name: `A`, inputs: [] },
    ])
    h.dbQueue.push([sharedDeviceRow()])

    const error = await rejectionOf(
      caller.startSession({ actionId: ACTION_ID, deviceId: SHARED_DEVICE })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`can't run actions`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`requires action-inputs on the owner's device for a builtin start`, async () => {
    ownerHostsSharedDevice([`actions`])
    h.dbQueue.push([sharedDeviceRow({ sharedTeamId: BUILTIN_TEAM_ID })])

    const error = await rejectionOf(
      caller.startSession({
        actionId: BUILTIN_ID,
        teamId: BUILTIN_TEAM_ID,
        deviceId: SHARED_DEVICE,
        inputs: { description: `x` },
      })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`action inputs`)
  })

  it(`routes a builtin start on a shared device with startedBy`, async () => {
    ownerHostsSharedDevice([`actions`, `action-inputs`])
    h.dbQueue.push([sharedDeviceRow({ sharedTeamId: BUILTIN_TEAM_ID })])

    await caller.startSession({
      actionId: BUILTIN_ID,
      teamId: BUILTIN_TEAM_ID,
      deviceId: SHARED_DEVICE,
      inputs: { description: `Review PRs weekly` },
    })

    expect(lastStartBody()).toMatchObject({
      userId: `owner-1`,
      startedBy: `actor`,
      actionId: BUILTIN_ID,
      teamId: BUILTIN_TEAM_ID,
    })
  })
})

// ── Host-account allowances on live-session controls (EXP-432) ───────────────
// A shared-device session is requester-OWNED (userId) and host-OPERATED
// (hostUserId). Both principals may publish/view/kill it; nobody else may.

const SESSION_ID = `66666666-6666-4666-8666-666666666666`

function queueSession(overrides: Record<string, unknown> = {}) {
  h.dbQueue.push([
    {
      id: SESSION_ID,
      userId: `requester`,
      hostUserId: `actor`,
      teamId: `ws-1`,
      status: `running`,
      ...overrides,
    },
  ])
}

describe(`steer.mintTicket — owner OR host (EXP-432)`, () => {
  it(`lets the hosting account publish a requester-owned session`, async () => {
    queueSession()
    await caller.mintTicket({ kind: `publisher`, codingSessionId: SESSION_ID })
    expect(h.mintSteerTicket).toHaveBeenCalledWith(expect.anything(), {
      kind: `publisher`,
      userId: `actor`,
      teamId: `ws-1`,
      sessionId: SESSION_ID,
    })
  })

  it(`lets the hosting account view a requester-owned session`, async () => {
    queueSession()
    await caller.mintTicket({ kind: `viewer`, codingSessionId: SESSION_ID })
    expect(h.mintSteerTicket).toHaveBeenCalledWith(expect.anything(), {
      kind: `viewer`,
      userId: `actor`,
      teamId: `ws-1`,
      sessionId: SESSION_ID,
    })
  })

  it(`refuses a third party as publisher AND as viewer`, async () => {
    queueSession({ hostUserId: `someone-else` })
    let error = await rejectionOf(
      caller.mintTicket({ kind: `publisher`, codingSessionId: SESSION_ID })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)

    queueSession({ hostUserId: `someone-else` })
    error = await rejectionOf(
      caller.mintTicket({ kind: `viewer`, codingSessionId: SESSION_ID })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(h.mintSteerTicket).not.toHaveBeenCalled()
  })

  it(`still mints for the session owner when there is no host`, async () => {
    queueSession({ userId: `actor`, hostUserId: null })
    await caller.mintTicket({ kind: `viewer`, codingSessionId: SESSION_ID })
    expect(h.mintSteerTicket).toHaveBeenCalledTimes(1)
    expect(h.assertTeamMember).not.toHaveBeenCalled()
  })

  it(`re-checks team membership for a requester on someone else's host`, async () => {
    queueSession({ userId: `actor`, hostUserId: `host-1` })
    await caller.mintTicket({ kind: `viewer`, codingSessionId: SESSION_ID })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, `ws-1`)
    expect(h.mintSteerTicket).toHaveBeenCalledTimes(1)
  })

  it(`refuses a requester who left the team on someone else's host`, async () => {
    queueSession({ userId: `actor`, hostUserId: `host-1` })
    h.assertTeamMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN`, message: `Not a member` })
    )
    const error = await rejectionOf(
      caller.mintTicket({ kind: `viewer`, codingSessionId: SESSION_ID })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(h.mintSteerTicket).not.toHaveBeenCalled()
  })
})

describe(`steer.killSession — owner OR host (EXP-432)`, () => {
  it(`lets the hosting account kill what runs on its own machine`, async () => {
    // An already-ended row short-circuits the transaction write, so the
    // authorization branch is what this exercises.
    queueSession({ status: `ended` })
    const result = await caller.killSession({ codingSessionId: SESSION_ID })
    expect(result.session).toMatchObject({ id: SESSION_ID })
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), SESSION_ID)
  })

  it(`refuses a user who is neither owner nor host`, async () => {
    queueSession({ hostUserId: `someone-else`, status: `ended` })
    const error = await rejectionOf(
      caller.killSession({ codingSessionId: SESSION_ID })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })
})

// EXP-481: remote resume — single-issue only, strictly gated on the
// persisted row's `resume` cap.
describe(`steer.startSession — resume (EXP-481)`, () => {
  const resumeRow = (caps: string[]) => [
    { userId: `actor`, deviceId: `dev-1`, caps },
  ]

  it(`rejects resume on a batch start at the input layer`, async () => {
    const error = await rejectionOf(
      caller.startSession({
        issueIds: [ISSUE_A, ISSUE_B],
        deviceId: `dev-1`,
        resume: true,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`rejects resume on an action start at the input layer`, async () => {
    const error = await rejectionOf(
      caller.startSession({
        actionId: `33333333-3333-4333-8333-333333333333`,
        deviceId: `dev-1`,
        resume: true,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`refuses when the persisted row lacks the resume cap`, async () => {
    h.dbQueue.push(resumeRow([`actions`]))
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1`, resume: true })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`can't resume`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`refuses when the device never registered (legacy desktop)`, async () => {
    h.dbQueue.push([])
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1`, resume: true })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
  })

  it(`rides the relay body when the cap is present`, async () => {
    h.dbQueue.push(resumeRow([`actions`, `resume`, `worktrees`]))
    const result = await caller.startSession({
      issueId: ISSUE_A,
      deviceId: `dev-1`,
      resume: true,
    })
    expect(result).toEqual({ ok: true })
    expect(lastStartBody()).toMatchObject({ issueId: ISSUE_A, resume: true })
  })

  it(`stays off the wire when not requested`, async () => {
    await caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })
    // Undefined option fields are dropped by JSON.stringify in relayPostStart
    // — same stance as model/effort.
    expect(lastStartBody().resume).toBeUndefined()
  })
})
