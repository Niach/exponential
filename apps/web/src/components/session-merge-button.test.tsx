import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const merge = vi.hoisted(() => vi.fn())
const failedAction = vi.hoisted(() => vi.fn())
const stack = vi.hoisted(() => ({ ready: true, confirm: null as unknown }))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    pullRequests: { merge: { mutate: merge }, close: { mutate: vi.fn() } },
  },
}))
vi.mock(`@/lib/collections`, () => ({
  pullRequestCollection: { utils: { awaitTxId: vi.fn(async () => undefined) } },
  pullRequestIssueCollection: {},
}))
vi.mock(`@/lib/action-error`, () => ({ failedAction, SKIP_ERROR_TOAST: {} }))
vi.mock(`@/hooks/use-stack-merge-choice`, () => ({
  usePullRequestStackConfirm: () => stack,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...((await importOriginal()) as object),
  useLiveQuery: () => ({ data: [] }),
}))

import { MERGE_PR_LABEL, PullRequestMergePill } from "@/components/session-merge-button"
import { MERGE_STACK_LABEL } from "@/lib/pr-stack"

const pr = {
  id: `pr-1`,
  number: 42,
  head: `exp/APP-5`,
  base: `main`,
  state: `open` as const,
  url: `https://github.com/acme/app/pull/42`,
  repositoryId: `repo-1`,
}

// EXP-1293: THE merge pill over a pull_requests row.
describe(`PullRequestMergePill`, () => {
  beforeEach(() => {
    merge.mockReset()
    failedAction.mockReset()
    stack.confirm = null
  })

  it(`hides unless the PR is open and the viewer may merge`, () => {
    const { rerender } = render(<PullRequestMergePill pr={{ ...pr, state: `merged` }} />)
    expect(screen.queryByTestId(`pr-merge-pill`)).toBeNull()
    rerender(<PullRequestMergePill pr={pr} canMerge={false} />)
    expect(screen.queryByTestId(`pr-merge-pill`)).toBeNull()
  })

  it(`confirms, then merges the one PR`, async () => {
    merge.mockResolvedValue({ merged: true, txid: 1 })
    render(<PullRequestMergePill pr={pr} />)
    expect(screen.getByTestId(`pr-merge-pill`).textContent).toContain(MERGE_PR_LABEL)
    fireEvent.click(screen.getByTestId(`pr-merge-pill`))
    fireEvent.click(await screen.findByRole(`button`, { name: /^Merge$/ }))
    await waitFor(() => expect(merge).toHaveBeenCalledWith({ pullRequestId: `pr-1` }, {}))
  })

  it(`a stack member merges the stack through its top`, async () => {
    merge.mockResolvedValue({ merged: true, txid: 1 })
    stack.confirm = {
      title: MERGE_STACK_LABEL,
      landing: [`#42`, `#43`],
      staysOpen: [],
      body: `Lands 2 pull requests, bottom-up: #42, #43.`,
      input: { issueId: `pr-2`, mergeStack: true },
    }
    render(<PullRequestMergePill pr={pr} />)
    expect(screen.getByTestId(`pr-merge-pill`).textContent).toContain(MERGE_STACK_LABEL)
    fireEvent.click(screen.getByTestId(`pr-merge-pill`))
    const dialog = await screen.findByTestId(`stack-merge-confirm-dialog`)
    fireEvent.click(
      Array.from(dialog.querySelectorAll(`button`)).find(
        (button) => button.textContent === MERGE_STACK_LABEL
      )!
    )
    await waitFor(() =>
      expect(merge).toHaveBeenCalledWith({ pullRequestId: `pr-2`, stack: true }, {})
    )
  })

  it(`a refusal toasts`, async () => {
    merge.mockResolvedValue({ merged: false, error: `Conflicts`, txid: 1 })
    render(<PullRequestMergePill pr={pr} />)
    fireEvent.click(screen.getByTestId(`pr-merge-pill`))
    fireEvent.click(await screen.findByRole(`button`, { name: /^Merge$/ }))
    await waitFor(() => expect(failedAction).toHaveBeenCalledWith(`merge`, `the PR`, `Conflicts`))
  })
})
