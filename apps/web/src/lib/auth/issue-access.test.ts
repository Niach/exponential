import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the public-workspace mutation matrix: membership on a public board is
// an open self-service join, so a plain `member` must NOT gain blanket issue
// write/delete rights — only owner-members, the issue creator, or an instance
// admin. Private workspaces keep member == writer.

const h = vi.hoisted(() => {
  const state = {
    workspace: { id: `ws-1`, isPublic: false, publicWritePolicy: `members` },
    member: undefined as { role: string } | undefined,
    isAdmin: false,
    issueCreatorId: `creator-1`,
  }
  return { state }
})

vi.mock(`@/lib/auth/membership`, () => ({
  getIssueWorkspaceContext: vi.fn(async () => ({
    issueId: `issue-1`,
    projectId: `proj-1`,
    workspaceId: `ws-1`,
  })),
  getWorkspaceById: vi.fn(async () => h.state.workspace),
  getWorkspaceMember: vi.fn(async () => h.state.member),
  isWorkspaceModerator: vi.fn(async () => false),
  assertWorkspaceAccess: vi.fn(),
  assertMatchingWorkspaceIds: vi.fn(),
}))

vi.mock(`@/lib/admin`, () => ({
  isUserAdmin: vi.fn(async () => h.state.isAdmin),
}))

vi.mock(`@/db/connection`, () => {
  const queryBuilder: Record<string, unknown> = {}
  for (const method of [`from`, `where`, `limit`]) {
    queryBuilder[method] = vi.fn(() => queryBuilder)
  }
  ;(queryBuilder as { then: unknown }).then = (
    resolve: (v: unknown) => unknown,
    reject: (e: unknown) => unknown
  ) =>
    Promise.resolve([{ creatorId: h.state.issueCreatorId }]).then(
      resolve,
      reject
    )
  return { db: { select: vi.fn(() => queryBuilder) } }
})

import { assertIssueAccess } from "@/lib/auth/access"

describe(`assertIssueAccess write/delete`, () => {
  beforeEach(() => {
    h.state.workspace = {
      id: `ws-1`,
      isPublic: false,
      publicWritePolicy: `members`,
    }
    h.state.member = undefined
    h.state.isAdmin = false
    h.state.issueCreatorId = `creator-1`
  })

  it(`private workspace: any member may write`, async () => {
    h.state.member = { role: `member` }
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `write`)
    ).resolves.toBeTruthy()
  })

  it(`private workspace: non-member is rejected`, async () => {
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `write`)
    ).rejects.toThrow(`Not a member`)
  })

  it(`public workspace: plain member gets NO blanket write`, async () => {
    h.state.workspace.isPublic = true
    h.state.member = { role: `member` }
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `write`)
    ).rejects.toThrow(`Only the issue creator or a workspace member`)
  })

  it(`public workspace: owner-member may write`, async () => {
    h.state.workspace.isPublic = true
    h.state.member = { role: `owner` }
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `write`)
    ).resolves.toBeTruthy()
  })

  it(`public workspace: the issue creator may write (member or not)`, async () => {
    h.state.workspace.isPublic = true
    h.state.member = { role: `member` }
    h.state.issueCreatorId = `user-1`
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `write`)
    ).resolves.toBeTruthy()
  })

  it(`public workspace: instance admin may write`, async () => {
    h.state.workspace.isPublic = true
    h.state.isAdmin = true
    await expect(
      assertIssueAccess(`user-1`, `issue-1`, `delete`)
    ).resolves.toBeTruthy()
  })
})
