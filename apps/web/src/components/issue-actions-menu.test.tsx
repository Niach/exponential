import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { contract } from "@exp/domain-contract"
import { TooltipProvider } from "@exp/ui"
import type { Issue } from "@/db/schema"

// EXP-1154: Close PR without merging sits in the issue's `…`, a destructive
// item directly above Delete issue, only while `useClosePr` offers it.

const close = vi.hoisted(() => ({
  canClose: true,
  request: vi.fn(),
}))

vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/components/issue-relations-card`, () => ({
  RELATION_SIDES: [],
  pickLabel: () => ``,
  useAddRelation: () => ({ pick: vi.fn(), dialog: null }),
}))
vi.mock(`@/components/close-pr-dialog`, () => ({
  useClosePr: () => ({
    canClose: close.canClose,
    closing: false,
    request: close.request,
    dialog: <div data-testid="close-pr-dialog" />,
  }),
}))

import { IssueActionsMenu } from "@/components/issue-actions-menu"

const issue = {
  id: `i1`,
  identifier: `MET-1`,
  duplicateOfId: null,
  prState: `open`,
} as unknown as Issue

function openMenu() {
  render(
    <TooltipProvider>
      <IssueActionsMenu issue={issue} board={{ slug: `met` }} teamSlug="acme" />
    </TooltipProvider>
  )
  fireEvent.pointerDown(screen.getByLabelText(`Issue actions`), {
    button: 0,
    pointerType: `mouse`,
  })
}

beforeEach(() => {
  close.canClose = true
  close.request.mockClear()
})

describe(`IssueActionsMenu Close PR`, () => {
  it(`puts Close PR right above Delete issue and asks on select`, () => {
    openMenu()
    const items = screen.getAllByRole(`menuitem`).map((node) => node.textContent)
    const at = items.indexOf(contract.diffUi.closePr)
    expect(at).toBeGreaterThanOrEqual(0)
    expect(items[at + 1]).toBe(`Delete issue`)
    expect(screen.getByTestId(`issue-close-pr`).getAttribute(`data-variant`)).toBe(
      `destructive`
    )
    fireEvent.click(screen.getByTestId(`issue-close-pr`))
    expect(close.request).toHaveBeenCalled()
    expect(screen.getByTestId(`close-pr-dialog`)).toBeTruthy()
  })

  it(`offers nothing without an open PR to close`, () => {
    close.canClose = false
    openMenu()
    expect(screen.queryByTestId(`issue-close-pr`)).toBeNull()
  })
})
