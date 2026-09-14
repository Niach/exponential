import { readFileSync } from "node:fs"
import { join } from "node:path"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

vi.mock(`@/hooks/use-mobile`, () => ({ useIsMobile: () => true }))

import { IssueChip, type IssueChipIssue } from "@/components/issue-chip"

// EXP-885: the ONE web issue chip — the shape, the three parts, the ✕. The
// hover preview is switched off here by the mobile mock (the hover host
// self-disables on phones), so nothing reaches the synced collections.

const issue: IssueChipIssue = {
  id: `i1`,
  identifier: `APP-12`,
  title: `Android issue badges have no status`,
  status: `in_progress`,
  statusId: null,
}

function chip(): HTMLElement {
  return screen.getByTestId(`chip`)
}

describe(`IssueChip`, () => {
  it(`draws the glyph, the identifier and the title in one rounded rect`, () => {
    render(<IssueChip issue={issue} testId="chip" />)
    const root = chip()
    expect(root.textContent).toContain(`APP-12`)
    expect(root.textContent).toContain(`Android issue badges have no status`)
    // The status glyph is an svg, first child of the chip's body.
    expect(root.querySelector(`svg`)).toBeTruthy()
    // The shared box, never a capsule (EXP-423: a 6px rect on all four
    // clients).
    expect(root.className).toContain(`issue-chip`)
    expect(root.className).not.toContain(`rounded-full`)
    // No ✕ unless the caller asked for one, and nothing is a target.
    expect(screen.queryByRole(`button`)).toBeNull()
  })

  it(`opens on click when the caller passes a handler`, () => {
    const onClick = vi.fn()
    render(<IssueChip issue={issue} onClick={onClick} testId="chip" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Open APP-12` }))
    expect(onClick).toHaveBeenCalledTimes(1)
  })

  it(`renders the trailing ✕ inside the chip and never removes on a body click`, () => {
    const onRemove = vi.fn()
    const onClick = vi.fn()
    render(
      <IssueChip
        issue={issue}
        onClick={onClick}
        onRemove={onRemove}
        testId="chip"
        removeTestId="chip-remove"
      />
    )
    const remove = screen.getByTestId(`chip-remove`)
    expect(remove.getAttribute(`aria-label`)).toBe(`Remove APP-12`)
    // INSIDE the chip — no separate capsule button beside it.
    expect(chip().contains(remove)).toBe(true)
    expect(chip().getAttribute(`data-removable`)).toBe(`true`)
    fireEvent.click(screen.getByRole(`button`, { name: `Open APP-12` }))
    expect(onRemove).not.toHaveBeenCalled()
    fireEvent.click(remove)
    expect(onRemove).toHaveBeenCalledTimes(1)
  })

  it(`keeps the ✕ in place but inert while the host is busy`, () => {
    render(
      <IssueChip
        issue={issue}
        onRemove={vi.fn()}
        removeDisabled
        testId="chip"
        removeTestId="chip-remove"
      />
    )
    expect((screen.getByTestId(`chip-remove`) as HTMLButtonElement).disabled).toBe(
      true
    )
  })
})

describe(`the chip's shared box`, () => {
  // The markdown editor draws the same chip as a ProseMirror DECORATION
  // (lib/issue-ref-extension.ts) — the document text must stay the bare
  // `#IDENT` token. Both carry `issue-chip`, so the box cannot drift.
  const src = join(import.meta.dirname, `..`)
  const css = readFileSync(join(src, `styles.css`), `utf8`)

  it(`lives in styles.css as a 6px rect, not a capsule`, () => {
    const block = css.slice(css.indexOf(`\n.issue-chip {`))
    expect(block.slice(0, block.indexOf(`}`))).toContain(`border-radius: 6px`)
  })

  it(`is what the editor decoration carries too`, () => {
    const extension = readFileSync(
      join(src, `lib`, `issue-ref-extension.ts`),
      `utf8`
    )
    expect(extension).toContain(`class: \`issue-chip issue-ref-pill\``)
    // The decoration keeps ONLY what a decoration must do itself.
    expect(css).toContain(`.tiptap-content .issue-ref-pill::before`)
  })
})
