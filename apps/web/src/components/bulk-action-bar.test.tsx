import { act, fireEvent, render, screen } from "@testing-library/react"
import { TRPCClientError } from "@trpc/client"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { BlockCounts } from "@/lib/issue-graph"
import {
  CREATE_WORKFLOW_LABEL,
  START_AS_BATCH_LABEL,
  START_AS_STACK_LABEL,
} from "@/lib/workflow-view"

// EXP-981: the bulk bar's play MENU. One pill, three items in contract order:
// the batch (today's behaviour), the stack (one blocked issue only — the
// composer's blocked-start dialog then offers the stacked PR) and
// "Create workflow…", which files the selection as one DAG and opens it.

const counts = vi.hoisted(() => ({ map: new Map<string, BlockCounts>() }))
const openComposer = vi.hoisted(() => vi.fn())
const navigate = vi.hoisted(() => vi.fn())
const createMutate = vi.hoisted(() => vi.fn())
const toastError = vi.hoisted(() => vi.fn())

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => navigate,
  useParams: () => ({ teamSlug: `acme` }),
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})
vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  issueLabelCollection: {},
  workflowCollection: { utils: { awaitTxId: vi.fn().mockResolvedValue(true) } },
}))
vi.mock(`@/hooks/use-remote-start`, () => ({
  useRemoteStart: () => ({
    devices: [{ deviceId: `dev-1`, deviceLabel: `buildbox`, online: true }],
    starting: false,
    sentTo: null,
  }),
}))
vi.mock(`@/hooks/use-open-composer`, () => ({
  useOpenComposer: () => openComposer,
}))
vi.mock(`@/hooks/use-team-issue-graph`, () => ({
  useTeamIssueGraph: () => ({
    relations: [],
    issues: [],
    counts: counts.map,
  }),
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { workflows: { create: { mutate: createMutate } } },
}))
vi.mock(`sonner`, () => ({ toast: { error: toastError } }))

import { BulkStartCodingControl } from "@/components/bulk-action-bar"

/** Radix opens on pointerdown, not click (the repo's menu-test idiom). */
function openMenu(trigger: HTMLElement) {
  act(() => {
    fireEvent.pointerDown(trigger, {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

function mount(issueIds: string[]) {
  const onClear = vi.fn()
  render(
    <BulkStartCodingControl
      teamId="t1"
      currentUserId="u1"
      issueIds={issueIds}
      onClear={onClear}
    />
  )
  openMenu(screen.getByTestId(`bulk-start-coding`))
  return { onClear }
}

beforeEach(() => {
  counts.map = new Map()
  openComposer.mockReset()
  navigate.mockReset()
  toastError.mockReset()
  createMutate.mockReset().mockResolvedValue({
    txId: 1,
    workflow: { id: `wf-1` },
  })
})

describe(`BulkStartCodingControl`, () => {
  it(`offers the three items, in order`, () => {
    mount([`i1`, `i2`])
    expect(
      [
        screen.getByTestId(`bulk-start-batch`),
        screen.getByTestId(`bulk-start-stack`),
        screen.getByTestId(`bulk-create-workflow`),
      ].map((item) => item.textContent)
    ).toEqual([
      START_AS_BATCH_LABEL,
      START_AS_STACK_LABEL,
      CREATE_WORKFLOW_LABEL,
    ])
  })

  it(`starts a batch on the composer and clears the selection`, () => {
    const { onClear } = mount([`i1`, `i2`])
    fireEvent.click(screen.getByTestId(`bulk-start-batch`))
    expect(openComposer).toHaveBeenCalledWith({ issueIds: [`i1`, `i2`] })
    expect(onClear).toHaveBeenCalled()
  })

  it(`enables the stack only for ONE issue that something blocks`, () => {
    counts.map = new Map([[`i1`, { blockedBy: 1, blocking: 0 }]])
    mount([`i1`])
    expect(
      screen.getByTestId(`bulk-start-stack`).getAttribute(`data-disabled`)
    ).toBeNull()
  })

  it(`disables the stack for a batch, and for a single unblocked issue`, () => {
    counts.map = new Map([[`i1`, { blockedBy: 1, blocking: 0 }]])
    const batch = render(
      <BulkStartCodingControl
        teamId="t1"
        currentUserId="u1"
        issueIds={[`i1`, `i2`]}
        onClear={vi.fn()}
      />
    )
    openMenu(batch.getByTestId(`bulk-start-coding`))
    expect(
      batch.getByTestId(`bulk-start-stack`).getAttribute(`data-disabled`)
    ).toBe(``)
    batch.unmount()

    counts.map = new Map()
    mount([`i1`])
    expect(
      screen.getByTestId(`bulk-start-stack`).getAttribute(`data-disabled`)
    ).toBe(``)
  })

  it(`files the selection as a workflow and opens it`, async () => {
    const { onClear } = mount([`i1`, `i2`])
    fireEvent.click(screen.getByTestId(`bulk-create-workflow`))
    await vi.waitFor(() =>
      expect(createMutate).toHaveBeenCalledWith(
        { teamId: `t1`, issueIds: [`i1`, `i2`] },
        expect.anything()
      )
    )
    await vi.waitFor(() => expect(onClear).toHaveBeenCalled())
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        to: `/t/$teamSlug/workflows/$workflowId`,
        params: { teamSlug: `acme`, workflowId: `wf-1` },
      })
    )
  })

  it(`shows the router's own sentence when a workflow is refused`, async () => {
    createMutate.mockRejectedValue(
      new TRPCClientError(`APP-3 is already started; pick backlog issues`)
    )
    mount([`i1`, `i2`])
    fireEvent.click(screen.getByTestId(`bulk-create-workflow`))
    await vi.waitFor(() =>
      expect(toastError).toHaveBeenCalledWith(
        `APP-3 is already started; pick backlog issues`
      )
    )
    expect(navigate).not.toHaveBeenCalled()
  })
})
