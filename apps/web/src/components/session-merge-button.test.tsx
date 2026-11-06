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

import { PullRequestStatusPill } from "@/components/session-merge-button"
import { MERGE_STACK_LABEL } from "@/lib/pr-stack"
import mergeTrain from "@exp/domain-contract/fixtures/merge-train.json"

const pr = {
  id: `pr-1`,
  number: 42,
  head: `exp/APP-5`,
  base: `main`,
  state: `open` as const,
  url: `https://github.com/acme/app/pull/42`,
  repositoryId: `repo-1`,
  mergeState: `checks` as const,
  mergeWait: null as string | null,
  mergeHold: null as string | null,
  mergePosition: null as number | null,
  mergeTrainId: null as string | null,
}

const pill = () => screen.getByTestId(mergeTrain.testIds.statusPill)
const approval = { ...pr, mergeWait: `approval` }

// #EXP-1710: THE header merge STATUS pill (merge-train.json `statusPill`).
describe(`PullRequestStatusPill`, () => {
  beforeEach(() => {
    merge.mockReset()
    failedAction.mockReset()
    stack.confirm = null
    checks.rows = []
  })

  it(`a closed PR has none; a merged one reads Merged`, () => {
    const { rerender } = render(<PullRequestStatusPill pr={{ ...pr, state: `closed` as never }} />)
    expect(screen.queryByTestId(mergeTrain.testIds.statusPill)).toBeNull()
    rerender(<PullRequestStatusPill pr={{ ...pr, state: `merged` as never }} />)
    expect(pill().textContent).toBe(`Merged`)
  })

  it(`a status without check rows is readonly`, () => {
    render(<PullRequestStatusPill pr={{ ...pr, mergeState: `queued` as never, mergePosition: 2 }} />)
    expect(pill().textContent).toBe(`Queued · 2nd`)
    expect(pill().getAttribute(`data-mode`)).toBe(`readonly`)
  })

  it(`a status with check rows opens the Checks list`, async () => {
    checks.rows = [{ name: `gate`, status: `in_progress`, conclusion: null }]
    render(<PullRequestStatusPill pr={{ ...pr, mergeState: `conflict` as never }} />)
    expect(pill().textContent).toBe(`Conflict`)
    expect(pill().getAttribute(`data-tone`)).toBe(`red`)
    expect(pill().getAttribute(`data-mode`)).toBe(`action`)
  })

  it(`approval mode: Needs approval confirms, then queues the one PR`, async () => {
    merge.mockResolvedValue({ merged: false, queued: true })
    render(<PullRequestStatusPill pr={approval} />)
    expect(pill().textContent).toBe(`Needs approval`)
    fireEvent.click(pill())
    fireEvent.click(await screen.findByRole(`button`, { name: /^Merge$/ }))
    await waitFor(() => expect(merge).toHaveBeenCalledWith({ pullRequestId: `pr-1` }, {}))
  })

  it(`a viewer's Needs approval is no action`, () => {
    render(<PullRequestStatusPill pr={approval} member={false} />)
    expect(pill().getAttribute(`data-mode`)).toBe(`readonly`)
  })

  it(`a stack member approves the stack through its top`, async () => {
    merge.mockResolvedValue({ merged: false, queued: true })
    stack.confirm = {
      title: MERGE_STACK_LABEL,
      landing: [`#42`, `#43`],
      staysOpen: [],
      body: `Lands 2 pull requests, bottom-up: #42, #43.`,
      input: { issueId: `pr-2`, mergeStack: true },
    }
    render(<PullRequestStatusPill pr={approval} />)
    fireEvent.click(pill())
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
    merge.mockResolvedValue({ merged: false, error: `Conflicts` })
    render(<PullRequestStatusPill pr={approval} />)
    fireEvent.click(pill())
    fireEvent.click(await screen.findByRole(`button`, { name: /^Merge$/ }))
    await waitFor(() => expect(failedAction).toHaveBeenCalledWith(`merge`, `the PR`, `Conflicts`))
  })
})
