import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { SessionMergeButton } from "@/components/session-merge-button"

const mockState = vi.hoisted(() => ({
  mergeMutate: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: {
      mergePr: {
        mutate: mockState.mergeMutate,
      },
    },
  },
}))

// EXP-678: the one Merge control the Agents row and the steering strip share.
describe(`SessionMergeButton`, () => {
  beforeEach(() => {
    mockState.mergeMutate.mockReset()
    mockState.mergeMutate.mockResolvedValue({ merged: true })
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
      expect(mockState.mergeMutate).toHaveBeenCalledWith({ issueId: `i1` })
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
})
