import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import { TooltipProvider } from "@exp/ui"
import type { PullRequestRow } from "@/lib/session-tree"

// EXP-1307: the thread header's `…` on the Guide (`guideMenuEntries`, ×4):
// Close PR while the PR is open and the viewer is a member. EXP-1338: no
// GitHub entry, so anything else renders no trigger at all.

vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/components/session-merge-button`, () => ({
  PullRequestMergePill: () => null,
  usePullRequestClose: (pr: { state: string }, canClose: boolean) => ({
    entry:
      canClose && pr.state === `open`
        ? {
            kind: `item`,
            id: `close-pr`,
            label: `Close PR without merging`,
            destructive: true,
            onSelect: () => {},
          }
        : null,
    dialog: null,
  }),
}))

import { ThreadGuideMenu } from "@/components/thread/thread-guide"

const pr = (state: string) =>
  ({ id: `p1`, number: 54, state, url: `https://github.com/a/b/pull/54` }) as PullRequestRow

function openMenu(state: string, canClose: boolean) {
  render(
    <TooltipProvider>
      <ThreadGuideMenu pr={pr(state)} canClose={canClose} />
    </TooltipProvider>
  )
  fireEvent.pointerDown(screen.getByTestId(`thread-guide-menu`), {
    button: 0,
    pointerType: `mouse`,
  })
  return screen.getAllByRole(`menuitem`).map((node) => node.textContent)
}

beforeEach(() => vi.restoreAllMocks())

describe(`ThreadGuideMenu`, () => {
  it(`offers Close PR alone on an open PR`, () => {
    expect(openMenu(`open`, true)).toEqual([closePrCopy.menuItem])
  })

  it(`renders no trigger for a non-member or a merged PR`, () => {
    for (const [state, canClose] of [
      [`open`, false],
      [`merged`, true],
    ] as const) {
      const { unmount } = render(
        <TooltipProvider>
          <ThreadGuideMenu pr={pr(state)} canClose={canClose} />
        </TooltipProvider>
      )
      expect(screen.queryByTestId(`thread-guide-menu`)).toBeNull()
      unmount()
    }
  })
})
