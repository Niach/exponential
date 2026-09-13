import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { BoardSettingsPage } from "@/components/team/boards-section"
import type { Board, Team } from "@/db/schema"

// EXP-862: the board settings DIALOG became a page, and a page has no close
// event to flush a pending rename on. The name field still commits on blur,
// but React unmounting a focused input dispatches no focusout, so every exit
// that isn't a focus change (browser Back, a keyboard nav, the nav switching
// board) has to flush the draft itself.

const mockState = vi.hoisted(() => ({
  updateMutate: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    boards: {
      update: { mutate: mockState.updateMutate },
      setRepository: { mutate: vi.fn() },
      archive: { mutate: vi.fn() },
      delete: { mutate: vi.fn() },
    },
    repositories: { add: { mutate: vi.fn() } },
  },
}))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => vi.fn(),
}))

vi.mock(`@/components/board-repo-field`, () => ({
  BoardRepoField: () => <div data-testid="repo-field" />,
}))

const board = (overrides: Partial<Board> = {}) =>
  ({
    id: `board-a`,
    name: `Apps`,
    prefix: `APP`,
    color: `#6366f1`,
    icon: `code`,
    repositoryId: null,
    defaultBranch: null,
    ...overrides,
  }) as unknown as Board

const team = { id: `team-1`, slug: `acme` } as unknown as Team

const nameField = () => screen.getByLabelText(`Name`)

describe(`BoardSettingsPage rename`, () => {
  beforeEach(() => {
    mockState.updateMutate.mockReset()
    mockState.updateMutate.mockResolvedValue({})
  })

  it(`flushes a pending rename when the page unmounts without a blur`, () => {
    const view = render(<BoardSettingsPage board={board()} team={team} />)
    fireEvent.change(nameField(), { target: { value: `Platform` } })
    expect(mockState.updateMutate).not.toHaveBeenCalled()

    view.unmount()
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      boardId: `board-a`,
      name: `Platform`,
    })
  })

  it(`commits on Enter and leaves nothing for the unmount flush`, () => {
    const view = render(<BoardSettingsPage board={board()} team={team} />)
    // Enter commits by blurring the field, so it has to hold focus the way
    // it does while someone is typing into it.
    ;(nameField() as HTMLInputElement).focus()
    fireEvent.change(nameField(), { target: { value: `Platform` } })
    fireEvent.keyDown(nameField(), { key: `Enter` })

    expect(mockState.updateMutate).toHaveBeenCalledTimes(1)
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      boardId: `board-a`,
      name: `Platform`,
    })

    view.unmount()
    expect(mockState.updateMutate).toHaveBeenCalledTimes(1)
  })

  it(`flushes onto the board being left, never the one navigated to`, () => {
    const view = render(<BoardSettingsPage board={board()} team={team} />)
    fireEvent.change(nameField(), { target: { value: `Platform` } })

    // The settings nav switching board keeps this component mounted.
    view.rerender(
      <BoardSettingsPage
        board={board({ id: `board-b`, name: `Infra`, prefix: `INF` })}
        team={team}
      />
    )
    expect(mockState.updateMutate).toHaveBeenCalledTimes(1)
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      boardId: `board-a`,
      name: `Platform`,
    })
    expect((nameField() as HTMLInputElement).value).toBe(`Infra`)

    view.unmount()
    expect(mockState.updateMutate).toHaveBeenCalledTimes(1)
  })

  // A teammate renaming the board while the page is open: the flush compares
  // against the row as it stands NOW, so a draft that already matches the
  // remote name writes nothing, an untouched field never rolls it back, and
  // a real local edit still lands.
  it(`does not rewrite a remote rename the draft already matches`, () => {
    const view = render(<BoardSettingsPage board={board()} team={team} />)
    fireEvent.change(nameField(), { target: { value: `Platform` } })
    view.rerender(
      <BoardSettingsPage board={board({ name: `Platform` })} team={team} />
    )
    view.unmount()
    expect(mockState.updateMutate).not.toHaveBeenCalled()
  })

  it(`never rolls a remote rename back to the name the field was seeded with`, () => {
    const view = render(<BoardSettingsPage board={board()} team={team} />)
    // Typed away and back: the field reads the old name, but the row moved on.
    fireEvent.change(nameField(), { target: { value: `Apps 2` } })
    fireEvent.change(nameField(), { target: { value: `Apps` } })
    view.rerender(<BoardSettingsPage board={board({ name: `Infra` })} team={team} />)
    view.unmount()
    expect(mockState.updateMutate).not.toHaveBeenCalled()
  })

  it(`still flushes a real local edit over a remote rename`, () => {
    const view = render(<BoardSettingsPage board={board()} team={team} />)
    fireEvent.change(nameField(), { target: { value: `Platform` } })
    view.rerender(<BoardSettingsPage board={board({ name: `Infra` })} team={team} />)
    view.unmount()
    expect(mockState.updateMutate).toHaveBeenCalledTimes(1)
    expect(mockState.updateMutate).toHaveBeenCalledWith({
      boardId: `board-a`,
      name: `Platform`,
    })
  })

  it(`writes nothing when the draft is unchanged or empty`, () => {
    const view = render(<BoardSettingsPage board={board()} team={team} />)
    fireEvent.change(nameField(), { target: { value: `Platform` } })
    fireEvent.change(nameField(), { target: { value: `Apps` } })
    view.unmount()

    const blank = render(<BoardSettingsPage board={board()} team={team} />)
    fireEvent.change(nameField(), { target: { value: `   ` } })
    blank.unmount()

    expect(mockState.updateMutate).not.toHaveBeenCalled()
  })
})
