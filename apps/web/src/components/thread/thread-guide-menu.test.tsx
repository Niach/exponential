import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import { contract } from "@exp/domain-contract"
import { TooltipProvider } from "@exp/ui"
import type { PullRequestRow } from "@/lib/session-tree"

// EXP-1307: the thread header's `…` on the Guide (`guideMenuEntries`, ×4):
// Open on GitHub, then Close PR while the PR is open and the viewer is a member.

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
  it(`offers Open on GitHub then Close PR on an open PR`, () => {
    expect(openMenu(`open`, true)).toEqual([contract.diffUi.openOnGithub, closePrCopy.menuItem])
  })

  it(`offers Open on GitHub alone to a non-member or on a merged PR`, () => {
    expect(openMenu(`open`, false)).toEqual([contract.diffUi.openOnGithub])
  })

  it(`opens the PR in a new tab`, () => {
    const open = vi.spyOn(window, `open`).mockReturnValue(null)
    openMenu(`merged`, true)
    fireEvent.click(screen.getByRole(`menuitem`))
    expect(open).toHaveBeenCalledWith(
      `https://github.com/a/b/pull/54`,
      `_blank`,
      `noopener,noreferrer`
    )
  })
})
