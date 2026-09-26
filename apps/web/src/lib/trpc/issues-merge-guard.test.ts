import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { router, authedProcedure } from "@/lib/trpc"
import {
  WORKFLOW_LANDING,
  WORKFLOW_MERGE_REFUSAL,
  assertMergeOutsideWorkflow,
} from "@/lib/trpc/issues"

// EXP-1094: a live workflow node's PR merges only through the workflow's
// train; every hand merge (Reviews, the issue page, MCP pr_merge, which all
// call issues.mergePr) is refused.

function fakeExecutor(results: unknown[][]) {
  const queue = [...results]
  const chain = (rows: unknown[]) => {
    const p = Promise.resolve(rows) as Promise<unknown[]> & Record<string, () => unknown>
    for (const m of [`from`, `where`, `innerJoin`, `limit`]) p[m] = () => p
    return p
  }
  return { select: () => chain(queue.shift() ?? []) } as never
}

describe(`assertMergeOutsideWorkflow`, () => {
  it(`refuses a PR whose issue a live workflow covers`, async () => {
    const executor = fakeExecutor([[{ workflowId: `wf-1`, nodeId: `n-1`, issueId: `a` }]])
    await expect(assertMergeOutsideWorkflow(executor, [`a`], `EXP-1`)).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `EXP-1's pull request ${WORKFLOW_MERGE_REFUSAL}`,
    })
  })

  it(`lets any other PR through`, async () => {
    await expect(
      assertMergeOutsideWorkflow(fakeExecutor([[]]), [`a`, `b`], `EXP-1`)
    ).resolves.toBeUndefined()
  })
})

describe(`WORKFLOW_LANDING`, () => {
  it(`survives the authed middleware, so landNode's caller is recognised`, async () => {
    const probe = router({
      seen: authedProcedure.query(({ ctx }) =>
        Boolean((ctx as { [WORKFLOW_LANDING]?: boolean })[WORKFLOW_LANDING])
      ),
    })
    const ctx = { session: { user: { id: `u-1` } }, db: {}, request: new Request(`http://x`) } as never
    expect(await probe.createCaller(ctx).seen()).toBe(false)
    expect(
      await probe.createCaller({ ...(ctx as object), [WORKFLOW_LANDING]: true } as never).seen()
    ).toBe(true)
  })
})
