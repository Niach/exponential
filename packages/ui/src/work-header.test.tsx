import { createRef } from "react"
import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { CollapsedTitle } from "./detail-chrome"
import { WORK_BAR_HEIGHT, WORK_COLUMN_CLASS, WorkHeader } from "./work-header"

// EXP-877: the ONE work header the issue face and the run face share.
// EXP-1162: a compact bar — always collapsed on a run face, floating over the
// issue face's scroller and breaking into the collapsed title there.

describe(`WorkHeader`, () => {
  it(`renders the collapsed title, the trailing cluster and the tray`, () => {
    render(
      <WorkHeader
        title={<CollapsedTitle identifier="EXP-961" title="One header" />}
        trailing={<button data-testid="trailing">T</button>}
        tray={<div data-testid="tray">tray</div>}
      />
    )
    expect(screen.getByText(`EXP-961`)).toBeTruthy()
    expect(screen.getByText(`One header`)).toBeTruthy()
    expect(screen.getByTestId(`trailing`)).toBeTruthy()
    expect(screen.getByTestId(`tray`)).toBeTruthy()
  })

  it(`hides the title and the edge layer until the title row scrolled away`, () => {
    const { rerender } = render(
      <WorkHeader floating collapsed={false} title={<span>EXP-961</span>} />
    )
    const header = screen.getByTestId(`work-header`)
    expect(screen.queryByText(`EXP-961`)).toBeNull()
    expect(header.querySelector(`.glass-edge-top-card`)?.className).toContain(
      `opacity-0`
    )
    rerender(<WorkHeader floating collapsed title={<span>EXP-961</span>} />)
    expect(screen.getByText(`EXP-961`)).toBeTruthy()
    expect(
      header.querySelector(`.glass-edge-top-card`)?.className
    ).not.toContain(`opacity-0`)
  })

  // A floating bar covers the title row's first line: it takes no height and
  // only its cluster takes the pointer.
  it(`floats over the scroller without taking height or the pointer`, () => {
    render(
      <WorkHeader
        floating
        collapsed={false}
        title="EXP-961"
        trailing={<button data-testid="trailing">T</button>}
      />
    )
    const header = screen.getByTestId(`work-header`)
    expect(header.className).toContain(`pointer-events-none`)
    expect(header.parentElement?.className).toContain(`sticky`)
    expect(header.parentElement?.className).toContain(`h-0`)
    expect(screen.getByTestId(`trailing`).parentElement?.className).toContain(
      `pointer-events-auto`
    )
  })

  it(`forwards the ref and keeps the shared reading column`, () => {
    const ref = createRef<HTMLDivElement>()
    render(<WorkHeader ref={ref} title="EXP-961" className="pt-2" />)
    const header = screen.getByTestId(`work-header`)
    expect(ref.current).toBe(header)
    expect(header.className).toContain(`pt-2`)
    const row = header.querySelector(`.${WORK_COLUMN_CLASS.split(/\s+/)[0]}`)
    expect(row?.className).toContain(`h-12`)
    expect(WORK_BAR_HEIGHT).toBe(48)
  })
})

describe(`CollapsedTitle`, () => {
  it(`stacks the mono identifier over the title on one truncated line`, () => {
    render(<CollapsedTitle identifier="EXP-1162" title="Header blur" />)
    expect(screen.getByText(`EXP-1162`).parentElement?.className).toContain(
      `font-mono`
    )
    expect(screen.getByText(`Header blur`).className).toContain(`truncate`)
  })

  it(`an issue-less run shows its title alone`, () => {
    render(<CollapsedTitle title="Chat" animate={false} />)
    const node = screen.getByTestId(`collapsed-title`)
    expect(node.querySelector(`.font-mono`)).toBeNull()
    expect(node.className).not.toContain(`animate-in`)
  })
})
