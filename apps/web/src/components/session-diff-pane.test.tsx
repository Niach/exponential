import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  clampDiffPaneWidth,
  readDiffPaneWidth,
  writeDiffPaneWidth,
  SessionDiffPane,
  DIFF_PANE_MIN_WIDTH,
} from "@/components/session-diff-pane"
import type { PullFile } from "@/components/diff-view"

// The pane pulls in diff-view, which imports the tRPC client at module load.
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

const file = (filename: string): PullFile => ({
  filename,
  status: `modified`,
  additions: 2,
  deletions: 1,
  patch: [`@@ -1 +1,2 @@`, ` kept`, `+added`].join(`\n`),
})

describe(`the pane's width memory (§11)`, () => {
  it(`never narrows past the minimum, and leaves one for the transcript`, () => {
    expect(clampDiffPaneWidth(100, 1_200)).toBe(DIFF_PANE_MIN_WIDTH)
    expect(clampDiffPaneWidth(600, 1_200)).toBe(600)
    expect(clampDiffPaneWidth(1_100, 1_200)).toBe(840)
    // A column too narrow for both still gives the pane its minimum.
    expect(clampDiffPaneWidth(500, 400)).toBe(DIFF_PANE_MIN_WIDTH)
  })

  it(`round-trips through storage, per session`, () => {
    const store = new Map<string, string>()
    const storage = {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
    }
    expect(readDiffPaneWidth(`sess-1`, storage)).toBeNull()
    writeDiffPaneWidth(`sess-1`, 512.6, storage)
    expect(readDiffPaneWidth(`sess-1`, storage)).toBe(513)
    expect(readDiffPaneWidth(`sess-2`, storage)).toBeNull()
  })

  it(`a junk or under-minimum entry reads as no preference`, () => {
    const storage = { getItem: (key: string) => (key.endsWith(`a`) ? `wide` : `12`) }
    expect(readDiffPaneWidth(`a`, storage)).toBeNull()
    expect(readDiffPaneWidth(`b`, storage)).toBeNull()
  })
})

describe(`SessionDiffPane`, () => {
  it(`names the selected file and closes`, () => {
    const onClose = vi.fn()
    render(
      <SessionDiffPane
        sessionId="sess-1"
        files={[file(`src/a.ts`), file(`src/b.ts`)]}
        selected="src/b.ts"
        onSelect={vi.fn()}
        onClose={onClose}
      />
    )
    expect(
      screen
        .getAllByTitle(`src/b.ts`)
        .some((node) => node.textContent === `src/b.ts`)
    ).toBe(true)
    fireEvent.click(screen.getByLabelText(`Close the changes pane`))
    expect(onClose).toHaveBeenCalledOnce()
  })

  it(`the scope chip names the turn and takes the pane back to all changes`, () => {
    const onClearScope = vi.fn()
    const { rerender } = render(
      <SessionDiffPane
        sessionId="sess-1"
        files={[file(`src/a.ts`)]}
        selected="src/a.ts"
        onSelect={vi.fn()}
        onClose={vi.fn()}
      />
    )
    // No scope: no chip at all.
    expect(screen.queryByTestId(`session-diff-scope`)).toBeNull()
    rerender(
      <SessionDiffPane
        sessionId="sess-1"
        files={[file(`src/a.ts`)]}
        selected="src/a.ts"
        onSelect={vi.fn()}
        onClose={vi.fn()}
        scopeLabel={`This turn: 1 file`}
        onClearScope={onClearScope}
      />
    )
    const chip = screen.getByTestId(`session-diff-scope`)
    expect(chip.textContent).toBe(`This turn: 1 file`)
    expect(chip.getAttribute(`title`)).toBe(`Show all changes`)
    fireEvent.click(chip)
    expect(onClearScope).toHaveBeenCalledOnce()
  })

  it(`hides the chip when there is nothing to widen back to`, () => {
    // The run has published no session diff yet, so the turn's files ARE all
    // the changes: a chip here would blank the pane on click.
    render(
      <SessionDiffPane
        sessionId="sess-1"
        files={[file(`src/a.ts`)]}
        selected="src/a.ts"
        onSelect={vi.fn()}
        onClose={vi.fn()}
        scopeLabel={`This turn: 1 file`}
      />
    )
    expect(screen.queryByTestId(`session-diff-scope`)).toBeNull()
  })

  it(`its header buttons are borderless ghosts (EXP-862)`, () => {
    render(
      <SessionDiffPane
        sessionId="sess-1"
        files={[file(`src/a.ts`), file(`src/b.ts`)]}
        selected={null}
        onSelect={vi.fn()}
        onClose={vi.fn()}
      />
    )
    for (const label of [`Show the file list`, `Close the changes pane`]) {
      const button = screen.getByLabelText(label)
      expect(button.getAttribute(`data-variant`)).toBe(`ghost`)
      expect(button.getAttribute(`data-size`)).toBe(`icon-sm`)
    }
  })

  it(`the file list is a toggle, and only with more than one file`, () => {
    const onSelect = vi.fn()
    const { rerender } = render(
      <SessionDiffPane
        sessionId="sess-1"
        files={[file(`src/a.ts`)]}
        selected={null}
        onSelect={onSelect}
        onClose={vi.fn()}
      />
    )
    expect(screen.queryByLabelText(`Show the file list`)).toBeNull()
    rerender(
      <SessionDiffPane
        sessionId="sess-1"
        files={[file(`src/a.ts`), file(`src/b.ts`)]}
        selected={null}
        onSelect={onSelect}
        onClose={vi.fn()}
      />
    )
    fireEvent.click(screen.getByLabelText(`Show the file list`))
    expect(screen.getByText(`2 files changed`)).toBeTruthy()
    // The path label renders the directory and the basename as separate
    // spans, so the row is matched by its whole text content.
    const row = screen
      .getAllByRole(`button`)
      .find((node) => node.textContent?.startsWith(`Msrc/b.ts`))
    expect(row).toBeTruthy()
    fireEvent.click(row as HTMLElement)
    expect(onSelect).toHaveBeenCalledWith(`src/b.ts`)
  })
})
