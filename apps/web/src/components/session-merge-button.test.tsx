import {
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react"
import { TRPCClientError } from "@trpc/client"
import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  canOfferFixConflicts,
  SessionMergeButton,
  SessionMergePill,
} from "@/components/session-merge-button"
import {
  closeLaunchDialog,
  useLaunchDialogSeed,
} from "@/lib/launch-dialog-store"

const mockState = vi.hoisted(() => ({
  mergeMutate: vi.fn(),
  sessionMergeMutate: vi.fn(),
  navigate: vi.fn(),
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

// EXP-825: the conflict swap's "Fix conflicts" hands the ONE launcher a seed
// (no device lookup here). EXP-1019: that seed names an action, so it opens
// the start-coding dialog over this surface instead of navigating — the
// router stub stays, to prove nothing travels.
vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => mockState.navigate,
  useParams: () => ({ teamSlug: `acme` }),
  // EXP-851: `useOpenComposer` reads the current screen + its `?from=` so the
  // launch carries the origin it was started from — a context-free page here,
  // so the composer URL stays exactly the seed.
  useLocation: ({ select }: { select: (l: { pathname: string }) => unknown }) =>
    select({ pathname: `/t/acme/devices` }),
  useSearch: ({ select }: { select: (s: Record<string, unknown>) => unknown }) =>
    select({}),
}))

vi.mock(`sonner`, () => ({
  toast: { error: vi.fn() },
}))

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
    expect(screen.getByText(/Merge PR #7 into the default branch/)).toBeTruthy()

    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))
    await waitFor(() =>
      expect(mockState.mergeMutate).toHaveBeenCalledWith(
        { issueId: `i1` },
        { context: { skipErrorToast: true } }
      )
    )
    // Resolved, but the row has not echoed yet — still "Merging…".
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
  // rule) was dead on every issue-fed surface — the tray, the run header, the
  // Changes faces, the review detail all toasted a real conflict instead.
  it(`the swap rule needs a conflict, an issue, a branch and the relay — never a team id`, () => {
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

  // EXP-706: "Fix conflicts" REPLACES Merge in its own slot, never sits
  // beside it — and only where the caller wired the recovery run. The props
  // here are EXACTLY what `mergeTargetProps` derives from a synced issue row
  // (EXP-917: no team id — the shape never syncs one).
  it(`swaps to Fix conflicts when the merge is refused by a conflict`, async () => {
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

    const fix = await screen.findByRole(`button`, {
      name: `Fix merge conflicts`,
    })
    expect(fix.textContent).toContain(`Fix conflicts`)
    // One trailing action, not two.
    expect(screen.queryByRole(`button`, { name: `Merge pull request` })).toBeNull()

    // EXP-825: the click lands on the composer with the builtin picked and
    // this PR pre-filled (any linked issue id resolves the PR, EXP-323).
    // EXP-1019: on the launcher DIALOG, over the run the merge failed on.
    fireEvent.click(fix)
    expect(mockState.navigate).not.toHaveBeenCalled()
    const { result } = renderHook(() => useLaunchDialogSeed())
    expect(result.current).toEqual({
      issueIds: [],
      actionId: `builtin:fix-conflicts`,
      prIssueId: `i1`,
    })
    closeLaunchDialog()
  })

  // The swap must never be a dead end: a conflict resolved OUTSIDE the
  // recovery run (a teammate rebases and pushes) has to be mergeable again.
  it(`keeps a Retry merge affordance beside the swapped-in Fix conflicts`, async () => {
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
    await screen.findByRole(`button`, { name: `Fix merge conflicts` })

    mockState.mergeMutate.mockReset()
    mockState.mergeMutate.mockResolvedValue({ merged: true })
    fireEvent.click(screen.getByRole(`button`, { name: `Retry merge` }))
    expect(screen.getByText(/Merge PR #7 into the default branch/)).toBeTruthy()
    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))
    await waitFor(() =>
      expect(mockState.mergeMutate).toHaveBeenCalledWith(
        { issueId: `i1` },
        { context: { skipErrorToast: true } }
      )
    )
  })

  // A refusal describes ONE snapshot of the PR — a re-synced issue row drops
  // it, so the plain Merge button comes back on its own.
  it(`drops a stale refusal when the issue row re-syncs`, async () => {
    mockState.mergeMutate.mockRejectedValue(conflictError())
    const { rerender } = render(
      <SessionMergeButton
        prState="open"
        prNumber={7}
        issueId="i1"
        updatedAt="2026-09-01T10:00:00.000Z"
        label="Merge"
        branch="exp/MET-12"
        steerEnabled
      />
    )

    fireEvent.click(screen.getByRole(`button`, { name: `Merge pull request` }))
    fireEvent.click(screen.getByRole(`button`, { name: `Merge` }))
    await screen.findByRole(`button`, { name: `Fix merge conflicts` })

    rerender(
      <SessionMergeButton
        prState="open"
        prNumber={7}
        issueId="i1"
        updatedAt="2026-09-01T10:05:00.000Z"
        label="Merge"
        branch="exp/MET-12"
        steerEnabled
      />
    )

    await waitFor(() =>
      expect(
        screen.queryByRole(`button`, { name: `Fix merge conflicts` })
      ).toBeNull()
    )
    expect(
      screen.getByRole(`button`, { name: `Merge pull request` })
    ).toBeTruthy()
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
      screen.getByText(/The run's coding session closes unless the team/)
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

  // EXP-895: the `pill` arm — the ONE merge control every Changes surface wears.
  // Same behaviour, a `Pill size="md" mode="action" primary` instead of a Button.
  it(`the pill arm is the primary Pill, with the same confirm`, async () => {
    render(<SessionMergePill prState="open" prNumber={7} issueId="i1" label="Merge PR" />)
    const pill = screen.getByRole<HTMLButtonElement>(`button`, {
      name: `Merge pull request`,
    })
    expect(pill.dataset.slot).toBe(`pill`)
    expect(pill.textContent).toContain(`Merge PR`)
    // The accent paint comes from `Pill`'s own `primary` flag — no hand-rolled
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
  // of `sm` pills (status, priority, Stop) — the same box, not a taller one.
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

  it(`the pill arm swaps to Fix conflicts in the SAME slot`, async () => {
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
    const fix = await screen.findByRole<HTMLButtonElement>(`button`, {
      name: `Fix merge conflicts`,
    })
    expect(fix.dataset.slot).toBe(`pill`)
    expect(screen.queryByRole(`button`, { name: `Merge pull request` })).toBeNull()
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
    expect(
      screen.queryByRole(`button`, { name: `Fix merge conflicts` })
    ).toBeNull()
  })
})
