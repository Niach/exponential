import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { parsePatch } from "@exp/domain-contract/diff"
import { SessionFileCard } from "@/components/session-file-card"

// EXP-895: a card row IS a `DiffFile` — the call's own hunks ride it, so the
// Changes face can open on the turn alone (EXP-862).
const files = (count: number) =>
  Array.from({ length: count }, (_, i) =>
    parsePatch(
      `src/file-${i}.ts`,
      `modified`,
      `@@ -1 +1 @@\n-old ${i}\n+new ${i}`
    )
  )

describe(`SessionFileCard (§12)`, () => {
  it(`titles the turn's files and opens the pane at one, scoped to the turn`, () => {
    const onOpenFile = vi.fn()
    const card = { turnId: 5, afterId: 7, files: files(2) }
    render(<SessionFileCard card={card} onOpenFile={onOpenFile} />)
    expect(screen.getByText(`2 files edited`)).toBeTruthy()
    fireEvent.click(screen.getByTitle(`src/file-1.ts`))
    expect(onOpenFile).toHaveBeenCalledWith(`src/file-1.ts`, card)
  })

  it(`lists five paths and folds the rest`, () => {
    render(
      <SessionFileCard card={{ turnId: 1, afterId: 1, files: files(7) }} onOpenFile={vi.fn()} />
    )
    expect(screen.getByText(`7 files edited`)).toBeTruthy()
    expect(screen.queryByTitle(`src/file-6.ts`)).toBeNull()
    fireEvent.click(screen.getByText(`2 more`))
    expect(screen.getByTitle(`src/file-6.ts`)).toBeTruthy()
    fireEvent.click(screen.getByText(`Show less`))
    expect(screen.queryByTitle(`src/file-6.ts`)).toBeNull()
  })

  it(`one file reads in the singular and needs no fold`, () => {
    render(
      <SessionFileCard card={{ turnId: 1, afterId: 1, files: files(1) }} onOpenFile={vi.fn()} />
    )
    expect(screen.getByText(`1 file edited`)).toBeTruthy()
    expect(screen.queryByText(/more$/)).toBeNull()
  })
})
