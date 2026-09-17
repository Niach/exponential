import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  editCard,
  EDIT_CARD_PREVIEW,
  type EditCardFeedItem,
} from "@exp/domain-contract/edit-card"

import { EditedFilesCard } from "./edited-files-card"

// EXP-916: the card is purely presentational — the rows, the title and the
// fold label are the contract's `editCard`; the open set is the caller's.

const patch = (path: string) =>
  [`--- a/${path}`, `+++ b/${path}`, `@@ -1,2 +1,3 @@`, ` kept`, `-gone`, `+new`].join(
    `\n`
  )

const edit = (id: number, path: string): EditCardFeedItem => ({
  id,
  kind: `tool`,
  toolKind: `edit`,
  detail: path,
  settled: true,
  diff: patch(path),
})

const items = (n: number) =>
  Array.from({ length: n }, (_, i) => edit(i + 1, `src/file-${i + 1}.ts`))

describe(`EditedFilesCard`, () => {
  it(`draws the contract's title over one FileDiffCard per row`, () => {
    render(
      <EditedFilesCard
        view={editCard(items(2))}
        openPaths={new Set()}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByTestId(`edited-files-card`)).toBeTruthy()
    expect(screen.getByText(`2 files edited`)).toBeTruthy()
    expect(screen.getAllByTestId(`file-diff-card`).length).toBe(2)
  })

  it(`only an open path shows its patch, and a header click toggles it`, () => {
    const onToggle = vi.fn()
    const { rerender } = render(
      <EditedFilesCard
        view={editCard(items(2))}
        openPaths={new Set()}
        onToggle={onToggle}
      />
    )
    expect(screen.queryByText(`@@ -1,2 +1,3 @@`)).toBeNull()

    fireEvent.click(screen.getByRole(`button`, { name: /file-2\.ts/ }))
    expect(onToggle).toHaveBeenCalledWith(`src/file-2.ts`)

    rerender(
      <EditedFilesCard
        view={editCard(items(2))}
        openPaths={new Set([`src/file-2.ts`])}
        onToggle={onToggle}
      />
    )
    expect(screen.getAllByText(`@@ -1,2 +1,3 @@`).length).toBe(1)
  })

  it(`folds past the contract's preview and the toggle says so`, () => {
    const count = EDIT_CARD_PREVIEW + 3
    render(
      <EditedFilesCard
        view={editCard(items(count))}
        openPaths={new Set()}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getAllByTestId(`file-diff-card`).length).toBe(EDIT_CARD_PREVIEW)
    const more = screen.getByTestId(`edited-files-more`)
    expect(more.textContent).toBe(`3 more`)

    fireEvent.click(more)
    expect(screen.getAllByTestId(`file-diff-card`).length).toBe(count)
    expect(screen.getByTestId(`edited-files-more`).textContent).toBe(
      contract.diffUi.showLess
    )
  })

  it(`a live row past the preview keeps the whole list up, fold row gone`, () => {
    const count = EDIT_CARD_PREVIEW + 2
    const feed = items(count)
    const view = editCard(feed, feed[feed.length - 1].id)
    expect(view.liveIndex).toBe(count - 1)
    render(
      <EditedFilesCard view={view} openPaths={new Set()} onToggle={vi.fn()} />
    )
    expect(screen.getAllByTestId(`file-diff-card`).length).toBe(count)
    expect(screen.queryByTestId(`edited-files-more`)).toBeNull()
  })

  it(`pending and failed rows are headers only — no counts, no patch`, () => {
    const view = editCard([
      { id: 1, kind: `tool`, toolKind: `edit`, detail: `src/pending.ts` },
      {
        id: 2,
        kind: `tool`,
        toolKind: `edit`,
        detail: `src/broken.ts`,
        settled: true,
        failed: true,
      },
    ])
    render(
      <EditedFilesCard
        view={view}
        openPaths={new Set([`src/pending.ts`, `src/broken.ts`])}
        onToggle={vi.fn()}
      />
    )
    const cards = screen.getAllByTestId(`file-diff-card`)
    expect(cards.map((c) => c.getAttribute(`data-state-kind`))).toEqual([
      `pending`,
      `failed`,
    ])
    // No `+0 −0` chip on either, and nothing unfolds even though the caller
    // put both paths in the open set.
    expect(screen.queryByText(`+0`)).toBeNull()
    expect(screen.queryByText(`No textual diff (binary or too large)`)).toBeNull()
    expect(screen.getByText(`failed`)).toBeTruthy()
  })
})
