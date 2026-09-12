import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { SessionFileCard } from "@/components/session-file-card"

// The card pulls in diff-view (for the shared +N -M cell), which imports the
// tRPC client at module load.
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

const files = (count: number) =>
  Array.from({ length: count }, (_, i) => ({
    path: `src/file-${i}.ts`,
    additions: i,
    deletions: 1,
  }))

describe(`SessionFileCard (§12)`, () => {
  it(`titles the turn's files and opens the pane at one`, () => {
    const onOpenFile = vi.fn()
    render(
      <SessionFileCard
        card={{ afterId: 7, files: files(2) }}
        onOpenFile={onOpenFile}
      />
    )
    expect(screen.getByText(`2 files edited`)).toBeTruthy()
    fireEvent.click(screen.getByTitle(`src/file-1.ts`))
    expect(onOpenFile).toHaveBeenCalledWith(`src/file-1.ts`)
  })

  it(`lists five paths and folds the rest`, () => {
    render(
      <SessionFileCard card={{ afterId: 1, files: files(7) }} onOpenFile={vi.fn()} />
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
      <SessionFileCard card={{ afterId: 1, files: files(1) }} onOpenFile={vi.fn()} />
    )
    expect(screen.getByText(`1 file edited`)).toBeTruthy()
    expect(screen.queryByText(/more$/)).toBeNull()
  })
})
