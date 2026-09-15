import { readFileSync } from "node:fs"
import { join } from "node:path"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => true,
}))

import { IssueChip as IssueChipView } from "@exp/ui"
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
  //
  // EXP-887: the box itself (`.issue-chip`, a 6px rect) moved into @exp/ui's
  // styles.css and is locked by that package's styles.test.ts; what stays here
  // is the half only the app has — the decoration that copies the class.
  const src = join(import.meta.dirname, `..`)
  const css = readFileSync(join(src, `styles.css`), `utf8`)

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

describe(`an issue the client cannot resolve`, () => {
  // EXP-887: the steering feed's MCP tool-result preview used to drop to bare
  // mono text when the row had not synced here. It is still an ISSUE, so it
  // draws the SAME chip — just inert: no target, no hover preview, and a muted
  // backlog glyph standing in for the status nothing can resolve yet.
  it(`still draws the chip, inert and muted`, () => {
    render(
      <IssueChipView
        identifier="APP-99"
        title="Filed by an agent on another team's board"
        status={{ icon: `circle-dashed`, colorClass: `text-muted-foreground` }}
        testId="chip"
      />
    )
    const root = chip()
    expect(root.className).toContain(`issue-chip`)
    expect(root.textContent).toContain(`APP-99`)
    expect(root.textContent).toContain(
      `Filed by an agent on another team's board`
    )
    expect(root.querySelector(`svg`)?.getAttribute(`class`)).toContain(
      `text-muted-foreground`
    )
    // Inert: nothing to open, nothing to remove.
    expect(screen.queryByRole(`button`)).toBeNull()
    expect(root.getAttribute(`data-removable`)).toBeNull()
  })
})
