import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import prChecks from "@exp/domain-contract/fixtures/pr-checks.json"
import mergeTrain from "@exp/domain-contract/fixtures/merge-train.json"
import { checksList } from "@exp/domain-contract/pr-checks"
import { TooltipProvider } from "@exp/ui"
import type { PullRequestRow } from "@/lib/session-tree"

// EXP-1307 / #EXP-1710: the thread header's `…` on the Guide replays
// merge-train.json `menu` (`mergeMenuEntries`, ×4): the Checks submenu
// whenever the PR has check rows (any viewer), Merge now (owners), Hold |
// Resume auto-merge, Close PR (members, a live PR). EXP-1338: no GitHub
// entry, so nothing renders no trigger at all.

vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
const checks = vi.hoisted(() => ({ rows: [] as unknown[] }))
vi.mock(`@/lib/collections`, () => ({ pullRequestCheckCollection: {} }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...((await importOriginal()) as object),
  useLiveQuery: () => ({ data: checks.rows }),
}))
const item = (id: string, label: string) => ({ kind: `item`, id, label, onSelect: () => {}, "data-testid": id })
vi.mock(`@/components/session-merge-button`, () => ({
  PullRequestStatusPill: () => null,
  usePullRequestClose: (pr: { state: string }, canClose: boolean) => ({
    entry: canClose && (pr.state === `open` || pr.state === `draft`) ? item(`close-pr`, `Close PR without merging`) : null,
    dialog: null,
  }),
  usePullRequestTrainRows: () => ({
    mergeNow: item(`merge-now`, `Merge now`),
    hold: item(`hold`, `Hold auto-merge`),
    resume: item(`resume`, `Resume auto-merge`),
    dialog: null,
  }),
}))

import { ThreadGuideMenu } from "@/components/thread/thread-guide"

const pr = (state: string, mergeState = `checks`) =>
  ({ id: `p1`, number: 54, state, mergeState, url: `https://github.com/a/b/pull/54` }) as PullRequestRow

function openMenu(state: string, canClose: boolean, isOwner = false, mergeState = `checks`) {
  render(
    <TooltipProvider>
      <ThreadGuideMenu pr={pr(state, mergeState)} canClose={canClose} isOwner={isOwner} />
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
  for (const c of mergeTrain.menu.filter((c) => c.input.state !== null)) {
    it(`merge-train menu: ${c.name}`, () => {
      checks.rows = prChecks.specimen.checks.slice(0, c.input.checks)
      const { state, member, owner, mergeState } = c.input as { state: string; member: boolean; owner: boolean; mergeState: string }
      if (c.expected.length === 0) return
      openMenu(state, member, owner, mergeState)
      const ids = screen
        .getAllByRole(`menuitem`)
        .map((node) => (node.getAttribute(`data-testid`) === prChecks.testIds.checksMenu ? `checks` : node.getAttribute(`data-testid`)))
      expect(ids).toEqual(c.expected)
    })
  }

  it(`a member's open PR: Hold, then Close`, () => {
    expect(openMenu(`open`, true)).toEqual([`Hold auto-merge`, closePrCopy.menuItem])
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
    expect(openMenu(`open`, true)).toEqual([`${prChecks.copy.checks}1 failing`, `Hold auto-merge`, closePrCopy.menuItem])
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
