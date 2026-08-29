import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// steer.startSession accepts EITHER a single issueId (wire-unchanged) or
// issueIds (2..30 → batch). It resolves every issue's team + repo
// server-side, enforces one-team / one-repo, and routes a legacy body
// for a single (or duplicate-collapsed) id vs a "fat" batch body (issueIds +
// teamId + repo, installationId stripped) for 2+. The relay call is
// mocked, so a caller + a handful of stubs is enough.
//
// EXP-485/EXP-542: the TARGET MACHINE is resolved purely from the persisted
// `devices` row — startSession never reads relay presence any more (the
// online frame carries no agents/caps advertisement, and online-ness stays
// with relayPostStart's 404). So every scenario below queues the row(s)
// resolveTargetDevice selects; see queueOwnDevice / queueSharedDevice.

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
      orderBy: () => chain,
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

// resolveTargetDevice runs the caller's OWN (user_id, device_id) select
// first and only falls through to the shared-server select when it misses,
// so a scenario queues its rows in that order. A row is the whole
// advertisement now: agents, unauthedAgents and caps are notNull columns.
// Queue NOTHING to model a machine that never registered (EXP-542).
function ownDeviceRow(over: Record<string, unknown> = {}) {
  return {
    userId: `actor`,
    deviceId: `dev-1`,
    agents: [`claude`, `codex`, `pi`],
    unauthedAgents: [],
    caps: [],
    ...over,
  }
}

function queueOwnDevice(over: Record<string, unknown> = {}): void {
  h.dbQueue.push([ownDeviceRow(over)])
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
    // The registered row says nothing about online-ness — an offline machine
    // is exactly this: a resolvable row whose relay bucket 404s.
    queueOwnDevice()
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
    queueOwnDevice()
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
    queueOwnDevice()
    await caller.startSession({ issueIds: [ISSUE_A], deviceId: `dev-1` })
    const body = lastStartBody()
    expect(body.issueId).toBe(ISSUE_A)
    expect(`issueIds` in body).toBe(false)
  })

  it(`collapses duplicate issueIds to one → legacy single-issue body`, async () => {
    queueOwnDevice()
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
    queueOwnDevice()
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
    queueOwnDevice()
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

  it(`rejects an agent the row did not register`, async () => {
    queueOwnDevice({ agents: [`claude`] })
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1`, agent: `codex` })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`codex is not installed`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`names a signed-out agent instead of "not installed" (EXP-409)`, async () => {
    // Installed but signed out: the runnable list is empty (no claude
    // fallback since EXP-542) and the message says where to fix it.
    queueOwnDevice({ agents: [], unauthedAgents: [`claude`] })
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(
      `claude is installed on that device but not signed in`
    )
    expect(h.relayPostStart).not.toHaveBeenCalled()
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

    // Blank model is the codex/pi "CLI default" — valid. (Only this call
    // reaches the device resolve; the two above fail at the input layer.)
    queueOwnDevice()
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

    queueOwnDevice()
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

// The action row the action branch loads FIRST — the device row it resolves
// last always goes on the queue after it.
function queueAction(over: Record<string, unknown> = {}) {
  h.dbQueue.push([
    {
      id: ACTION_ID,
      teamId: `ws-1`,
      repositoryId: null,
      name: `A`,
      inputs: [],
      ...over,
    },
  ])
}

describe(`steer.startSession — action runs (EXP-257)`, () => {
  it(`forwards the FULL option set on an action start (no more Claude-only clamp)`, async () => {
    queueAction({ name: `Code review` })
    queueOwnDevice({ caps: [`actions`, `action-inputs`] })
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

  it(`rejects an agent the row did not register on action starts`, async () => {
    queueAction()
    queueOwnDevice({ caps: [`actions`, `action-inputs`], agents: [`claude`] })
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
    queueAction()
    queueOwnDevice({
      agents: [],
      unauthedAgents: [`claude`],
      caps: [`actions`, `action-inputs`],
    })
    const error = await rejectionOf(
      caller.startSession({ actionId: ACTION_ID, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(
      `claude is installed on that device but not signed in`
    )
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  // EXP-639: the `actions` / `action-inputs` cap refusals are gone — every
  // build above the version floor advertises them whenever it advertises a
  // runnable agent, so the agent list is the only gate on this path.
  it(`starts an action on a row advertising no caps at all`, async () => {
    queueAction()
    queueOwnDevice()
    await caller.startSession({ actionId: ACTION_ID, deviceId: `dev-1` })
    expect(h.relayPostStart).toHaveBeenCalledTimes(1)
  })

  it(`honors a non-claude agent without any cap, but only if the row runs it`, async () => {
    queueAction()
    queueOwnDevice({ agents: [`claude`, `codex`] })
    await caller.startSession({
      actionId: ACTION_ID,
      deviceId: `dev-1`,
      agent: `codex`,
      model: `gpt-5.6-sol`,
    })
    expect(lastStartBody()).toMatchObject({ agent: `codex` })

    queueAction()
    queueOwnDevice({ agents: [`claude`] })
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

  it(`carries inputs to a row advertising no caps`, async () => {
    queueAction({
      inputs: [{ key: `topic`, label: `Topic`, type: `text`, required: false }],
    })
    queueOwnDevice()
    await caller.startSession({
      actionId: ACTION_ID,
      deviceId: `dev-1`,
      inputs: { topic: `perf` },
    })
    expect(lastStartBody()).toMatchObject({
      inputs: [{ key: `topic`, value: `perf` }],
    })
  })

  it(`validates values against the schema — missing required, unknown key`, async () => {
    // Both calls fail before the device resolve, so no row is queued.
    const defs = [{ key: `topic`, label: `Topic`, type: `text`, required: true }]
    queueAction({ inputs: defs })
    let error = await rejectionOf(
      caller.startSession({ actionId: ACTION_ID, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`Missing required input`)

    queueAction({ inputs: defs })
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
    queueAction({
      inputs: [{ key: `repo`, label: `Repository`, type: `repo`, required: true }],
    })
    // The repo lookup select, then the device resolve.
    h.dbQueue.push([{ teamId: `ws-1`, fullName: `acme/api` }])
    queueOwnDevice({ caps: [`actions`, `action-inputs`] })
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
    queueAction({
      inputs: [{ key: `repo`, label: `Repository`, type: `repo`, required: true }],
    })
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
    // The device row is the ONLY queued select — the builtin never loads a
    // DB action.
    queueOwnDevice({ caps: [`actions`, `action-inputs`] })
    const teamId = `55555555-5555-4555-8555-555555555555`
    await caller.startSession({
      actionId: BUILTIN_ID,
      teamId,
      deviceId: `dev-1`,
      inputs: { description: `Review PRs weekly` },
    })
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

  it(`builtin starts need no caps on the row (EXP-639)`, async () => {
    queueOwnDevice()
    await caller.startSession({
      actionId: BUILTIN_ID,
      teamId: `55555555-5555-4555-8555-555555555555`,
      deviceId: `dev-1`,
      inputs: { description: `x` },
    })
    expect(h.relayPostStart).toHaveBeenCalledTimes(1)
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
    // resolveActionInputs' repo resolver, the chat repo-group lookup, then
    // the device resolve.
    h.dbQueue.push([{ teamId: BUILTIN_TEAM_ID, fullName: `acme/api` }])
    h.dbQueue.push([
      {
        id: REPO_INPUT_ID,
        fullName: `acme/api`,
        defaultBranch: `main`,
        defaultBranchOverride: `develop`,
      },
    ])
    queueOwnDevice({ caps: [`actions`, `action-inputs`, `chat`] })
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

  it(`starts without the chat cap — actions + action-inputs is enough (EXP-624)`, async () => {
    // The per-start `chat` gate is gone: the fleet floor advertising it is
    // enforced by CLIENT_MIN_VERSION_DESKTOP instead.
    h.dbQueue.push([{ teamId: BUILTIN_TEAM_ID, fullName: `acme/api` }])
    h.dbQueue.push([
      {
        id: REPO_INPUT_ID,
        fullName: `acme/api`,
        defaultBranch: `main`,
        defaultBranchOverride: null,
      },
    ])
    queueOwnDevice({ caps: [`actions`, `action-inputs`] })
    const result = await caller.startSession({
      actionId: CHAT_ID,
      teamId: BUILTIN_TEAM_ID,
      deviceId: `dev-1`,
      inputs: { prompt: `hello`, repo: REPO_INPUT_ID },
    })
    expect(result).toEqual({ ok: true })
    expect(lastStartBody()).toMatchObject({
      actionId: CHAT_ID,
      repo: { fullName: `acme/api`, defaultBranch: `main` },
    })
  })
})

// ── Shared server devices (EXP-432) ──────────────────────────────────────────
// A deviceId the caller doesn't own resolves through the `devices` table: a
// registered SERVER device shared with the subject's team routes to its
// OWNER's relay bucket, and the start carries `startedBy` so the hosting
// daemon can attribute the session row back to the requester. Anything the
// team-scoped lookup does not match refuses outright (EXP-542) — the
// pre-EXP-542 lenient "post it with the caller's own userId" path is gone.

const SHARED_DEVICE = `srv-1`

// The row the shared lookup finds. It carries the scoping columns so the
// harness's where-filter can reject a row the real query would not match,
// plus the advertisement the start is gated on.
function sharedDeviceRow(overrides: Record<string, unknown> = {}) {
  return {
    userId: `owner-1`,
    deviceId: SHARED_DEVICE,
    sharedTeamId: `ws-1`,
    kind: `server`,
    agents: [`claude`, `codex`, `pi`],
    unauthedAgents: [],
    caps: [],
    ...overrides,
  }
}

// The caller owns nothing under this deviceId, so the own-row select comes
// back empty and the shared-row select answers.
function queueSharedDevice(overrides: Record<string, unknown> = {}): void {
  h.dbQueue.push([])
  h.dbQueue.push([sharedDeviceRow(overrides)])
}

describe(`steer.startSession — shared devices (EXP-432)`, () => {
  it(`routes an issue start to the sharing owner's bucket with startedBy`, async () => {
    queueSharedDevice()

    await caller.startSession({ issueId: ISSUE_A, deviceId: SHARED_DEVICE })

    // EXP-485: resolution is pure DB — no presence round-trip at all.
    expect(lastStartBody()).toMatchObject({
      userId: `owner-1`,
      startedBy: `actor`,
      deviceId: SHARED_DEVICE,
      issueId: ISSUE_A,
    })
  })

  it(`routes a batch start to the sharing owner's bucket with startedBy`, async () => {
    queueSharedDevice()

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

  it(`gates the agent list against the OWNER's row, not the caller's`, async () => {
    queueSharedDevice({ agents: [`claude`] })

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
    queueOwnDevice()

    await caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })

    const body = lastStartBody()
    expect(body.userId).toBe(`actor`)
    expect(`startedBy` in body).toBe(false)
  })

  it(`refuses a deviceId nobody registered (EXP-542)`, async () => {
    // Nothing queued: both selects come back empty. Every supported build
    // registers at startup, so an unresolvable target is refused here
    // instead of being posted at the caller's own relay bucket.
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `ghost` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`hasn't registered`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`does NOT resolve a device shared with a DIFFERENT team`, async () => {
    // The issue lives in ws-1; the share targets another team, so the
    // team-scoped lookup matches nothing and the start refuses.
    queueSharedDevice({ sharedTeamId: `ws-OTHER` })

    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: SHARED_DEVICE })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`hasn't registered`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`does NOT resolve a non-server (desktop) device row`, async () => {
    queueSharedDevice({ kind: `desktop` })

    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: SHARED_DEVICE })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`hasn't registered`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`never marks the caller's OWN shared device as a shared start`, async () => {
    // The caller shared their own server device with the team — the own-row
    // select wins, so no startedBy rides. The shared row queued behind it is
    // left unconsumed: that second select never runs.
    queueOwnDevice({ deviceId: SHARED_DEVICE, sharedTeamId: `ws-1` })
    h.dbQueue.push([sharedDeviceRow()])

    await caller.startSession({ issueId: ISSUE_A, deviceId: SHARED_DEVICE })

    const body = lastStartBody()
    expect(body.userId).toBe(`actor`)
    expect(`startedBy` in body).toBe(false)
    expect(h.dbQueue).toHaveLength(1)
  })

  it(`routes an action start to the owner's bucket with startedBy`, async () => {
    queueAction()
    queueSharedDevice({ caps: [`actions`, `action-inputs`] })

    await caller.startSession({ actionId: ACTION_ID, deviceId: SHARED_DEVICE })

    expect(lastStartBody()).toMatchObject({
      userId: `owner-1`,
      startedBy: `actor`,
      actionId: ACTION_ID,
      teamId: `ws-1`,
    })
  })

  it(`checks the action agent against the OWNER's row`, async () => {
    // The owner's box runs claude only — a codex action start refuses there
    // even though the caller's own machine could run it.
    queueAction()
    queueSharedDevice({ agents: [`claude`] })

    const error = await rejectionOf(
      caller.startSession({
        actionId: ACTION_ID,
        deviceId: SHARED_DEVICE,
        agent: `codex`,
      })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`codex is not installed`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`routes a builtin start on a shared device with startedBy`, async () => {
    // EXP-639: a capless owner row is fine — only the agent list gates.
    queueSharedDevice({ sharedTeamId: BUILTIN_TEAM_ID })

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

// EXP-481: remote resume — single-issue only. EXP-639 dropped the `resume`
// cap refusal: every build above the version floor honors the flag.
describe(`steer.startSession — resume (EXP-481)`, () => {
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

  it(`refuses when the device never registered (legacy desktop)`, async () => {
    const error = await rejectionOf(
      caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1`, resume: true })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`hasn't registered`)
  })

  it(`rides the relay body when requested`, async () => {
    queueOwnDevice()
    const result = await caller.startSession({
      issueId: ISSUE_A,
      deviceId: `dev-1`,
      resume: true,
    })
    expect(result).toEqual({ ok: true })
    expect(lastStartBody()).toMatchObject({ issueId: ISSUE_A, resume: true })
  })

  it(`stays off the wire when not requested`, async () => {
    queueOwnDevice()
    await caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })
    // Undefined option fields are dropped by JSON.stringify in relayPostStart
    // — same stance as model/effort.
    expect(lastStartBody().resume).toBeUndefined()
  })
})

// EXP-679: `parentSessionId` — this start was asked for by another coding
// session (the MCP `exponential_sessions_start` tool passes its own session
// id). Its only wire effect is `startedReason: 'agent'`, which the device
// writes onto the new row so the run is unattended; the parent linkage itself
// is stamped server-side by the MCP tool. Gated on the parent being one of the
// caller's OWN live sessions.
describe(`steer.startSession — agent-started runs (EXP-679)`, () => {
  const PARENT = `88888888-8888-4888-8888-888888888888`

  function queueParent(over: Record<string, unknown> = {}): void {
    h.dbQueue.push([
      { id: PARENT, userId: `actor`, hostUserId: null, status: `running`, ...over },
    ])
  }

  it(`brands an issue start as agent-driven`, async () => {
    queueParent()
    queueOwnDevice({ caps: [`agent-start`] })

    await caller.startSession({
      issueId: ISSUE_A,
      deviceId: `dev-1`,
      parentSessionId: PARENT,
    })

    expect(lastStartBody()).toMatchObject({
      issueId: ISSUE_A,
      startedReason: `agent`,
    })
  })

  it(`brands a batch start too`, async () => {
    queueParent({ status: `in_review`, userId: `owner`, hostUserId: `actor` })
    queueOwnDevice({ caps: [`agent-start`] })

    await caller.startSession({
      issueIds: [ISSUE_A, ISSUE_B],
      deviceId: `dev-1`,
      parentSessionId: PARENT,
    })

    expect(lastStartBody()).toMatchObject({
      issueIds: [ISSUE_A, ISSUE_B],
      startedReason: `agent`,
    })
  })

  it(`brands a resume too`, async () => {
    queueParent()
    h.dbQueue.push([
      {
        id: uuid(7),
        userId: `actor`,
        hostUserId: null,
        teamId: `ws-1`,
        status: `ended`,
        deviceId: `dev-1`,
        issueId: null,
        actionId: null,
        actionName: `Refresh screenshots`,
        branch: null,
      },
    ])
    queueOwnDevice({ caps: [`resume-run`, `agent-start`] })

    await caller.startSession({
      resumeSessionId: uuid(7),
      deviceId: `dev-1`,
      parentSessionId: PARENT,
    })

    expect(lastStartBody()).toMatchObject({
      resumeSessionId: uuid(7),
      startedReason: `agent`,
    })
  })

  it(`refuses a parent that is not the caller's own live session`, async () => {
    for (const over of [
      { userId: `someone-else`, hostUserId: null },
      { status: `ended` },
    ]) {
      h.dbQueue.length = 0
      queueParent(over)
      queueOwnDevice()
      const error = await rejectionOf(
        caller.startSession({
          issueId: ISSUE_A,
          deviceId: `dev-1`,
          parentSessionId: PARENT,
        })
      )
      expect((error as TRPCError).code, JSON.stringify(over)).toBe(`FORBIDDEN`)
    }
    // A parent that no longer exists is the same refusal.
    h.dbQueue.length = 0
    queueOwnDevice()
    const gone = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        parentSessionId: PARENT,
      })
    )
    expect((gone as TRPCError).code).toBe(`FORBIDDEN`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  // A host that doesn't know the brand drops `startedReason` off the frame
  // and writes an ATTENDED run: it would never report and never end while
  // the parent polls it forever. Refuse instead of starting it.
  it(`refuses a device without the agent-start cap`, async () => {
    queueParent()
    queueOwnDevice({ caps: [`resume-run`] })

    const error = await rejectionOf(
      caller.startSession({
        issueId: ISSUE_A,
        deviceId: `dev-1`,
        parentSessionId: PARENT,
      })
    )

    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toBe(
      `That machine's app is too old to run an agent-started session. Update it and try again.`
    )
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`stays off the wire on a plain human start`, async () => {
    queueOwnDevice()
    await caller.startSession({ issueId: ISSUE_A, deviceId: `dev-1` })
    expect(`startedReason` in lastStartBody()).toBe(false)
  })
})

// EXP-637: resuming an ENDED run. A subject of its own — the device's run
// registry already holds the agent, options and cwd, so naming any of them
// here would just contradict it. Owner-only (a live session is owner-only
// since EXP-312, and a resume makes one live again) and pinned to the machine
// that holds the worktree.
describe(`steer.startSession — resume a run (EXP-637)`, () => {
  const RESUME = `77777777-7777-4777-8777-777777777777`

  function queueEndedRun(over: Record<string, unknown> = {}): void {
    h.dbQueue.push([
      {
        id: RESUME,
        userId: `actor`,
        hostUserId: null,
        teamId: `ws-1`,
        status: `ended`,
        deviceId: `dev-1`,
        issueId: null,
        actionId: null,
        actionName: `Refresh screenshots`,
        branch: `exp/refresh-screenshots-1a2b3c4d`,
        ...over,
      },
    ])
  }

  it(`rides the relay with the run's own routing hints`, async () => {
    queueEndedRun()
    queueOwnDevice({ caps: [`actions`, `resume-run`] })

    const result = await caller.startSession({
      resumeSessionId: RESUME,
      deviceId: `dev-1`,
    })

    expect(result).toEqual({ ok: true })
    expect(lastStartBody()).toMatchObject({
      resumeSessionId: RESUME,
      teamId: `ws-1`,
      actionName: `Refresh screenshots`,
      branch: `exp/refresh-screenshots-1a2b3c4d`,
    })
    // No launch options ride a resume.
    expect(lastStartBody().agent).toBeUndefined()
    expect(lastStartBody().resume).toBeUndefined()
  })

  it(`refuses to combine a resume with any other subject or option`, async () => {
    for (const extra of [
      { issueId: ISSUE_A },
      { actionId: uuid(3) },
      { agent: `codex` },
      { model: `opus` },
      { planMode: true },
      { resume: true },
      { teamId: `ws-1` },
    ]) {
      const error = await rejectionOf(
        caller.startSession({
          resumeSessionId: RESUME,
          deviceId: `dev-1`,
          ...extra,
        } as never)
      )
      expect((error as TRPCError).code, JSON.stringify(extra)).toBe(
        `BAD_REQUEST`
      )
    }
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`refuses a run that is not the caller's own`, async () => {
    queueEndedRun({ userId: `someone-else`, hostUserId: `actor` })
    const error = await rejectionOf(
      caller.startSession({ resumeSessionId: RESUME, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`refuses a run that is still live`, async () => {
    queueEndedRun({ status: `running` })
    const error = await rejectionOf(
      caller.startSession({ resumeSessionId: RESUME, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`still live`)
  })

  it(`refuses a run whose worktree lives on another machine`, async () => {
    queueEndedRun({ deviceId: `dev-2` })
    const other = await rejectionOf(
      caller.startSession({ resumeSessionId: RESUME, deviceId: `dev-1` })
    )
    expect((other as TRPCError).message).toContain(`another machine`)

    // A pre-EXP-549 row has no device stamp at all — same refusal.
    queueEndedRun({ deviceId: null })
    const unstamped = await rejectionOf(
      caller.startSession({ resumeSessionId: RESUME, deviceId: `dev-1` })
    )
    expect((unstamped as TRPCError).message).toContain(`another machine`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`refuses a device without the resume-run cap`, async () => {
    queueEndedRun()
    queueOwnDevice({ caps: [`actions`, `resume`] })
    const error = await rejectionOf(
      caller.startSession({ resumeSessionId: RESUME, deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toBe(
      `That device can't resume runs yet. Update it.`
    )
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`carries an issue run's issueId instead of the action hints`, async () => {
    queueEndedRun({
      issueId: ISSUE_A,
      actionName: null,
      branch: null,
    })
    queueOwnDevice({ caps: [`resume-run`] })
    // Nothing queued for the live-session probe — the issue is free.

    await caller.startSession({ resumeSessionId: RESUME, deviceId: `dev-1` })

    const body = lastStartBody()
    expect(body).toMatchObject({ resumeSessionId: RESUME, issueId: ISSUE_A })
    expect(body.actionName).toBeUndefined()
    expect(body.branch).toBeUndefined()
  })

  // EXP-662: one session per issue. The desktop would refuse the frame
  // anyway, so the refusal happens here, named, instead of vanishing.
  it(`refuses an issue run whose issue already has a live session`, async () => {
    queueEndedRun({ issueId: ISSUE_A, actionName: null, branch: null })
    queueOwnDevice({ caps: [`resume-run`] })
    h.dbQueue.push([{ id: uuid(9), deviceLabel: `studio` }])

    const error = await rejectionOf(
      caller.startSession({ resumeSessionId: RESUME, deviceId: `dev-1` })
    )

    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toBe(
      `That issue already has a live session on studio`
    )
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })
})
