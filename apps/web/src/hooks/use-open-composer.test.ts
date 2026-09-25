import { renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

const navigate = vi.hoisted(() => vi.fn())
vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => navigate,
  useParams: () => ({ teamSlug: `acme` }),
  useLocation: () => `/t/acme/boards/web/issues/APP-3`,
  useSearch: () => null,
}))

import { composerSearch, useOpenComposer } from "@/hooks/use-open-composer"
import {
  closeLaunchDialog,
  useLaunchDialogSeed,
} from "@/lib/launch-dialog-store"

// EXP-870: the composer URL for a seed and an origin.
describe(`composerSearch`, () => {
  it(`carries the origin's token beside the seed`, () => {
    expect(
      composerSearch({ actionId: `a1` }, { kind: `board`, boardSlug: `web` })
    ).toEqual({ action: `a1`, from: `board:web` })
  })

  // A pinned action is context-free: no `from`, so the Agent page opens
  // full-width and the run it starts brings no list along.
  it(`omits from for a context-free (pinned) start`, () => {
    expect(composerSearch({ actionId: `a1` }, null)).toEqual({ action: `a1` })
  })

  it(`keeps every seeded field`, () => {
    expect(
      composerSearch({ issueIds: [`i1`, `i2`], text: `go` }, { kind: `inbox` })
    ).toEqual({ issues: `i1,i2`, text: `go`, from: `inbox` })
  })
})

// EXP-1019: a start that knows its subject opens the LAUNCHER over the
// current surface; only a plain chat still travels to the Agent page.
describe(`useOpenComposer`, () => {
  const openLauncher = (seed: Parameters<ReturnType<typeof useOpenComposer>>[0]) => {
    navigate.mockClear()
    const { result } = renderHook(() => useOpenComposer())
    result.current(seed)
  }

  it(`opens the dialog on the action, without navigating`, () => {
    openLauncher({ actionId: `builtin:fix-conflicts`, prIssueId: `i9` })
    expect(navigate).not.toHaveBeenCalled()
    const { result } = renderHook(() => useLaunchDialogSeed())
    expect(result.current).toEqual({
      actionId: `builtin:fix-conflicts`,
      prIssueId: `i9`,
      issueIds: [],
    })
    closeLaunchDialog()
  })

  it(`opens the dialog on the picked issues, without navigating`, () => {
    openLauncher({ issueIds: [`i1`, `i2`] })
    expect(navigate).not.toHaveBeenCalled()
    const { result } = renderHook(() => useLaunchDialogSeed())
    expect(result.current).toEqual({ issueIds: [`i1`, `i2`] })
    closeLaunchDialog()
  })

  it(`still navigates to the Agent page for a subjectless chat`, () => {
    openLauncher({ text: `hello` })
    expect(navigate).toHaveBeenCalledTimes(1)
    expect(navigate.mock.calls[0]![0]).toMatchObject({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug: `acme` },
    })
  })
})
