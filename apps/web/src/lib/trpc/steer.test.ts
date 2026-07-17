import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// steer.startSession accepts EITHER a single issueId (wire-unchanged) or
// issueIds (2..30 → batch). It resolves every issue's workspace + repo
// server-side, enforces one-workspace / one-repo, and routes a legacy body
// for a single (or duplicate-collapsed) id vs a "fat" batch body (issueIds +
// workspaceId + repo, installationId stripped) for 2+. The relay call is
// mocked, so a caller + a handful of stubs is enough.

const h = vi.hoisted(() => ({
  getSteerRelayConfig: vi.fn(),
  relayPostStart: vi.fn(),
  assertWorkspaceMember: vi.fn(),
  getIssueWorkspaceContext: vi.fn(),
  resolveProjectRepository: vi.fn(),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/workspace-membership`, () => ({
  assertWorkspaceMember: h.assertWorkspaceMember,
  getIssueWorkspaceContext: h.getIssueWorkspaceContext,
}))
vi.mock(`@/lib/trpc/repositories`, () => ({
  resolveProjectRepository: h.resolveProjectRepository,
}))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayPostStart: h.relayPostStart,
  // Referenced (not called) by sibling procedures we never invoke here.
  mintSteerTicket: vi.fn(),
  relayGetDevices: vi.fn(),
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
  h.assertWorkspaceMember.mockReset()
  h.assertWorkspaceMember.mockResolvedValue({ role: `member` })
  h.getIssueWorkspaceContext.mockReset()
  h.getIssueWorkspaceContext.mockImplementation(async (id: string) => ({
    issueId: id,
    projectId: `proj-${id}`,
    workspaceId: `ws-1`,
  }))
  h.resolveProjectRepository.mockReset()
  h.resolveProjectRepository.mockResolvedValue({
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
    expect(h.getIssueWorkspaceContext).not.toHaveBeenCalled()
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })
})

describe(`steer.startSession — server-side validation`, () => {
  it(`rejects issues spanning multiple workspaces`, async () => {
    h.getIssueWorkspaceContext.mockImplementation(async (id: string) => ({
      issueId: id,
      projectId: `proj-${id}`,
      workspaceId: id === ISSUE_A ? `ws-1` : `ws-2`,
    }))
    const error = await rejectionOf(
      caller.startSession({ issueIds: [ISSUE_A, ISSUE_B], deviceId: `dev-1` })
    )
    expect((error as TRPCError).code).toBe(`PRECONDITION_FAILED`)
    expect((error as TRPCError).message).toContain(`one workspace`)
    expect(h.relayPostStart).not.toHaveBeenCalled()
  })

  it(`rejects issues spanning multiple repositories, naming both`, async () => {
    h.resolveProjectRepository.mockImplementation(
      async (projectId: string) => ({
        repositoryId: projectId === `proj-${ISSUE_A}` ? `repo-a` : `repo-b`,
        fullName: projectId === `proj-${ISSUE_A}` ? `acme/api` : `acme/web`,
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

  it(`rejects when a project has no linked repository`, async () => {
    h.resolveProjectRepository.mockResolvedValue(null)
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
    expect(`workspaceId` in body).toBe(false)
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
    expect(h.getIssueWorkspaceContext).toHaveBeenCalledTimes(1)
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
      workspaceId: `ws-1`,
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
