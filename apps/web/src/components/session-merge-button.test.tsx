import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { TRPCClientError } from "@trpc/client"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { SessionMergeButton } from "@/components/session-merge-button"

const mockState = vi.hoisted(() => ({
  mergeMutate: vi.fn(),
  sessionMergeMutate: vi.fn(),
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

// The conflict swap's launcher: stubbed so the test never drags the synced
// device collections (and the whole launch dialog) into jsdom.
vi.mock(`@/hooks/use-remote-start`, () => ({
  useRemoteStart: () => ({
    devices: [],
    starting: false,
    sentTo: null,
    startIssues: vi.fn(),
    runAction: vi.fn(),
    refresh: vi.fn(),
    latestVersions: null,
  }),
}))

vi.mock(`@/components/launch-dialog/launch-dialog`, () => ({
  LaunchDialog: () => null,
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

  // EXP-706: "Fix conflicts" REPLACES Merge in its own slot, never sits
  // beside it — and only where the caller wired the recovery run.
  it(`swaps to Fix conflicts when the merge is refused by a conflict`, async () => {
    mockState.mergeMutate.mockRejectedValue(conflictError())
    render(
      <SessionMergeButton
        prState="open"
        prNumber={7}
        issueId="i1"
        label="Merge"
        branch="exp/MET-12"
        teamId="t1"
        currentUserId="u1"
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
        teamId="t1"
        currentUserId="u1"
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
        teamId="t1"
        currentUserId="u1"
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
        teamId="t1"
        currentUserId="u1"
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
        teamId="t1"
        currentUserId="u1"
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
