import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/chats.json"

import { PhoneBoardMenuStrip, phoneBoardMenuHeight } from "./phone-board-menu"
import { PhoneTab, PhoneTabBar } from "./phone-tab-bar"

// #EXP-1384 (chats.json `shell.bar.drag`, `shell.boardMenu`): the drag lens
// selects the tab it is lifted on, a lift far off the bar cancels, a plain
// tap stays the tab's own click; the board menu lists the rows and closes.

// jsdom lays nothing out: each tab gets a 60px-wide box side by side.
function layout(nav: HTMLElement) {
  nav.getBoundingClientRect = () => ({ left: 0, right: 248, top: 700, bottom: 752, width: 248, height: 52, x: 0, y: 700, toJSON: () => ({}) })
  ;[...nav.querySelectorAll<HTMLElement>(`[data-slot=phone-tab]`)].forEach((tab, index) => {
    const left = 4 + index * 60
    tab.getBoundingClientRect = () => ({ left, right: left + 60, top: 704, bottom: 748, width: 60, height: 44, x: left, y: 704, toJSON: () => ({}) })
  })
}

function renderBar() {
  const clicks: string[] = []
  render(
    <PhoneTabBar>
      {[`board`, `chats`, `apps`, `team`].map((tab) => (
        <PhoneTab key={tab} label={tab} icon={<svg />} selected={tab === `board`} onClick={() => clicks.push(tab)} />
      ))}
    </PhoneTabBar>
  )
  const nav = screen.getByRole(`navigation`)
  layout(nav)
  return { nav, clicks }
}

function pointer(type: string, x: number, y: number, target: EventTarget = window) {
  const event = new MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0 })
  Object.assign(event, { pointerId: 1 })
  act(() => {
    target.dispatchEvent(event)
  })
}

describe(`PhoneTabBar drag`, () => {
  it(`selects the tab the lens is lifted on`, () => {
    const { clicks } = renderBar()
    const board = screen.getByLabelText(`board`)
    pointer(`pointerdown`, 30, 720, board)
    expect(document.querySelector(`[data-slot=phone-tab-lens]`)).toBeTruthy()
    pointer(`pointermove`, 100, 722)
    pointer(`pointermove`, 160, 722)
    expect(screen.getByLabelText(`apps`).dataset.candidate).toBe(`true`)
    pointer(`pointerup`, 160, 722)
    expect(clicks).toEqual([`apps`])
    // The browser's own click after a drag is eaten.
    fireEvent.click(board)
    expect(clicks).toEqual([`apps`])
  })

  it(`cancels a lift farther than cancelDistance off the bar`, () => {
    const { clicks } = renderBar()
    pointer(`pointerdown`, 30, 720, screen.getByLabelText(`board`))
    pointer(`pointermove`, 160, 720)
    pointer(`pointerup`, 160, 700 - fixture.shell.bar.drag.cancelDistance - 10)
    expect(clicks).toEqual([])
  })

  it(`leaves a plain tap to the tab's own click`, () => {
    const { clicks } = renderBar()
    const chats = screen.getByLabelText(`chats`)
    pointer(`pointerdown`, 90, 720, chats)
    pointer(`pointerup`, 90, 720)
    expect(clicks).toEqual([])
    fireEvent.click(chats)
    expect(clicks).toEqual([`chats`])
  })
})

describe(`PhoneBoardMenuStrip`, () => {
  it(`lists the boards, the active one checked, then New board; Escape closes`, () => {
    const onOpenChange = vi.fn()
    const onSelect = vi.fn()
    const onNewBoard = vi.fn()
    render(
      <PhoneBoardMenuStrip
        open
        onOpenChange={onOpenChange}
        rows={[
          { id: `a`, name: `Web`, glyph: <svg />, active: true },
          { id: `b`, name: `iOS`, glyph: <svg /> },
        ]}
        onSelect={onSelect}
        onNewBoard={onNewBoard}
      >
        <span>strip</span>
      </PhoneBoardMenuStrip>
    )
    const rows = screen.getAllByTestId(fixture.testIds.boardMenuRow)
    expect(rows.map((row) => row.textContent)).toEqual([`Web`, `iOS`, fixture.copy.boardMenu.newBoard])
    expect(rows[0]!.getAttribute(`aria-checked`)).toBe(`true`)
    fireEvent.click(rows[1]!)
    expect(onSelect).toHaveBeenCalledWith(`b`)
    expect(onOpenChange).toHaveBeenCalledWith(false)
    onOpenChange.mockReset()
    fireEvent.keyDown(window, { key: `Escape` })
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })

  it(`caps the panel at maxVisibleRows`, () => {
    const { rowHeight, maxVisibleRows } = fixture.shell.boardMenu
    const pad = fixture.shell.bar.padding * 2
    expect(phoneBoardMenuHeight(3)).toBe(3 * rowHeight + pad)
    expect(phoneBoardMenuHeight(30)).toBe(maxVisibleRows * rowHeight + pad)
  })
})
