import { useRef, useState } from "react"
import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/chats.json"
import { LoadEarlierButton, StableRow, sameData, useTranscriptWindow } from "./transcript-window"

// #EXP-1380 (chats.json WINDOW + PERF): the newest `transcript.window` rows,
// Load earlier widens by `windowStep`, a new arrival never shrinks it; a row
// re-renders only when its own data changes.

const { window: WINDOW, windowStep: STEP } = fixture.transcript
const make = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `m${i}`, body: `${i}` }))

type Row = { id: string; body: string; type?: string; answered?: boolean }

function Harness({ rows, resetKey }: { rows: Row[]; resetKey?: string }) {
  const ref = useRef<HTMLDivElement | null>(null)
  const win = useTranscriptWindow(rows, ref, { resetKey })
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

  it(`reaches up to an older waiting card`, () => {
    const rows: Row[] = make(WINDOW + 150)
    rows[30] = { id: `q`, body: `q`, type: `question`, answered: false }
    render(<Harness rows={rows} />)
    expect(screen.getByTestId(`first`).textContent).toBe(`q`)
    expect(screen.getByTestId(`count`).textContent).toBe(String(rows.length - 30))
  })

  it(`keeps the pinned start when an answered card or an arrival changes the rows`, () => {
    const rows: Row[] = make(WINDOW + 150)
    rows[30] = { id: `q`, body: `q`, type: `question`, answered: false }
    const { rerender } = render(<Harness rows={rows} />)
    const answered = rows.map((row) => (row.id === `q` ? { ...row, answered: true } : row))
    rerender(<Harness rows={[...answered, { id: `new`, body: `new` }]} />)
    expect(screen.getByTestId(`first`).textContent).toBe(`q`)
  })

  it(`restarts at the newest window on a reopen`, () => {
    const rows = make(WINDOW + 10)
    const { rerender } = render(<Harness rows={rows} resetKey="a" />)
    fireEvent.click(screen.getByTestId(fixture.testIds.loadEarlier))
    expect(screen.getByTestId(`count`).textContent).toBe(String(rows.length))
    rerender(<Harness rows={[...rows, ...make(5).map((r) => ({ ...r, id: `x${r.id}` }))]} resetKey="b" />)
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW))
  })

  it(`shows everything without a button under the window`, () => {
    render(<Harness rows={make(3)} />)
    expect(screen.getByTestId(`count`).textContent).toBe(`3`)
    expect(screen.queryByTestId(fixture.testIds.loadEarlier)).toBeNull()
  })
})

// Review of #1218: a busy tree's message pages are mostly its children's
// messages (no row of their own), so the loaded rows can fall short of one
// step; ONE press pages messages in until the step is filled.
function Paged({ total, perPage, rowsPerPage }: { total: number; perPage: number; rowsPerPage: number }) {
  const ref = useRef<HTMLDivElement | null>(null)
  const [pages, setPages] = useState(1)
  const loaded = Math.min(total, pages * perPage)
  // Every message page holds `rowsPerPage` rows (the rest are children's).
  const rowCount = Math.ceil((loaded / perPage) * rowsPerPage)
  const all = make(Math.ceil((total / perPage) * rowsPerPage))
  const rows = all.slice(all.length - rowCount)
  const win = useTranscriptWindow(rows, ref, {
    more: loaded < total,
    loaded,
    loadMore: () => setPages((n) => n + 1),
  })
  return (
    <div ref={ref}>
      {win.canLoadEarlier ? <LoadEarlierButton onClick={win.loadEarlier} /> : null}
      <span data-testid="count">{win.rows.length}</span>
      <span data-testid="pages">{pages}</span>
    </div>
  )
}

describe(`Load earlier over on-demand messages`, () => {
  it(`shows one full step of earlier rows in one press`, async () => {
    // 200 messages loaded = 120 rows: the window (100) plus 20 above.
    render(<Paged total={2_000} perPage={200} rowsPerPage={120} />)
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW))
    await act(async () => fireEvent.click(screen.getByTestId(fixture.testIds.loadEarlier)))
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW + STEP))
  })

  it(`pages through messages that paint no row`, async () => {
    // A page of 200 messages adds only 10 rows (children's work).
    render(<Paged total={6_000} perPage={200} rowsPerPage={10} />)
    // The first paint pages in until the window fills.
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW))
    await act(async () => fireEvent.click(screen.getByTestId(fixture.testIds.loadEarlier)))
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW + STEP))
  })

  it(`stops at the oldest message`, async () => {
    // 200 of 300 messages = 120 of 180 rows.
    render(<Paged total={300} perPage={200} rowsPerPage={120} />)
    expect(screen.getByTestId(`count`).textContent).toBe(String(WINDOW))
    await act(async () => fireEvent.click(screen.getByTestId(fixture.testIds.loadEarlier)))
    expect(screen.getByTestId(`count`).textContent).toBe(`180`)
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
