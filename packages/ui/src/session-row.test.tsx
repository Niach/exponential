import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import listItem from "@exp/domain-contract/fixtures/list-item.json"
import { SessionRow } from "./session-row"
import { treeGuides } from "./tree-guides"

// EXP-1248: THE session row. Every row leads with the run mark at
// base + indent·depth, carries no fold or trailing chevron and no button, and
// a big row adds the toned caption under the title.

const { geometry } = listItem

function Device({ className }: { className?: string }) {
  return <svg data-testid="device-glyph" className={className} />
}

/** The row's direct children, minus the connector layer. */
function slotsOf(row: HTMLElement): (string | null)[] {
  return Array.from(row.children)
    .filter((child) => child.getAttribute(`data-testid`) !== `tree-guides`)
    .map((child) => child.getAttribute(`data-slot`))
}

describe(`SessionRow`, () => {
  it(`leads with the mark at base + indent·depth on every row`, () => {
    const depths = [0, 0, 1, 1]
    const guides = treeGuides(depths)
    render(
      <div>
        {depths.map((depth, index) => (
          <SessionRow
            key={index}
            data-testid={`row-${index}`}
            agent="claude"
            title={`Run ${index}`}
            depth={depth}
            guide={guides[index]}
          />
        ))}
      </div>
    )
    depths.forEach((depth, index) => {
      const row = screen.getByTestId(`row-${index}`)
      expect(row.style.paddingLeft).toBe(`${geometry.base + geometry.indent * depth}px`)
      expect(slotsOf(row)[0]).toBe(`run-mark`)
    })
  })

  it(`never draws a chevron or a button of its own`, () => {
    render(
      <SessionRow
        data-testid="row"
        agent="claude"
        identifier="EXP-1"
        title="Ship it"
        caption="Building · mint · 5 min"
        deviceIcon={Device}
        onClick={() => {}}
      />
    )
    const row = screen.getByTestId(`row`)
    expect(row.querySelector(`[data-icon="ui-chevron-right"]`)).toBeNull()
    expect(row.querySelector(`[data-icon="ui-chevron-down"]`)).toBeNull()
    expect(row.querySelector(`button`)).toBeNull()
    expect(slotsOf(row)).toEqual([`run-mark`, `session-row-text`, `session-row-device`])
  })

  it(`small is one line at the small height, big adds the toned caption`, () => {
    const { unmount } = render(
      <SessionRow
        data-testid="row"
        size="small"
        agent="claude"
        title="Ship it"
        caption="Building · mint · 5 min"
      />
    )
    let row = screen.getByTestId(`row`)
    expect(row.className).toContain(`h-8`)
    expect(geometry.small).toBe(32)
    expect(row.querySelector(`[data-slot="session-row-caption"]`)).toBeNull()
    unmount()

    render(
      <SessionRow
        data-testid="row"
        size="big"
        agent="claude"
        title="Ship it"
        caption="Needs input · mint · 5 min"
        captionTone="amber"
      />
    )
    row = screen.getByTestId(`row`)
    expect(row.className).toContain(`h-[${geometry.big}px]`)
    const caption = row.querySelector(`[data-slot="session-row-caption"]`)!
    expect(caption.textContent).toBe(`Needs input · mint · 5 min`)
    expect(caption.className).toContain(`text-amber-400`)
  })

  it(`wears the mark state and opens on click`, () => {
    const onClick = vi.fn()
    render(
      <SessionRow
        data-testid="row"
        agent="claude"
        markState="needs_input"
        title="Ship it"
        onClick={onClick}
        active
      />
    )
    const row = screen.getByTestId(`row`)
    expect(row.querySelector(`[data-slot="run-mark"]`)?.getAttribute(`data-state`)).toBe(`needs_input`)
    expect(row.className).toContain(`bg-glass-active`)
    fireEvent.click(row)
    expect(onClick).toHaveBeenCalledOnce()
  })

  it(`puts the device icon at the trailing edge`, () => {
    render(
      <SessionRow data-testid="row" agent="codex" title="Ship it" deviceIcon={Device} deviceName="mint" />
    )
    const row = screen.getByTestId(`row`)
    expect(slotsOf(row).at(-1)).toBe(`session-row-device`)
    expect(screen.getByTestId(`device-glyph`)).toBeTruthy()
  })
})
