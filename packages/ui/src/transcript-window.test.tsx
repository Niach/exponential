import { useRef } from "react"
import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/chats.json"
import { LoadEarlierButton, StableRow, sameData, useTranscriptWindow } from "./transcript-window"

// #EXP-1380 (chats.json WINDOW + PERF): the newest `transcript.window` rows,
// Load earlier widens by `windowStep`, a new arrival never shrinks it; a row
// re-renders only when its own data changes.

const { window: WINDOW, windowStep: STEP } = fixture.transcript
const make = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `m${i}`, body: `${i}` }))

function Harness({ rows }: { rows: { id: string; body: string }[] }) {
  const ref = useRef<HTMLDivElement | null>(null)
  const win = useTranscriptWindow(rows, ref)
  return (
    <div ref={ref}>
      {win.canLoadEarlier ? <LoadEarlierButton onClick={win.loadEarlier} /> : null}
      <span data-testid="first">{win.rows[0]?.id}</span>
      <span data-testid="count">{win.rows.length}</span>
    </div>
  )
}

describe(`useTranscriptWindow`, () => {
  it(`paints the newest window and widens by one step`, () => {
    const rows = make(WINDOW + STEP + 50)
    render(<Harness rows={rows} />)
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW))
    const button = screen.getByTestId(fixture.testIds.loadEarlier)
    expect(button.textContent).toBe(fixture.copy.loadEarlier)
    fireEvent.click(button)
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW + STEP))
    fireEvent.click(screen.getByTestId(fixture.testIds.loadEarlier))
    expect(screen.getByTestId(`count`).textContent).toBe(String(rows.length))
    expect(screen.queryByTestId(fixture.testIds.loadEarlier)).toBeNull()
  })

  it(`never shrinks on a new arrival`, () => {
    const rows = make(WINDOW + 10)
    const { rerender } = render(<Harness rows={rows} />)
    const first = screen.getByTestId(`first`).textContent
    rerender(<Harness rows={[...rows, { id: `new`, body: `new` }]} />)
    expect(screen.getByTestId(`first`).textContent).toBe(first)
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW + 1))
  })

  it(`shows everything without a button under the window`, () => {
    render(<Harness rows={make(3)} />)
    expect(screen.getByTestId(`count`).textContent).toBe(`3`)
    expect(screen.queryByTestId(fixture.testIds.loadEarlier)).toBeNull()
  })
})

describe(`StableRow`, () => {
  it(`re-renders only when its row data or deps change`, () => {
    let renders = 0
    const render1 = (row: { id: string; body: string }) => {
      renders += 1
      return <span>{row.body}</span>
    }
    const { rerender } = render(<StableRow row={{ id: `a`, body: `x` }} deps={[1]} render={render1} />)
    expect(renders).toBe(1)
    // A rebuilt but equal row (a fresh object) and a fresh render closure.
    act(() => rerender(<StableRow row={{ id: `a`, body: `x` }} deps={[1]} render={(row) => render1(row)} />))
    expect(renders).toBe(1)
    act(() => rerender(<StableRow row={{ id: `a`, body: `y` }} deps={[1]} render={render1} />))
    expect(renders).toBe(2)
    act(() => rerender(<StableRow row={{ id: `a`, body: `y` }} deps={[2]} render={render1} />))
    expect(renders).toBe(3)
  })

  it(`compares plain data deeply`, () => {
    expect(sameData({ a: [1, { b: `c` }] }, { a: [1, { b: `c` }] })).toBe(true)
    expect(sameData({ a: [1] }, { a: [1, 2] })).toBe(false)
    expect(sameData({ a: null }, { a: undefined })).toBe(false)
  })
})
