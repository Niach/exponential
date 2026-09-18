import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { IssueGroupBand } from "./issue-group-band"

// EXP-862/EXP-961: ONE group band — the board's big list and the sidebar's
// issue lists draw it identically, only the density differs.

const glyph = { icon: `circle-dashed`, colorClass: `text-muted-foreground` } as const

const band = (container: HTMLElement) =>
  container.querySelector(`[data-slot="issue-group-header"]`) as HTMLElement

describe(`IssueGroupBand`, () => {
  it(`names the group, counts it and folds the whole strip`, () => {
    const onToggle = vi.fn()
    render(
      <IssueGroupBand
        glyph={glyph}
        name="Backlog"
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

  it(`turns the chevron with the fold`, () => {
    const { container, rerender } = render(
      <IssueGroupBand
        glyph={glyph}
        name="Backlog"
        count={0}
        open={false}
        onToggle={vi.fn()}
      />
    )
    const chevron = () => container.querySelectorAll(`svg`)[0]!
    expect(chevron().getAttribute(`class`)).not.toContain(`rotate-90`)
    rerender(
      <IssueGroupBand
        glyph={glyph}
        name="Backlog"
        count={0}
        open
        onToggle={vi.fn()}
      />
    )
    expect(chevron().getAttribute(`class`)).toContain(`rotate-90`)
  })

  it(`hangs the group's own action outside the fold button`, () => {
    render(
      <IssueGroupBand
        glyph={glyph}
        name="Backlog"
        count={1}
        open
        onToggle={vi.fn()}
        trailing={<button type="button">New issue in Backlog</button>}
      />
    )
    const action = screen.getByRole(`button`, { name: `New issue in Backlog` })
    expect(action.closest(`[aria-expanded]`)).toBeNull()
  })

  it(`pins the list density and rounds the compact one`, () => {
    const { container, rerender } = render(
      <IssueGroupBand
        glyph={glyph}
        name="Backlog"
        count={1}
        open
        onToggle={vi.fn()}
      />
    )
    expect(band(container).className).toContain(`md:sticky`)
    expect(band(container).className).toContain(`md:backdrop-blur-md`)
    rerender(
      <IssueGroupBand
        glyph={glyph}
        name="Backlog"
        count={1}
        open
        onToggle={vi.fn()}
        density="compact"
      />
    )
    expect(band(container).className).toContain(`rounded-md`)
    expect(band(container).className).not.toContain(`md:sticky`)
  })

  it(`takes the wash as a class, as an inline fill, or not at all`, () => {
    const { container, rerender } = render(
      <IssueGroupBand
        glyph={glyph}
        name="Backlog"
        count={1}
        open
        onToggle={vi.fn()}
        wash={{ className: `bg-zinc-500/10` }}
      />
    )
    expect(band(container).className).toContain(`bg-zinc-500/10`)
    rerender(
      <IssueGroupBand
        glyph={glyph}
        name="Triage"
        count={1}
        open
        onToggle={vi.fn()}
        wash={{ className: ``, style: { backgroundColor: `rgba(255, 136, 0, 0.1)` } }}
      />
    )
    expect(band(container).style.backgroundColor).toBe(`rgba(255, 136, 0, 0.1)`)
    rerender(
      <IssueGroupBand
        glyph={glyph}
        name="Backlog"
        count={1}
        open
        onToggle={vi.fn()}
      />
    )
    expect(band(container).className).not.toContain(`bg-zinc-500/10`)
    expect(band(container).style.backgroundColor).toBe(``)
  })

  it(`paints the status glyph with the row's own color`, () => {
    const { container } = render(
      <IssueGroupBand
        glyph={{ icon: `circle-dashed`, colorHex: `#ff8800` }}
        name="Triage"
        count={1}
        open
        onToggle={vi.fn()}
      />
    )
    const status = container.querySelectorAll(`svg`)[1]!
    expect(status.getAttribute(`class`)).toContain(`size-3.5`)
    expect((status as unknown as HTMLElement).style.color).toBe(
      `rgb(255, 136, 0)`
    )
  })
})
