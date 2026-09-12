import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import {
  IssueGroupHeader,
  statusGroupWash,
} from "@/components/issue-group-header"
import type { StatusRowOption } from "@/lib/team-statuses"

// EXP-862: ONE group band for the board's big list and the sidebar's issue
// lists — glyph, name, count over the status tint, the whole strip folding
// its group.

const builtin: StatusRowOption = {
  id: `row-backlog`,
  name: `Backlog`,
  colorHex: `#71717a`,
  category: `backlog`,
  builtinKey: `backlog`,
  sortOrder: 1,
  icon: `circle-dashed`,
}

const custom: StatusRowOption = {
  ...builtin,
  id: `row-triage`,
  name: `Triage`,
  colorHex: `#ff8800`,
  builtinKey: null,
}

describe(`statusGroupWash`, () => {
  it(`keeps the builtin classes and washes a custom row from its own hex`, () => {
    expect(statusGroupWash(builtin).className).toBe(`bg-zinc-500/10`)
    expect(statusGroupWash(builtin).style).toBeUndefined()
    expect(statusGroupWash(custom).className).toBe(``)
    expect(statusGroupWash(custom).style?.backgroundColor).toBe(
      `rgba(255, 136, 0, 0.1)`
    )
  })
})

describe(`IssueGroupHeader`, () => {
  it(`names the group, counts it and states the fold`, () => {
    const onToggle = vi.fn()
    render(
      <IssueGroupHeader
        status={builtin}
        count={7}
        open
        onToggle={onToggle}
      />
    )
    const fold = screen.getByRole(`button`, { expanded: true })
    expect(fold.textContent).toContain(`Backlog`)
    expect(fold.textContent).toContain(`7`)
    fireEvent.click(fold)
    expect(onToggle).toHaveBeenCalledTimes(1)
  })

  it(`renders a folded group as collapsed`, () => {
    render(
      <IssueGroupHeader
        status={builtin}
        count={0}
        open={false}
        onToggle={vi.fn()}
      />
    )
    expect(screen.getByRole(`button`, { expanded: false })).toBeTruthy()
  })

  it(`drops the tint when the caller asks (a phone's plain header)`, () => {
    const { container, rerender } = render(
      <IssueGroupHeader status={builtin} count={1} open onToggle={vi.fn()} />
    )
    const band = () =>
      container.querySelector(`[data-slot="issue-group-header"]`)!
    expect(band().className).toContain(`bg-zinc-500/10`)
    rerender(
      <IssueGroupHeader
        status={builtin}
        count={1}
        open
        onToggle={vi.fn()}
        tinted={false}
      />
    )
    expect(band().className).not.toContain(`bg-zinc-500/10`)
  })

  it(`hangs the group's own action outside the fold button`, () => {
    render(
      <IssueGroupHeader
        status={builtin}
        count={1}
        open
        onToggle={vi.fn()}
        trailing={<button type="button">New issue in Backlog</button>}
      />
    )
    const action = screen.getByRole(`button`, { name: `New issue in Backlog` })
    expect(action.closest(`[aria-expanded]`)).toBeNull()
  })
})
