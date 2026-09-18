import { createRef } from "react"
import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import {
  DETAIL_STICKY_BAND_CLASS,
  WORK_COLUMN_CLASS,
  WorkHeader,
} from "./work-header"

// EXP-877: the ONE work header the issue face and the run face share — three
// slots and the sticky band, so the title never moves when the face flips.

describe(`WorkHeader`, () => {
  it(`renders the title, the trailing cluster and the tray`, () => {
    render(
      <WorkHeader
        title={<h1 data-testid="title">EXP-961</h1>}
        trailing={<button data-testid="trailing">T</button>}
        tray={<div data-testid="tray">tray</div>}
      />
    )
    expect(screen.getByTestId(`title`)).toBeTruthy()
    expect(screen.getByTestId(`trailing`)).toBeTruthy()
    expect(screen.getByTestId(`tray`)).toBeTruthy()
  })

  it(`drops the trailing wrapper entirely when there is no cluster`, () => {
    render(<WorkHeader title={<h1 data-testid="title">EXP-961</h1>} />)
    const header = screen.getByTestId(`work-header`)
    expect(header.querySelector(`.pt-4`)).toBeNull()
  })

  // The issue editor measures this node to inset its own scroller.
  it(`forwards the ref to the measured band`, () => {
    const ref = createRef<HTMLDivElement>()
    render(<WorkHeader ref={ref} title="EXP-961" />)
    expect(ref.current).toBe(screen.getByTestId(`work-header`))
  })

  it(`wears the sticky band and the shared reading column`, () => {
    render(<WorkHeader title="EXP-961" className="pt-2" />)
    const header = screen.getByTestId(`work-header`)
    for (const token of DETAIL_STICKY_BAND_CLASS.split(/\s+/)) {
      expect(header.className).toContain(token)
    }
    expect(header.className).toContain(`pt-2`)
    expect(header.querySelector(`.${WORK_COLUMN_CLASS.split(/\s+/)[0]}`)).not
      .toBeNull()
  })
})
