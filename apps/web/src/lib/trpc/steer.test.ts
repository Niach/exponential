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
  const dbQueue: unknown[][] = []
  const makeChain = () => {
    const chain = {
      from: () => chain,
      where: () => chain,
      limit: () => Promise.resolve(dbQueue.shift() ?? []),
    }
    return chain
  }
  return {
    getSteerRelayConfig: vi.fn(),
    relayPostStart: vi.fn(),
    relayGetDevices: vi.fn(),
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
}))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayPostStart: h.relayPostStart,
  relayGetDevices: h.relayGetDevices,
  // Referenced (not called) by sibling procedures we never invoke here.
  mintSteerTicket: vi.fn(),
  relayPostKill: vi.fn(),
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

    error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        agent: `pi`,
        planMode: true,
      })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)

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
