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

  // EXP-1162: the tabs carry the state — the title never does. The Run tab
  // wears the agent's brand mark (never a dot), an open PR dots Results.
  it(`marks the Run tab with the agent and dots the open pull request`, () => {
    render(
      <WorkFaceStrip
        face="issue"
        dots={{ run: `needs_input`, results: `review` }}
        run={{ agent: `codex`, state: `needs_input` }}
        items={[
          { face: `issue`, label: `Issue`, onSelect: vi.fn() },
          { face: `run`, label: `Run`, onSelect: vi.fn() },
          { face: `results`, label: `Results`, onSelect: vi.fn() },
        ]}
      />
    )
    const mark = screen.getByTestId(`face-run-mark`)
    expect(mark.closest(`[data-face]`)?.getAttribute(`data-face`)).toBe(`run`)
    expect(mark.dataset.tone).toBe(`needs_input`)
    expect(mark.getAttribute(`aria-label`)).toBe(`Needs input`)
    // The mark LEADS the label and carries the sidebar row's amber badge.
    expect(mark.nextSibling?.textContent).toBe(`Run`)
    expect(mark.querySelector(`.bg-amber-500`)).not.toBeNull()
    const dots = screen.getAllByTestId(`face-dot`)
    expect(dots.map((dot) => dot.dataset.tone)).toEqual([`review`])
    expect(dots[0]?.closest(`[data-face]`)?.getAttribute(`data-face`)).toBe(
      `results`
    )
    expect(dots[0]?.getAttribute(`aria-label`)).toBe(`Pull request open`)
  })

  // EXP-1184: a working run wears the working mark, never a badge.
  it(`a working run shows the working mark without a badge`, () => {
    render(
      <WorkFaceStrip
        face="run"
        dots={{ run: `running` }}
        run={{ agent: `claude`, state: `working` }}
        items={[
          { face: `issue`, label: `Issue`, onSelect: vi.fn() },
          { face: `run`, label: `Run`, onSelect: vi.fn() },
        ]}
      />
    )
    const mark = screen.getByTestId(`face-run-mark`)
    expect(mark.getAttribute(`aria-label`)).toBe(`Working`)
    expect(mark.querySelector(`[data-slot="claude-spinner"]`)).not.toBeNull()
    expect(mark.querySelector(`[data-slot="run-mark-badge"]`)).toBeNull()
    expect(screen.queryByTestId(`face-dot`)).toBeNull()
  })

  it(`a finished run with an open PR wears the emerald badge`, () => {
    render(
      <WorkFaceStrip
        face="run"
        dots={{ run: `running` }}
        run={{ agent: `claude`, state: `review` }}
        items={[
          { face: `issue`, label: `Issue`, onSelect: vi.fn() },
          { face: `run`, label: `Run`, onSelect: vi.fn() },
        ]}
      />
    )
    const mark = screen.getByTestId(`face-run-mark`)
    expect(mark.querySelector(`.bg-emerald-500`)).not.toBeNull()
    expect(mark.querySelector(`[data-slot="claude-spinner"]`)).toBeNull()
  })
})
