import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/integrations/pr-sync`, () => ({ livePullRequestsOfRoot: vi.fn() }))
vi.mock(`@/lib/pull-requests`, () => ({ mergePullRequestOrThrow: vi.fn() }))

import { yoloMergeOrder } from "@/lib/yolo-tree-merge"

// EXP-1291: a thread's PRs merge parents first (base_pull_request_id), so a
// child never squashes into its parent's branch.
describe(`yoloMergeOrder`, () => {
  const at = (minute: number) => new Date(Date.UTC(2026, 9, 13, 12, minute))

  it(`puts every parent before its children, oldest first among peers`, () => {
    const order = yoloMergeOrder([
      { id: `child`, basePullRequestId: `root`, createdAt: at(1) },
      { id: `grandchild`, basePullRequestId: `child`, createdAt: at(0) },
      { id: `root`, basePullRequestId: null, createdAt: at(5) },
      { id: `lone`, basePullRequestId: null, createdAt: at(2) },
    ])
    expect(order).toEqual([`lone`, `root`, `child`, `grandchild`])
  })

  it(`treats a parent outside the set as merged already`, () => {
    expect(
      yoloMergeOrder([{ id: `a`, basePullRequestId: `gone`, createdAt: at(0) }])
    ).toEqual([`a`])
  })

  it(`survives a cycle`, () => {
    expect(
      yoloMergeOrder([
        { id: `a`, basePullRequestId: `b`, createdAt: at(0) },
        { id: `b`, basePullRequestId: `a`, createdAt: at(1) },
      ])
    ).toEqual([`a`, `b`])
  })
})
