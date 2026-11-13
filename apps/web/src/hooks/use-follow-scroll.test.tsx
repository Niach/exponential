import { act, fireEvent, render, screen } from "@testing-library/react"
import { useEffect } from "react"
import { describe, expect, it } from "vitest"
import { JumpToBottomButton } from "@exp/ui"
import { useFollowScroll } from "@/hooks/use-follow-scroll"

// EXP-1316 (1n): a chat's Conversation scrolls like the session feed —
// opens at the newest row, follows while pinned, stays put once the reader
// scrolled up (THE jump button shows) and jumps back.

let height = 1000

function Feed({ rows, held = false }: { rows: string[]; held?: boolean }) {
  const follow = useFollowScroll([rows])
  useEffect(() => follow.hold(held), [held, follow.hold])
  return (
    <div>
      <div
        ref={(node) => {
          follow.scrollRef.current = node
          if (!node) return
          Object.defineProperty(node, `scrollHeight`, { configurable: true, get: () => height })
          Object.defineProperty(node, `clientHeight`, { configurable: true, get: () => 200 })
        }}
        onScroll={follow.onScroll}
        data-testid="scroller"
      >
        <div ref={follow.contentRef}>
          {rows.map((row) => (
            <p key={row}>{row}</p>
          ))}
        </div>
      </div>
      <JumpToBottomButton
        visible={!follow.atBottom}
        onClick={follow.jumpToBottom}
      />
    </div>
  )
}

describe(`useFollowScroll`, () => {
  it(`opens at the bottom, follows, holds once scrolled up, jumps back`, () => {
    height = 1000
    const { rerender } = render(<Feed rows={[`a`, `b`]} />)
    const scroller = screen.getByTestId(`scroller`)
    expect(scroller.scrollTop).toBe(1000)
    expect(screen.getByTestId(`jump-to-bottom`).getAttribute(`data-visible`)).toBe(`false`)

    // A new row lands while pinned: the view follows.
    height = 1200
    rerender(<Feed rows={[`a`, `b`, `c`]} />)
    expect(scroller.scrollTop).toBe(1200)

    // The reader scrolls up: the jump button shows, new rows do not move it.
    act(() => {
      scroller.scrollTop = 100
      fireEvent.scroll(scroller)
    })
    expect(screen.getByTestId(`jump-to-bottom`).getAttribute(`data-visible`)).toBe(`true`)
    height = 1400
    rerender(<Feed rows={[`a`, `b`, `c`, `d`]} />)
    expect(scroller.scrollTop).toBe(100)

    // The jump returns to the newest row and pins again.
    fireEvent.click(screen.getByTestId(`jump-to-bottom`))
    expect(scroller.scrollTop).toBe(1400)
    expect(screen.getByTestId(`jump-to-bottom`).getAttribute(`data-visible`)).toBe(`false`)
  })
  it(`a late scroll event after the content grew keeps following (#EXP-1407)`, () => {
    height = 1000
    render(<Feed rows={[`a`]} />)
    const scroller = screen.getByTestId(`scroller`)
    expect(scroller.scrollTop).toBe(1000)
    // The Dock band measures in: the content grows with no row change, then
    // the pin's own scroll event lands short of the new tail.
    height = 1300
    act(() => {
      fireEvent.scroll(scroller)
    })
    expect(screen.getByTestId(`jump-to-bottom`).getAttribute(`data-visible`)).toBe(`false`)
    expect(scroller.scrollTop).toBe(1300)
  })
  it(`a pending deep link holds the feed off its newest row (waiting-cards.json DEEP LINKS)`, () => {
    height = 1000
    const { rerender } = render(<Feed rows={[`a`]} />)
    const scroller = screen.getByTestId(`scroller`)
    rerender(<Feed rows={[`a`]} held />)
    // The link scrolls the card to the centre (down, toward the tail): the
    // feed does not re-pin it.
    act(() => {
      scroller.scrollTop = 700
      fireEvent.scroll(scroller)
    })
    expect(scroller.scrollTop).toBe(700)
    // A row arrives while it is pending: nothing drags the view down.
    height = 1200
    rerender(<Feed rows={[`a`, `b`]} held />)
    expect(scroller.scrollTop).toBe(700)
    // Settled: the reader stays where the link put them.
    height = 1400
    rerender(<Feed rows={[`a`, `b`, `c`]} />)
    expect(scroller.scrollTop).toBe(700)
  })
  it(`a mouse press in the feed (a drag-select) is never scrolled under`, () => {
    height = 1000
    const { rerender } = render(<Feed rows={[`a`]} />)
    const scroller = screen.getByTestId(`scroller`)
    expect(scroller.scrollTop).toBe(1000)
    fireEvent.pointerDown(screen.getByText(`a`), { pointerType: `mouse`, button: 0 })
    height = 1200
    rerender(<Feed rows={[`a`, `b`]} />)
    expect(scroller.scrollTop).toBe(1000)
    // Released: the next row follows again.
    fireEvent.pointerUp(document)
    height = 1400
    rerender(<Feed rows={[`a`, `b`, `c`]} />)
    expect(scroller.scrollTop).toBe(1400)
  })
})
