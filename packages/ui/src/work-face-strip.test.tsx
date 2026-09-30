import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { DropdownMenuContent } from "./dropdown-menu"
import { ChangesFaceLabel, WorkFaceStrip } from "./work-face-strip"

// EXP-1152: THE Work face strip — hidden faces, absent under two unless the
// Runs caret earns it, and the Changes segment's counts-or-word label.

describe(`ChangesFaceLabel`, () => {
  it(`is the +N −M counts once known, the word until then`, () => {
    const { container, rerender } = render(
      <ChangesFaceLabel counts={{ additions: 12, deletions: 2 }} />
    )
    expect(container.textContent).toBe(`+12 −2`)
    expect(container.querySelector(`[aria-label="+12 −2"]`)).not.toBeNull()
    rerender(<ChangesFaceLabel counts={null} />)
    expect(container.textContent).toBe(`Changes`)
  })
})

describe(`WorkFaceStrip`, () => {
  it(`renders nothing under two faces without a run menu`, () => {
    const { container } = render(
      <WorkFaceStrip
        face="issue"
        items={[{ face: `issue`, label: `Issue`, onSelect: vi.fn() }]}
      />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`a lone Runs item with a menu still shows its caret`, () => {
    render(
      <WorkFaceStrip
        face="run"
        items={[{ face: `run`, label: `Runs`, onSelect: vi.fn() }]}
        runMenu={{ content: <DropdownMenuContent /> }}
      />
    )
    expect(screen.getByTestId(`work-face-toggle`)).toBeTruthy()
    expect(screen.getByTestId(`issue-run-switcher`).getAttribute(`title`)).toBe(
      `Switch run`
    )
  })

  it(`the caret needs a run item to hang from`, () => {
    render(
      <WorkFaceStrip
        face="issue"
        items={[
          { face: `issue`, label: `Issue`, onSelect: vi.fn() },
          { face: `diff`, label: <ChangesFaceLabel counts={null} />, onSelect: vi.fn() },
        ]}
        runMenu={{ content: <DropdownMenuContent /> }}
      />
    )
    expect(screen.queryByTestId(`issue-run-switcher`)).toBeNull()
    expect(
      Array.from(document.querySelectorAll(`[data-face]`)).map(
        (node) => node.textContent
      )
    ).toEqual([`Issue`, `Changes`])
  })
})
