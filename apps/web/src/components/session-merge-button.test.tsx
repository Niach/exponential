import {
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
  within,
} from "@testing-library/react"
import { TRPCClientError } from "@trpc/client"
import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  canOfferFixConflicts,
  MERGE_PR_LABEL,
  MERGE_STACK_LABEL,
  MergeCapsule,
  MergePrPill,
  MobileMergeCircle,
  SessionMergeButton,
  SessionMergePill,
  useMergeThrough,
} from "@/components/session-merge-button"
import {
  closeLaunchDialog,
  useLaunchDialogSeed,
} from "@/lib/launch-dialog-store"

const mockState = vi.hoisted(() => ({
  mergeMutate: vi.fn(),
  sessionMergeMutate: vi.fn(),
  navigate: vi.fn(),
  // EXP-1248: what the synced rows say about the PR's stack; the hook itself
  // is plumbing over live queries, tested in `use-stack-merge-choice.test.tsx`.
  stackChoice: vi.fn(),
  linkedCount: vi.fn(() => 1),
}))

vi.mock(`@/hooks/use-stack-merge-choice`, () => ({
  useStackMergeConfirm: (issueId: string | undefined, enabled: boolean) =>
    mockState.stackChoice(issueId, enabled),
}))

// EXP-1215: the confirm's body names how many issues the PR links.
vi.mock(`@/hooks/use-linked-issue-count`, () => ({
  useLinkedIssueCount: () => mockState.linkedCount(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: {
      mergePr: {
        mutate: mockState.mergeMutate,
      },
    },
    codingSessions: {
      mergePr: {
        mutate: mockState.sessionMergeMutate,
      },
    },
  },
}))

// EXP-825/EXP-1233: a refused conflict hands the ONE launcher a seed (no
// device lookup here). EXP-1019: that seed names an action, so it opens the
// start-coding dialog over this surface instead of navigating: the router
// stub stays, to prove nothing travels.
vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => mockState.navigate,
  useParams: () => ({ teamSlug: `acme` }),
  // EXP-851: `useOpenComposer` reads the current screen + its `?from=` so the
  // launch carries the origin it was started from: a context-free page here,
  // so the composer URL stays exactly the seed.
  useLocation: ({ select }: { select: (l: { pathname: string }) => unknown }) =>
    select({ pathname: `/t/acme/devices` }),
  useSearch: ({ select }: { select: (s: Record<string, unknown>) => unknown }) =>
    select({}),
}))

vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { error: vi.fn() },
}))
import { toast } from "sonner"

// A server refusal that codes as a real merge conflict (EXP-533).
function conflictError() {
  const error = new TRPCClientError(`Pull Request is not mergeable`)
  Object.assign(error, { data: { code: `CONFLICT` } })
  return error
}

// EXP-678: the one Merge control the Agents row and the steering strip share.
describe(`SessionMergeButton`, () => {
  beforeEach(() => {
    mockState.mergeMutate.mockReset()
    mockState.mergeMutate.mockResolvedValue({ merged: true })
    mockState.sessionMergeMutate.mockReset()
    mockState.sessionMergeMutate.mockResolvedValue({ merged: true })
    mockState.navigate.mockReset()
    mockState.stackChoice.mockReset()
    mockState.stackChoice.mockReturnValue({ ready: true, confirm: null })
    vi.mocked(toast.error).mockReset()
  })

  it(`renders nothing unless the PR is open`, () => {
    const { container } = render(
      <SessionMergeButton prState="merged" prNumber={7} issueId="i1" />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`renders the labeled glass pill the steering strip asks for`, () => {
    render(
      <SessionMergeButton
        prState="open"
        prNumber={7}
        issueId="i1"
        variant="glass"
        size="sm"
        label="Merge"
      />
    )
    const button = screen.getByRole<HTMLButtonElement>(`button`, {
      name: `Merge pull request`,
    })
    expect(button.textContent).toContain(`Merge`)
    expect(button.dataset.variant).toBe(`glass`)
    expect(button.dataset.size).toBe(`sm`)
  })

  it(`merges only after the confirm, and holds the spinner until the echo`, async () => {
    const { rerender } = render(
      <SessionMergeButton prState="open" prNumber={7} issueId="i1" label="Merge" />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    expect(mockState.mergeMutate).not.toHaveBeenCalled()
    expect(screen.getByText(`Merge PR #7?`)).toBeTruthy()

    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))
    await waitFor(() =>
      expect(mockState.mergeMutate).toHaveBeenCalledWith(
        { issueId: `i1` },
        { context: { skipErrorToast: true } }
      )
    )
    // Resolved, but the row has not echoed yet: still "Merging…".
    await waitFor(() =>
      expect(
        screen.getByRole<HTMLButtonElement>(`button`, { name: `Merging…` })
          .disabled
      ).toBe(true)
    )

    rerender(
      <SessionMergeButton prState="merged" prNumber={7} issueId="i1" label="Merge" />
    )
    expect(screen.queryByRole(`button`)).toBeNull()
  })

  // EXP-917: the swap gate takes only what a SYNCED issue row carries. The
  // `issues` shape drops `team_id`, so a gate on `teamId` (the pre-EXP-917
  // rule) was dead on every issue-fed surface: the tray, the run header, the
  // Changes faces, the review detail all toasted a real conflict instead.
  it(`the recovery rule needs a conflict, an issue, a branch and the relay: never a team id`, () => {
    const conflict = { message: `conflict`, conflict: true }
    expect(
      canOfferFixConflicts({
        failure: conflict,
        issueId: `i1`,
        branch: `exp/MET-12`,
        steerEnabled: true,
      })
    ).toBe(true)
    // Every other refusal keeps the plain Merge (EXP-533).
    expect(
      canOfferFixConflicts({
        failure: { message: `stale base`, conflict: false },
        issueId: `i1`,
        branch: `exp/MET-12`,
        steerEnabled: true,
      })
    ).toBe(false)
    // A run's own chore PR has no issue for the builtin to take (EXP-734).
    expect(
      canOfferFixConflicts({
        failure: conflict,
        issueId: undefined,
        branch: `exp/chat-abcd1234`,
        steerEnabled: true,
      })
    ).toBe(false)
    // The run rebases the branch, so one must be recorded.
    expect(
      canOfferFixConflicts({
        failure: conflict,
        issueId: `i1`,
        branch: null,
        steerEnabled: true,
      })
    ).toBe(false)
    // No relay, no composer to send the person to.
    expect(
      canOfferFixConflicts({
        failure: conflict,
        issueId: `i1`,
        branch: `exp/MET-12`,
        steerEnabled: false,
      })
    ).toBe(false)
  })

  // EXP-1233: a merge refused by a REAL conflict opens the LAUNCHER at once
  // on the Fix merge conflicts builtin with this PR picked and the refusal
  // flagged — no "Fix conflicts" button parked in the slot, no toast. The
  // props here are EXACTLY what `mergeTargetProps` derives from a synced
  // issue row (EXP-917: no team id: the shape never syncs one).
  it(`opens the recovery composer when the merge is refused by a conflict`, async () => {
    mockState.mergeMutate.mockRejectedValue(conflictError())
    render(
      <SessionMergeButton
        prState="open"
        prNumber={7}
        issueId="i1"
        label="Merge"
        branch="exp/MET-12"
        steerEnabled
      />
    )

    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))

    // EXP-1019: on the launcher DIALOG, over the run the merge failed on;
    // any linked issue id resolves the PR (EXP-323).
    const { result } = renderHook(() => useLaunchDialogSeed())
    await waitFor(() =>
      expect(result.current).toEqual({
        issueIds: [],
        actionId: `builtin:fix-conflicts`,
        prIssueId: `i1`,
        conflict: true,
      })
    )
    expect(mockState.navigate).not.toHaveBeenCalled()
    expect(toast.error).not.toHaveBeenCalled()
    // The button is plain Merge again: a conflict resolved outside the run
    // is one click away, and no "Fix conflicts" swap takes the slot.
    expect(
      screen.getByRole<HTMLButtonElement>(`button`, {
        name: `Merge pull request`,
      }).disabled
    ).toBe(false)
    expect(
      screen.queryByRole(`button`, { name: `Fix merge conflicts` })
    ).toBeNull()
    expect(screen.queryByRole(`button`, { name: `Retry merge` })).toBeNull()
    closeLaunchDialog()
  })

  // Every OTHER refusal (a stale base, branch protection, no network) is a
  // toast — a rebase-and-resolve run fixes none of them.
  it(`toasts a refusal that is not a conflict`, async () => {
    const error = new TRPCClientError(`Squash merges are not allowed`)
    Object.assign(error, { data: { code: `PRECONDITION_FAILED` } })
    mockState.mergeMutate.mockRejectedValue(error)
    render(
      <SessionMergeButton
        prState="open"
        prNumber={7}
        issueId="i1"
        label="Merge"
        branch="exp/MET-12"
        steerEnabled
      />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(`Couldn't merge the pull request`, {
        description: `Squash merges are not allowed`,
      })
    )
    const { result } = renderHook(() => useLaunchDialogSeed())
    expect(result.current).toBeNull()
  })

  // EXP-734: a run's OWN chore PR merges through the session, and the
  // recovery run cannot take it (the builtin action needs a representative
  // issue), so a conflict only reports itself.
  it(`a session target calls codingSessions.mergePr and never swaps to Fix conflicts on a CONFLICT error`, async () => {
    mockState.sessionMergeMutate.mockRejectedValue(conflictError())
    render(
      <SessionMergeButton
        prState="open"
        prNumber={7}
        sessionId="s1"
        label="Merge"
        branch="exp/chat-abcd1234"
        steerEnabled
      />
    )

    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    expect(
      screen.getByText(`It is squash-merged. No issue is linked to it.`)
    ).toBeTruthy()
    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))

    await waitFor(() =>
      expect(mockState.sessionMergeMutate).toHaveBeenCalledWith(
        { sessionId: `s1` },
        { context: { skipErrorToast: true } }
      )
    )
    expect(mockState.mergeMutate).not.toHaveBeenCalled()
    await waitFor(() =>
      expect(
        screen.getByRole<HTMLButtonElement>(`button`, {
          name: `Merge pull request`,
        }).disabled
      ).toBe(false)
    )
    expect(
      screen.queryByRole(`button`, { name: `Fix merge conflicts` })
    ).toBeNull()
  })

  // EXP-895: the `pill` arm: the ONE merge control every Changes surface wears.
  // Same behaviour, a `Pill size="md" mode="action" primary` instead of a Button.
  it(`the pill arm is the primary Pill, with the same confirm`, async () => {
    render(<SessionMergePill prState="open" prNumber={7} issueId="i1" label="Merge PR" />)
    const pill = screen.getByRole<HTMLButtonElement>(`button`, {
      name: `Merge pull request`,
    })
    expect(pill.dataset.slot).toBe(`pill`)
    expect(pill.textContent).toContain(`Merge PR`)
    // The accent paint comes from `Pill`'s own `primary` flag: no hand-rolled
    // `bg-primary` class beside it any more.
    expect(pill.className).toContain(`bg-primary`)
    expect(pill.className).toContain(`h-8`)

    fireEvent.click(pill)
    expect(mockState.mergeMutate).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))
    await waitFor(() =>
      expect(mockState.mergeMutate).toHaveBeenCalledWith(
        { issueId: `i1` },
        { context: { skipErrorToast: true } }
      )
    )
  })

  // EXP-889: in the property tray / run header the merge pill stands in a row
  // of `sm` pills (status, priority, Stop): the same box, not a taller one.
  it(`the tray pill takes the sm box of its siblings`, () => {
    render(
      <SessionMergePill
        prState="open"
        prNumber={7}
        issueId="i1"
        label="Merge PR"
        pillSize="sm"
      />
    )
    const pill = screen.getByRole<HTMLButtonElement>(`button`, {
      name: `Merge pull request`,
    })
    expect(pill.className).toContain(`h-6`)
    expect(pill.className).not.toContain(`h-8`)
  })

  it(`the pill arm opens the recovery composer on a conflict too`, async () => {
    mockState.mergeMutate.mockRejectedValue(conflictError())
    render(
      <SessionMergePill
        prState="open"
        prNumber={7}
        issueId="i1"
        label="Merge PR"
        branch="exp/MET-12"
        steerEnabled
      />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))
    const { result } = renderHook(() => useLaunchDialogSeed())
    await waitFor(() =>
      expect(result.current?.actionId).toBe(`builtin:fix-conflicts`)
    )
    expect(
      screen.getByRole<HTMLButtonElement>(`button`, { name: `Merge pull request` })
        .dataset.slot
    ).toBe(`pill`)
    closeLaunchDialog()
  })

  it(`keeps the plain Merge button when the caller wired no recovery run`, async () => {
    mockState.mergeMutate.mockRejectedValue(conflictError())
    render(
      <SessionMergeButton prState="open" prNumber={7} issueId="i1" label="Merge" />
    )

    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))

    await waitFor(() => expect(mockState.mergeMutate).toHaveBeenCalled())
    await waitFor(() =>
      expect(
        screen.getByRole<HTMLButtonElement>(`button`, {
          name: `Merge pull request`,
        }).disabled
      ).toBe(false)
    )
    // No branch, no relay: the refusal is a toast, the launcher stays shut.
    expect(toast.error).toHaveBeenCalled()
    const { result } = renderHook(() => useLaunchDialogSeed())
    expect(result.current).toBeNull()
  })
})

// EXP-1248: a stack member never merges off the plain confirm: ONE stack
// confirm, Merge stack through the top.
describe(`SessionMergeButton on a stack member`, () => {
  const confirm = {
    title: `Merge stack`,
    landing: [`EXP-1105`, `EXP-1144`, `EXP-1150`],
    staysOpen: [],
    body: `Lands 3 pull requests, bottom-up: EXP-1105, EXP-1144, EXP-1150.`,
    input: { issueId: `t`, mergeStack: true as const },
  }

  beforeEach(() => {
    mockState.mergeMutate.mockReset()
    mockState.mergeMutate.mockResolvedValue({ merged: true })
    mockState.stackChoice.mockReset()
    // The read is armed by the click, never before it.
    mockState.stackChoice.mockImplementation((_issueId, enabled: boolean) =>
      enabled ? { ready: true, confirm } : { ready: true, confirm: null }
    )
  })

  it(`an icon-only control arms the stack read on the click, not on render`, () => {
    render(<SessionMergeButton prState="open" prNumber={7} issueId="i1" />)
    expect(mockState.stackChoice).toHaveBeenCalledWith(`i1`, false)
    expect(mockState.stackChoice).not.toHaveBeenCalledWith(`i1`, true)
    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    expect(mockState.stackChoice).toHaveBeenCalledWith(`i1`, true)
  })

  // EXP-1251: the labelled control (tray pill, phone capsule) knows its word
  // before the click.
  it(`a labelled control reads the stack eagerly and says Merge stack`, () => {
    render(
      <SessionMergeButton prState="open" prNumber={7} issueId="i1" label="Merge PR" />
    )
    expect(mockState.stackChoice).toHaveBeenCalledWith(`i1`, true)
    const button = screen.getByRole(`button`, { name: `Merge stack` })
    expect(button.textContent).toBe(`Merge stack`)
  })

  it(`opens the ONE stack confirm and lands the whole stack from its top member`, async () => {
    render(
      <SessionMergeButton prState="open" prNumber={7} issueId="i1" label="Merge" />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Merge stack` }))
    await screen.findByText(
      `Lands 3 pull requests, bottom-up: EXP-1105, EXP-1144, EXP-1150.`
    )
    expect(screen.queryByText(`Merge PR #7?`)).toBeNull()
    // Two answers only: Cancel and the action itself.
    const dialog = screen.getByTestId(`stack-merge-confirm-dialog`)
    expect(within(dialog).queryByRole(`button`, { name: `Merge this pull request` })).toBeNull()

    fireEvent.click(within(dialog).getByRole(`button`, { name: `Merge stack` }))
    await waitFor(() =>
      expect(mockState.mergeMutate).toHaveBeenCalledWith(
        { issueId: `t`, mergeStack: true },
        { context: { skipErrorToast: true } }
      )
    )
    // The spinner holds until the row echoes, like a plain merge.
    await waitFor(() =>
      expect(
        screen.getByRole<HTMLButtonElement>(`button`, { name: `Merging…` })
          .disabled
      ).toBe(true)
    )
  })

  it(`toasts a refused stack merge, never a recovery run`, async () => {
    mockState.mergeMutate.mockRejectedValue(new Error(`GitHub could not stack PRs`))
    render(
      <SessionMergeButton prState="open" prNumber={7} issueId="i1" label="Merge" />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Merge stack` }))
    const dialog = await screen.findByTestId(`stack-merge-confirm-dialog`)
    fireEvent.click(within(dialog).getByRole(`button`, { name: `Merge stack` }))
    await waitFor(() => expect(toast.error).toHaveBeenCalled())
  })

  it(`Cancel merges nothing`, async () => {
    render(
      <SessionMergeButton prState="open" prNumber={7} issueId="i1" label="Merge" />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Merge stack` }))
    fireEvent.click(await screen.findByRole(`button`, { name: `Cancel` }))
    await waitFor(() =>
      expect(screen.queryByTestId(`stack-merge-confirm-dialog`)).toBeNull()
    )
    expect(mockState.mergeMutate).not.toHaveBeenCalled()
  })

  it(`a session target never asks about a stack`, () => {
    render(
      <SessionMergeButton prState="open" prNumber={7} sessionId="s1" label="Merge" />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    expect(mockState.stackChoice).not.toHaveBeenCalledWith(expect.anything(), true)
    expect(screen.getByText(`Merge PR #7?`)).toBeTruthy()
  })
})

// EXP-1251: ONE merge control, three dresses — the tray / header pill, the
// phone Guide bar's white capsule and the composer bar's circle — and the
// stack rail's Merge through here on the same one confirm.
describe(`the merge control's dresses`, () => {
  const target = {
    issueId: `i1`,
    prState: `open`,
    prNumber: 7,
    branch: `exp/EXP-1`,
    steerEnabled: false,
  }

  beforeEach(() => {
    mockState.stackChoice.mockReset()
    mockState.stackChoice.mockImplementation(() => ({ ready: true, confirm: null }))
  })

  it(`speaks the contract's words`, () => {
    expect(MERGE_PR_LABEL).toBe(`Merge PR`)
    expect(MERGE_STACK_LABEL).toBe(`Merge stack`)
  })

  it(`the pill and the capsule say Merge PR and hide once the PR is not open`, () => {
    const { rerender } = render(<MergePrPill {...target} />)
    expect(screen.getByRole(`button`).textContent).toBe(`Merge PR`)
    rerender(<MergeCapsule {...target} />)
    expect(screen.getByRole(`button`).textContent).toBe(`Merge PR`)
    rerender(<MergeCapsule {...target} prState="merged" />)
    expect(screen.queryByRole(`button`)).toBeNull()
  })

  it(`the circle is the glyph alone`, () => {
    render(<MobileMergeCircle {...target} />)
    const button = screen.getByRole(`button`, { name: `Merge pull request` })
    expect(button.textContent).toBe(``)
  })

  it(`Merge through here asks the one confirm and merges through the member`, async () => {
    const confirm = {
      title: `Merge through here`,
      landing: [`EXP-1`, `EXP-2`],
      staysOpen: [`EXP-3`],
      body: `Lands EXP-1, EXP-2; EXP-3 stays open.`,
      input: { issueId: `m2`, mergeStack: true as const },
    }
    mockState.stackChoice.mockImplementation((issueId: string | undefined, enabled: boolean) =>
      enabled && issueId === `m2` ? { ready: true, confirm } : { ready: true, confirm: null }
    )
    mockState.mergeMutate.mockReset()
    mockState.mergeMutate.mockResolvedValue({ merged: true })
    function Host() {
      const through = useMergeThrough()
      return (
        <>
          <button type="button" onClick={() => through.request(`m2`)}>
            hover action
          </button>
          {through.dialog}
        </>
      )
    }
    render(<Host />)
    fireEvent.click(screen.getByText(`hover action`))
    const dialog = await screen.findByTestId(`stack-merge-confirm-dialog`)
    expect(within(dialog).getByText(`Lands EXP-1, EXP-2; EXP-3 stays open.`)).toBeTruthy()
    fireEvent.click(within(dialog).getByRole(`button`, { name: `Merge through here` }))
    await waitFor(() =>
      expect(mockState.mergeMutate).toHaveBeenCalledWith(
        { issueId: `m2`, mergeStack: true },
        { context: { skipErrorToast: true } }
      )
    )
  })
})
