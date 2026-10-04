import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const prFiles = vi.hoisted(() => vi.fn())

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: { prFiles: { query: prFiles } },
    repositories: { branchDiff: { query: vi.fn(async () => null) } },
  },
}))

import { useReviewFiles } from "@/hooks/use-review-files"

const patchFile = (filename: string) => ({
  filename,
  status: `modified`,
  additions: 3,
  deletions: 1,
  patch: `@@ -1 +1 @@\n-a\n+b`,
})

// EXP-1154: the issue route is reused across issues, so issue A's files must
// never survive the switch to issue B, even while B's fetch is disabled.
describe(`useReviewFiles`, () => {
  beforeEach(() => {
    prFiles.mockReset()
    prFiles.mockResolvedValue({ files: [patchFile(`src/a.ts`)] })
  })

  it(`resets to loading on an issue change while disabled`, async () => {
    const { result, rerender } = renderHook(
      ({ id, enabled }: { id: string; enabled: boolean }) =>
        useReviewFiles({ id, prNumber: 7 }, { enabled }),
      { initialProps: { id: `a`, enabled: true } }
    )
    await waitFor(() => expect(result.current.state.kind).toBe(`files`))

    await act(async () => {
      rerender({ id: `b`, enabled: false })
    })
    expect(result.current.state).toEqual({ kind: `loading` })
    expect(prFiles).toHaveBeenCalledTimes(1)
  })

  it(`keeps its files while the issue stays the same`, async () => {
    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useReviewFiles({ id: `a`, prNumber: 7 }, { enabled }),
      { initialProps: { enabled: true } }
    )
    await waitFor(() => expect(result.current.state.kind).toBe(`files`))
    rerender({ enabled: false })
    expect(result.current.state.kind).toBe(`files`)
  })
})
