import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  canWidenDiffScope,
  SessionDiffFace,
} from "@/components/session-diff-face"
import type { PullFile } from "@/components/diff-view"
import { DIFF_SCOPE_ALL_LABEL } from "@/lib/session-file-cards"

// The face pulls in diff-view, which imports the tRPC client at module load.
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

const file = (filename: string): PullFile => ({
  filename,
  status: `modified`,
  additions: 2,
  deletions: 1,
  patch: [`@@ -1 +1,2 @@`, ` kept`, `+added`].join(`\n`),
})

// EXP-877: the scope-chip behaviours the diff PANE's test used to lock
// (EXP-862), now on the diff FACE.
describe(`SessionDiffFace`, () => {
  it(`the scope chip names the turn and takes the face back to all changes`, () => {
    const onClearScope = vi.fn()
    render(
      <SessionDiffFace
        files={[file(`src/a.ts`)]}
        selected="src/a.ts"
        onSelect={vi.fn()}
        scopeLabel="This turn: 1 file"
        onClearScope={onClearScope}
      />
    )
    expect(screen.getByText(`This turn: 1 file`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`session-diff-scope`))
    expect(onClearScope).toHaveBeenCalledOnce()
    expect(screen.getByLabelText(DIFF_SCOPE_ALL_LABEL)).toBeTruthy()
  })

  it(`hides the chip when there is nothing to widen back to`, () => {
    render(
      <SessionDiffFace
        files={[file(`src/a.ts`)]}
        selected={null}
        onSelect={vi.fn()}
        scopeLabel="This turn: 1 file"
      />
    )
    expect(screen.queryByTestId(`session-diff-scope`)).toBeNull()
    expect(screen.queryByText(`This turn: 1 file`)).toBeNull()
  })

  it(`lists the files and hands a pick to the caller`, () => {
    const onSelect = vi.fn()
    render(
      <SessionDiffFace
        files={[file(`src/a.ts`), file(`src/b.ts`)]}
        selected="src/a.ts"
        onSelect={onSelect}
      />
    )
    expect(screen.getByText(`2 files changed`)).toBeTruthy()
    fireEvent.click(screen.getAllByText(`b.ts`)[0])
    expect(onSelect).toHaveBeenCalledWith(`src/b.ts`)
  })
})

describe(`canWidenDiffScope`, () => {
  it(`offers the chip only once the run published a session diff`, () => {
    expect(canWidenDiffScope(0)).toBe(false)
    expect(canWidenDiffScope(1)).toBe(true)
  })
})
