import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const merge = vi.hoisted(() => vi.fn())
const failedAction = vi.hoisted(() => vi.fn())
const stack = vi.hoisted(() => ({ ready: true, confirm: null as unknown }))
const checks = vi.hoisted(() => ({ rows: [] as Array<{ name: string; status: string; conclusion: string | null }> }))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    pullRequests: { merge: { mutate: merge }, close: { mutate: vi.fn() } },
  },
}))
vi.mock(`@/lib/collections`, () => ({
  pullRequestCollection: { utils: { awaitTxId: vi.fn(async () => undefined) } },
  pullRequestIssueCollection: {},
  pullRequestCheckCollection: {},
}))
vi.mock(`@/hooks/use-pull-request-checks`, async (importOriginal) => ({
  ...((await importOriginal()) as object),
  usePullRequestChecks: () => checks.rows,
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
import prChecks from "@exp/domain-contract/fixtures/pr-checks.json"

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
    checks.rows = []
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

  // #EXP-1611 (pr-checks.json MERGE PILL): the pill over `checks_state`.
  it(`pending: the primary pill with THE Spinner, still enabled, the plain confirm`, async () => {
    merge.mockResolvedValue({ merged: true, txid: 1 })
    render(<PullRequestMergePill pr={{ ...pr, checksState: `pending` }} />)
    const pill = screen.getByTestId(`pr-merge-pill`)
    expect(pill.textContent).toContain(MERGE_PR_LABEL)
    expect(pill.className).toContain(`bg-primary`)
    expect(pill.querySelector(`[data-slot=spinner]`)).not.toBeNull()
    expect((pill as HTMLButtonElement).disabled).toBe(false)
    fireEvent.click(pill)
    fireEvent.click(await screen.findByRole(`button`, { name: /^Merge$/ }))
    await waitFor(() => expect(merge).toHaveBeenCalledWith({ pullRequestId: `pr-1` }, {}))
  })

  it(`failure: the plain pill asks merge-failing-pr, then merges the PR`, async () => {
    merge.mockResolvedValue({ merged: true, txid: 1 })
    checks.rows = prChecks.specimen.checks
    render(<PullRequestMergePill pr={{ ...pr, checksState: `failure` }} />)
    const pill = screen.getByTestId(`pr-merge-pill`)
    expect(pill.textContent).toBe(prChecks.copy.mergeFailing)
    expect(pill.className).not.toContain(`bg-primary`)
    expect(pill.querySelector(`[data-glyph=ci-failed]`)?.getAttribute(`class`)).toContain(`text-destructive`)
    fireEvent.click(pill)
    expect(await screen.findByText(`Merge PR #42 with failing checks?`)).toBeTruthy()
    expect(screen.getByText(`Failing: gate.`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`pr-merge-failing-confirm`))
    await waitFor(() => expect(merge).toHaveBeenCalledWith({ pullRequestId: `pr-1` }, {}))
    expect(screen.queryByTestId(`stack-merge-confirm-dialog`)).toBeNull()
  })

  it(`failure on a stack member: the stack title, Merge lands the stack through its top`, async () => {
    merge.mockResolvedValue({ merged: true, txid: 1 })
    checks.rows = prChecks.specimen.checks
    stack.confirm = {
      title: MERGE_STACK_LABEL,
      landing: [`#42`, `#43`],
      staysOpen: [],
      body: `Lands 2 pull requests, bottom-up: #42, #43.`,
      input: { issueId: `pr-2`, mergeStack: true },
    }
    render(<PullRequestMergePill pr={{ ...pr, checksState: `failure` }} />)
    expect(screen.getByTestId(`pr-merge-pill`).textContent).toBe(prChecks.copy.mergeFailing)
    fireEvent.click(screen.getByTestId(`pr-merge-pill`))
    expect(await screen.findByText(`Merge the stack with failing checks?`)).toBeTruthy()
    expect(screen.queryByTestId(`stack-merge-confirm-dialog`)).toBeNull()
    fireEvent.click(screen.getByTestId(`pr-merge-failing-confirm`))
    await waitFor(() =>
      expect(merge).toHaveBeenCalledWith({ pullRequestId: `pr-2`, stack: true }, {})
    )
  })
})
