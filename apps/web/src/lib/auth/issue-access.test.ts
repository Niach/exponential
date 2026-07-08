import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the v7 issue mutation matrix: membership is the only capability gate
// (every membership is an explicit invite — the public-workspace self-service
// join and its moderator clamp are gone). Anonymous feedback-board viewers
// never reach these predicates: they have no session, and writes arrive only
// via the widget's server-side service.

const h = vi.hoisted(() => {
  const state = {
    member: undefined as
      | { role: string; userId: string; workspaceId: string }
      | undefined,
  }
  return { state }
})

vi.mock(`@/lib/auth/membership`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/auth/membership")>()
  return {
    ...actual,
    getIssueWorkspaceContext: vi.fn(async () => ({
      issueId: `issue-1`,
      projectId: `proj-1`,
      workspaceId: `ws-1`,
    })),
    getWorkspaceById: vi.fn(async () => ({ id: `ws-1` })),
    getWorkspaceMember: vi.fn(async () => h.state.member),
  }
})

import { assertIssueAccess } from "@/lib/auth/access"

describe(`assertIssueAccess (v7 membership-only)`, () => {
  beforeEach(() => {
    h.state.member = undefined
  })

  it(`member may write`, async () => {
    h.state.member = { role: `member`, userId: `user-1`, workspaceId: `ws-1` }
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `write`)
    ).resolves.toBeTruthy()
  })

  it(`owner may delete`, async () => {
    h.state.member = { role: `owner`, userId: `user-1`, workspaceId: `ws-1` }
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `delete`)
    ).resolves.toBeTruthy()
  })

  it(`non-member is rejected for write`, async () => {
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `write`)
    ).rejects.toThrow(`Not a member`)
  })

  it(`non-member is rejected for read`, async () => {
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `read`)
    ).rejects.toThrow(`Not a member`)
  })

  it(`member may read`, async () => {
    h.state.member = { role: `member`, userId: `user-1`, workspaceId: `ws-1` }
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `read`)
    ).resolves.toBeTruthy()
  })
})
