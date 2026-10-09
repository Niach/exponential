import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { DiffFile } from "@exp/domain-contract/diff"
import {
  GUIDE_NO_CHANGES,
  GuideBody,
  GuideStackCard,
  PR_DESCRIPTION_EMPTY,
  PR_DESCRIPTION_FALLBACK_TOPIC,
  prDescriptionGroups,
  STACK_CARD_TITLE,
} from "@/components/guide-face"
import type { StackView } from "@/lib/pr-stack"

// EXP-1251: the Guide face — the Stack card over the report, one Changes row
// per section, Other changes, Show complete diff; the PR body stands in for a
// missing report.

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

const stack: StackView = {
  rows: [
    { issueId: `top`, identifier: `VAPP-100`, title: `SwiftUI parity`, prNumber: 3, isCurrent: true },
    { issueId: `mid`, identifier: `VAPP-98`, title: `Renderer hardening`, prNumber: 2, isCurrent: false },
    { issueId: `low`, identifier: `VAPP-93`, title: `The site`, prNumber: 1, isCurrent: false },
  ],
  baseBranch: `master`,
}

describe(`prDescriptionGroups`, () => {
  it(`is the PR body as ONE group, [] until loaded`, () => {
    expect(prDescriptionGroups({ kind: `loading` })).toEqual([])
    const [one] = prDescriptionGroups({ kind: `ready`, title: ` `, body: ``, state: `open` })
    expect(one?.topic).toBe(PR_DESCRIPTION_FALLBACK_TOPIC)
    expect(one?.text).toBe(PR_DESCRIPTION_EMPTY)
  })
})

describe(`GuideStackCard`, () => {
  it(`draws the band without a count and the rail top first to the base`, () => {
    render(<GuideStackCard stack={stack} />)
    const card = screen.getByTestId(`guide-stack-card`)
    expect(card.textContent).toContain(STACK_CARD_TITLE)
    expect(card.textContent).not.toMatch(/Stack\s*3/)
    const text = card.textContent ?? ``
    expect(text.indexOf(`VAPP-100`)).toBeLessThan(text.indexOf(`VAPP-93`))
    expect(text).toContain(`master`)
  })

  it(`a member row replaces the subject; the current one does nothing`, () => {
    const onOpen = vi.fn()
    render(<GuideStackCard stack={stack} onOpen={onOpen} />)
    fireEvent.click(screen.getByText(`Renderer hardening`))
    expect(onOpen).toHaveBeenCalledWith(`mid`)
    onOpen.mockClear()
    fireEvent.click(screen.getByText(`SwiftUI parity`))
    expect(onOpen).not.toHaveBeenCalled()
  })

  it(`hovering a member offers Merge through here`, () => {
    const onMergeThrough = vi.fn()
    render(<GuideStackCard stack={stack} onMergeThrough={onMergeThrough} />)
    fireEvent.mouseEnter(screen.getByText(`Renderer hardening`))
    fireEvent.click(screen.getByText(`Merge through here`))
    expect(onMergeThrough).toHaveBeenCalledWith(`mid`)
  })
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

  it(`puts the Stack card first`, () => {
    render(
      <GuideBody
        groups={[group(`Summary`, `Did it.`)]}
        files={null}
        stack={<GuideStackCard stack={stack} />}
      />
    )
    const body = screen.getByTestId(`guide-body`)
    expect(body.firstElementChild?.querySelector(`[data-testid="guide-stack-card"]`)).not.toBeNull()
  })

  it(`says so while nothing was pushed, waits while the PR loads`, () => {
    const { rerender } = render(
      <GuideBody groups={[]} files={null} filesState={{ kind: `none` }} />
    )
    expect(screen.getByText(GUIDE_NO_CHANGES)).toBeTruthy()
    rerender(<GuideBody groups={[]} files={null} filesState={{ kind: `loading` }} />)
    expect(screen.getByText(`Loading the pull request…`)).toBeTruthy()
  })

  it(`offers Retry on a failed fetch`, () => {
    const onRetry = vi.fn()
    render(
      <GuideBody
        groups={[]}
        files={null}
        filesState={{ kind: `error`, message: `boom` }}
        onRetry={onRetry}
      />
    )
    fireEvent.click(screen.getByText(`Retry`))
    expect(onRetry).toHaveBeenCalled()
  })
})
