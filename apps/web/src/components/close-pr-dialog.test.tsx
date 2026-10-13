import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import { contract } from "@exp/domain-contract"
import type { Issue } from "@/db/schema"

// EXP-1154: Close PR moved into the issue's actions menu; the copy is the ×4
// fixture `close-pr.json` (one test per client reads it). EXP-1293: the PR
// is the issue's open `pull_requests` row, closed by id.

const mocks = vi.hoisted(() => ({
  closePr: vi.fn(),
  linked: [] as unknown[],
  prs: [] as unknown[],
  toastError: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { pullRequests: { close: { mutate: mocks.closePr } } },
}))
vi.mock(`@/lib/collections`, () => ({ pullRequestIssueCollection: {} }))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useIssuePullRequests: () => mocks.prs,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: mocks.linked }) }
})
vi.mock(`@/lib/action-error`, () => ({
  SKIP_ERROR_TOAST: { context: { skipErrorToast: true } },
  failedAction: mocks.toastError,
}))

import { closePrBody, useClosePr } from "@/components/close-pr-dialog"

const ISSUE = { id: `i1` } as unknown as Issue
const OPEN_PR = {
  id: `pr7`,
  number: 7,
  url: `https://github.com/o/r/pull/7`,
  state: `open`,
  updatedAt: `2026-10-04T10:00:00.000Z`,
}

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
  mocks.prs = [OPEN_PR]
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
      { pullRequestId: `pr7` },
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
    mocks.prs = [{ ...OPEN_PR, state: `merged` }]
    rerender(<Harness />)
    expect(screen.queryByText(`menu item`)).toBeNull()
  })

  it(`toasts a refusal`, async () => {
    mocks.closePr.mockRejectedValueOnce(new Error(`Branch protection`))
    render(<Harness />)
    await openDialog()
    fireEvent.click(screen.getByText(closePrCopy.confirm))
    await waitFor(() =>
      expect(mocks.toastError).toHaveBeenCalledWith(`close`, `the PR`, expect.any(String))
    )
  })
})
