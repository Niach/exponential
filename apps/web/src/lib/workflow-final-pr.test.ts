import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/integrations/github-pr`, () => ({
  createPullRequest: vi.fn(),
  resolveRepoToken: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({ applyPrLifecycleStatusInTx: vi.fn() }))
vi.mock(`@/lib/trpc/repositories`, () => ({ effectiveDefaultBranch: () => `main` }))
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoDefaultBranchCached: vi.fn(),
}))

import { finalPrBody, pickAuditNodes } from "@/lib/workflow-final-pr"

// EXP-982 — the final PR's body and its audit pick.
describe(`pickAuditNodes`, () => {
  const nodes = Array.from({ length: 10 }, (_, i) => `n${i}`)

  it(`picks k distinct nodes, the same ones for the same workflow`, () => {
    const first = pickAuditNodes(nodes, `wf-1`, 3)
    expect(first).toHaveLength(3)
    expect(new Set(first).size).toBe(3)
    expect(pickAuditNodes(nodes, `wf-1`, 3)).toEqual(first)
    expect(pickAuditNodes(nodes, `wf-2`, 3)).not.toEqual(first)
  })

  it(`never asks for more than there is`, () => {
    expect(pickAuditNodes([`a`, `b`], `wf`, 3).sort()).toEqual([`a`, `b`])
    expect(pickAuditNodes([], `wf`, 3)).toEqual([])
  })
})

describe(`finalPrBody`, () => {
  it(`lists every node as an issue ref, the audit picks as a checklist, and the decisions`, () => {
    const body = finalPrBody({
      name: `Mobile polish`,
      nodes: [
        { identifier: `APP-6`, title: `Contract`, prUrl: `https://gh/pr/1`, kind: `contract` },
        { identifier: `APP-7`, title: `Leaf`, prUrl: null, kind: `leaf` },
      ],
      audit: [{ identifier: `APP-7`, prUrl: null }],
      decisions: `2026-09-19: use cursor pagination`,
    })
    expect(body).toContain(`- #APP-6 (contract): https://gh/pr/1`)
    expect(body).toContain(`- #APP-7`)
    expect(body).toContain(`- [ ] #APP-7`)
    expect(body).toContain(`claims, not evidence`)
    expect(body).toContain(`## Decisions\n2026-09-19: use cursor pagination`)
  })

  it(`leaves the optional sections out when empty`, () => {
    const body = finalPrBody({ name: `X`, nodes: [], audit: [], decisions: `` })
    expect(body).not.toContain(`## Audit`)
    expect(body).not.toContain(`## Decisions`)
  })
})
