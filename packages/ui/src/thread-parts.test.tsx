import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  ThreadFoldedRun,
  ThreadLinkRow,
  ThreadParallelCard,
  ThreadRunHeader,
  ThreadStrip,
  type ThreadStripEntry,
} from "./thread-parts"

// EXP-1284: the thread's presentational parts — the child strip's order and
// marks, a block header's state word, the folded run's Show work, the link
// row and the parallel card's rows.

describe(`ThreadStrip`, () => {
  const entries = (onSelect = vi.fn()): ThreadStripEntry[] => [
    { kind: `conversation`, active: true, onSelect },
    {
      kind: `issue`,
      key: `a`,
      identifier: `EXP-1276`,
      title: `ios keyboard`,
      lead: null,
      active: false,
      fresh: true,
      onSelect,
    },
    { kind: `guide`, active: false, dot: true, onSelect },
  ]

  it(`reads Conversation · the issues · + · Guide, the caption trailing`, () => {
    render(<ThreadStrip entries={entries()} caption="1 run · macbook" onAdd={() => {}} />)
    const strip = screen.getByTestId(`thread-strip`)
    expect(strip.textContent).toBe(`ConversationEXP-1276ios keyboardGuide1 run · macbook`)
    expect(screen.getByTestId(`thread-child-conversation`).getAttribute(`aria-selected`)).toBe(`true`)
    expect(screen.getByTestId(`thread-child-EXP-1276`).getAttribute(`data-fresh`)).toBe(`true`)
    expect(screen.getByTestId(`thread-guide-dot`)).toBeTruthy()
    expect(screen.getByTestId(`thread-strip-add`)).toBeTruthy()
  })

  it(`is the pager on phones: identifiers only, the "+" trailing, no caption`, () => {
    render(<ThreadStrip entries={entries()} caption="1 run" variant="pager" />)
    const strip = screen.getByTestId(`thread-strip`)
    expect(strip.textContent).toBe(`ConversationEXP-1276Guide`)
    expect(strip.lastElementChild?.getAttribute(`data-testid`)).toBe(`thread-strip-add`)
  })

  it(`selects a child`, () => {
    const onSelect = vi.fn()
    render(<ThreadStrip entries={entries(onSelect)} />)
    fireEvent.click(screen.getByTestId(`thread-child-guide`))
    expect(onSelect).toHaveBeenCalledTimes(1)
  })
})

describe(`the conversation parts`, () => {
  it(`heads a block with its label, caption parts and state word`, () => {
    render(
      <ThreadRunHeader lead={null} label="Run 2" caption={[`macbook`, `resumed`]} state="needs_input" />
    )
    const header = screen.getByTestId(`thread-run-header`)
    expect(header.textContent).toBe(`Run 2 · macbook · resumedneeds input`)
    expect(header.getAttribute(`data-state`)).toBe(`needs_input`)
  })

  it(`folds an ended run to its name + Show work`, () => {
    const onShowWork = vi.fn()
    render(<ThreadFoldedRun name="Plan the batch" onShowWork={onShowWork} />)
    fireEvent.click(screen.getByRole(`button`, { name: `Show work` }))
    expect(onShowWork).toHaveBeenCalled()
  })

  it(`labels a link row and draws one card row per parallel run`, () => {
    const onOpen = vi.fn()
    render(
      <>
        <ThreadLinkRow lead={null} label="Filed and linked" chip={<span>EXP-1277</span>} />
        <ThreadParallelCard
          rows={[
            { key: `r3`, lead: null, name: `Threads schema`, meta: `mint · 22m`, onOpen },
            { key: `r4`, lead: null, name: `Getting started`, meta: `macbook · 9m` },
          ]}
        />
      </>
    )
    expect(screen.getByTestId(`thread-link-row`).textContent).toBe(`Filed and linkedEXP-1277`)
    const rows = screen.getAllByTestId(`thread-parallel-row`)
    expect(rows.map((row) => row.textContent)).toEqual([
      `Threads schemamint · 22m`,
      `Getting startedmacbook · 9m`,
    ])
    fireEvent.click(rows[0]!)
    expect(onOpen).toHaveBeenCalled()
  })
})
