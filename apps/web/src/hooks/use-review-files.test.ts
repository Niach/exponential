import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const files = vi.hoisted(() => vi.fn())

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { pullRequests: { files: { query: files } } },
}))

import { usePullRequestFiles } from "@/hooks/use-review-files"

const patchFile = (path: string) => ({
  path,
  additions: 3,
  deletions: 1,
  patch: `@@ -1 +1 @@\n-a\n+b`,
})

// EXP-1154: a reused route never shows PR A's files on PR B.
describe(`usePullRequestFiles`, () => {
  beforeEach(() => {
    files.mockReset()
    files.mockResolvedValue({ files: [patchFile(`src/a.ts`)] })
  })

  it(`resets to loading on a PR change, before B's files land`, async () => {
    const { result, rerender } = renderHook(({ id }: { id: string }) => usePullRequestFiles(id), {
      initialProps: { id: `a` },
    })
    await waitFor(() => expect(result.current.state.kind).toBe(`files`))
    expect(files).toHaveBeenCalledWith({ pullRequestId: `a` })

    files.mockReturnValueOnce(new Promise(() => {}))
    await act(async () => {
      rerender({ id: `b` })
    })
    expect(result.current.state).toEqual({ kind: `loading` })
    expect(files).toHaveBeenLastCalledWith({ pullRequestId: `b` })
  })

  it(`reports an empty diff as none and a failure as error`, async () => {
    files.mockResolvedValueOnce({ files: [] })
    const { result } = renderHook(() => usePullRequestFiles(`a`))
    await waitFor(() => expect(result.current.state.kind).toBe(`none`))
    files.mockRejectedValueOnce(new Error(`boom`))
    await act(async () => {
      result.current.reload()
    })
    await waitFor(() => expect(result.current.state).toEqual({ kind: `error`, message: `boom` }))
  })
})
