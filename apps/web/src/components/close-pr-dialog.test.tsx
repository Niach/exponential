import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import { contract } from "@exp/domain-contract"
import type { Issue } from "@/db/schema"

// EXP-1154: Close PR moved into the issue's actions menu; the copy is the ×4
// fixture `close-pr.json` (one test per client reads it).

const mocks = vi.hoisted(() => ({
  closePr: vi.fn(),
  linked: [] as unknown[],
  toastError: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { issues: { closePr: { mutate: mocks.closePr } } },
}))
vi.mock(`@/lib/collections`, () => ({ issueCollection: {} }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: mocks.linked }) }
})
vi.mock(`@exp/ui`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, toast: { error: mocks.toastError, success: vi.fn() } }
})

import { closePrBody, useClosePr } from "@/components/close-pr-dialog"

const ISSUE = {
  id: `i1`,
  prUrl: `https://github.com/o/r/pull/7`,
  prNumber: 7,
  prState: `open`,
  updatedAt: `2026-10-04T10:00:00.000Z`,
} as unknown as Issue

function Harness({
  issue = ISSUE,
  readOnly = false,
}: {
  issue?: Issue
  readOnly?: boolean
}) {
  const close = useClosePr(issue, { readOnly })
  return (
    <>
      {close.canClose && (
        <button type="button" onClick={close.request}>
          menu item
        </button>
      )}
      {close.dialog}
    </>
  )
}

const openDialog = async () => {
  vi.useFakeTimers()
  fireEvent.click(screen.getByText(`menu item`))
  await act(async () => {
    vi.runAllTimers()
  })
  vi.useRealTimers()
}

beforeEach(() => {
  vi.clearAllMocks()
  mocks.linked = [{ id: `i1` }]
  mocks.closePr.mockResolvedValue({ closed: true })
})

describe(`close PR copy fixture`, () => {
  it(`pins the menu item and the batch line`, () => {
    expect(contract.diffUi.closePr).toBe(closePrCopy.menuItem)
    expect(closePrBody(0)).toBe(closePrCopy.body)
    expect(closePrBody(2)).toBe(
      `${closePrCopy.body} It also closes the pull request for 2 linked issues.`
    )
  })
})

describe(`useClosePr`, () => {
  it(`confirms with the fixture copy and closes the PR`, async () => {
    render(<Harness />)
    await openDialog()
    expect(screen.getByText(closePrCopy.title)).toBeTruthy()
    expect(screen.getByText(closePrCopy.body)).toBeTruthy()
    fireEvent.click(screen.getByText(closePrCopy.confirm))
    expect(mocks.closePr).toHaveBeenCalledWith(
      { issueId: `i1` },
      { context: { skipErrorToast: true } }
    )
  })

  it(`appends the batch line when the PR links more issues`, async () => {
    mocks.linked = [{ id: `i1` }, { id: `i2` }, { id: `i3` }]
    render(<Harness />)
    await openDialog()
    expect(screen.getByText(closePrBody(2))).toBeTruthy()
  })

  it(`is offered only to a member while the PR is open`, () => {
    const { rerender } = render(<Harness readOnly />)
    expect(screen.queryByText(`menu item`)).toBeNull()
    rerender(<Harness issue={{ ...ISSUE, prState: `merged` } as Issue} />)
    expect(screen.queryByText(`menu item`)).toBeNull()
  })

  it(`toasts a refusal`, async () => {
    mocks.closePr.mockRejectedValueOnce(new Error(`Branch protection`))
    render(<Harness />)
    await openDialog()
    fireEvent.click(screen.getByText(closePrCopy.confirm))
    await waitFor(() => expect(mocks.toastError).toHaveBeenCalled())
  })
})
