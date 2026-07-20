import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// steer.startSession accepts EITHER a single issueId (wire-unchanged) or
// issueIds (2..30 → batch). It resolves every issue's team + repo
// server-side, enforces one-team / one-repo, and routes a legacy body
// for a single (or duplicate-collapsed) id vs a "fat" batch body (issueIds +
// teamId + repo, installationId stripped) for 2+. The relay call is
// mocked, so a caller + a handful of stubs is enough.

const h = vi.hoisted(() => ({
  getSteerRelayConfig: vi.fn(),
  relayPostStart: vi.fn(),
  relayGetDevices: vi.fn(),
  assertTeamMember: vi.fn(),
  getIssueTeamContext: vi.fn(),
  resolveBoardRepository: vi.fn(),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
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
