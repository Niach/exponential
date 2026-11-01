import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import prChecks from "@exp/domain-contract/fixtures/pr-checks.json"
import { checksList } from "@exp/domain-contract/pr-checks"
import { TooltipProvider } from "@exp/ui"
import type { PullRequestRow } from "@/lib/session-tree"

// EXP-1307: the thread header's `…` on the Guide (`guideMenuEntries`, ×4):
// Close PR while the PR is open and the viewer is a member. EXP-1338: no
// GitHub entry, so anything else renders no trigger at all. #EXP-1611:
// the Checks submenu leads whenever the PR has check rows, any viewer.

vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
const checks = vi.hoisted(() => ({ rows: [] as unknown[] }))
vi.mock(`@/lib/collections`, () => ({ pullRequestCheckCollection: {} }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...((await importOriginal()) as object),
  useLiveQuery: () => ({ data: checks.rows }),
}))
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

beforeEach(() => {
  vi.restoreAllMocks()
  checks.rows = []
})

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

  it(`#EXP-1611: the Checks submenu leads for any viewer, then Close`, () => {
    checks.rows = prChecks.specimen.checks
    expect(openMenu(`open`, false)).toEqual([`${prChecks.copy.checks}1 failing`])
  })

  it(`#EXP-1611: the Checks rows in checksList order; a pick opens the logs; no url = disabled`, () => {
    checks.rows = prChecks.specimen.checks
    const open = vi.spyOn(window, `open`).mockImplementation(() => null)
    expect(openMenu(`open`, true)).toEqual([`${prChecks.copy.checks}1 failing`, closePrCopy.menuItem])
    const trigger = screen.getByTestId(prChecks.testIds.checksMenu)
    trigger.focus()
    fireEvent.keyDown(trigger, { key: `ArrowRight` })
    const rows = screen.getAllByTestId(prChecks.testIds.checkRow)
    const expected = checksList(prChecks.specimen.checks, new Date())
    expect(rows.map((row) => row.textContent)).toEqual(
      expected.map((row) => `${row.name}${row.duration ?? ``}`)
    )
    expect(rows[0].textContent).toBe(`gate4m 12s`)
    expect(rows[4].getAttribute(`data-disabled`)).not.toBeNull()
    fireEvent.click(rows[0])
    expect(open).toHaveBeenCalledWith(prChecks.specimen.checks[3].detailsUrl, `_blank`, `noopener`)
  })
})
