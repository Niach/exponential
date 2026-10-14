import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { DiffFile } from "@exp/domain-contract/diff"
import { GUIDE_NO_CHANGES, GuideBody } from "@/components/guide-face"

const failedAction = vi.hoisted(() => vi.fn())
vi.mock(`@/lib/action-error`, () => ({ failedAction }))

// EXP-1251: the Guide body — one Changes row per section, Other changes,
// Show complete diff.

function diffFile(path: string, additions: number, deletions: number): DiffFile {
  return { path, status: `modified`, additions, deletions, hunks: [] } as unknown as DiffFile
}

const group = (topic: string, text: string, files: string[] = []) => ({
  topic,
  text,
  entries: [],
  earlier: [],
  files,
})

describe(`GuideBody`, () => {
  const files = [diffFile(`a.ts`, 3, 1), diffFile(`b.ts`, 2, 0), diffFile(`c.ts`, 1, 1)]

  it(`one Changes row per section, Other changes and Show complete diff`, () => {
    const onOpenChanges = vi.fn()
    render(
      <GuideBody
        groups={[group(`Summary`, `Did it.`), group(`Model`, `Slots.`, [`a.ts`, `b.ts`])]}
        files={files}
        onOpenChanges={onOpenChanges}
      />
    )
    const rows = screen.getAllByTestId(`guide-changes-row`)
    expect(rows).toHaveLength(2)
    expect(rows[0]?.textContent).toContain(`2 files`)
    expect(screen.getByTestId(`guide-other-changes`).textContent).toContain(`Other changes`)
    fireEvent.click(rows[0]!)
    expect(onOpenChanges).toHaveBeenCalledWith(
      expect.objectContaining({ section: 1 })
    )
    fireEvent.click(screen.getByTestId(`guide-complete-diff`))
    expect(onOpenChanges).toHaveBeenLastCalledWith(
      expect.objectContaining({ section: `all` })
    )
  })

  it(`with no report the diff alone is one Changes section`, () => {
    render(<GuideBody groups={[]} files={files} />)
    expect(screen.getByTestId(`guide-other-changes`).textContent).toContain(`Changes`)
  })

  it(`says so while nothing was pushed, waits while the PR loads`, () => {
    const { rerender } = render(
      <GuideBody groups={[]} files={null} filesState={{ kind: `none` }} />
    )
    expect(screen.getByText(GUIDE_NO_CHANGES)).toBeTruthy()
    rerender(<GuideBody groups={[]} files={null} filesState={{ kind: `loading` }} />)
    expect(screen.getByTestId(`feedback-skeleton`)).toBeTruthy()
  })

  it(`a failed fetch on open = the empty state + Retry, no toast; a failed Retry tap toasts`, () => {
    failedAction.mockClear()
    const onRetry = vi.fn()
    const body = (filesState: { kind: `error`; message: string } | { kind: `loading` }) => (
      <GuideBody groups={[]} files={null} filesState={filesState} onRetry={onRetry} />
    )
    const { rerender } = render(body({ kind: `error`, message: `boom` }))
    expect(screen.getByText(GUIDE_NO_CHANGES)).toBeTruthy()
    rerender(body({ kind: `error`, message: `boom` }))
    expect(failedAction).not.toHaveBeenCalled()

    fireEvent.click(screen.getByText(`Retry`))
    expect(onRetry).toHaveBeenCalledTimes(1)
    rerender(body({ kind: `loading` }))
    rerender(body({ kind: `error`, message: `boom` }))
    expect(failedAction).toHaveBeenCalledTimes(1)
    const [verb, thing, error, options] = failedAction.mock.calls[0]
    expect([verb, thing, error]).toEqual([`load`, `the changes`, `boom`])
    options.retry()
    expect(onRetry).toHaveBeenCalledTimes(2)
  })
})
