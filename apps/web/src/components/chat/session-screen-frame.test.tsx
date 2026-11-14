// #EXP-1732 (session-screen.json): THE session screen's frame: the header
// tabs (THE SegmentedControl, the muted counts; #FEED-239: no gear), the
// layout `sessionScreen()` decides (wide: the beside rule led by THE
// collapse button; phones: one pane).
import type { ReactNode } from "react"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { sessionScreen } from "@exp/domain-contract/session-screen"

const mocks = vi.hoisted(() => ({ isMobile: { current: false } }))

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => mocks.isMobile.current,
  MobileDetailHeader: ({
    title,
    menu,
    below,
    onBack,
  }: {
    title: ReactNode
    menu?: ReactNode
    below?: ReactNode
    onBack?: () => void
  }) => (
    <div data-testid="mobile-header">
      {onBack && <button type="button" data-testid="mobile-header-back" onClick={onBack} />}
      {title}
      {menu}
      {below}
    </div>
  ),
}))
vi.mock(`@/components/chat/session-body`, () => ({ BackStackButton: () => null }))

const { SessionScreenFrame } = await import(`@/components/chat/session-screen-frame`)

const counts = { issues: 2, sessions: 3, results: 0 }

function renderFrame(
  input: { kind: string; requested?: string | null; open?: string | null },
  props: { onTab?: (tab: string) => void; onCloseItem?: () => void } = {}
) {
  const value = sessionScreen({
    kind: input.kind,
    wide: !mocks.isMobile.current,
    requested: input.requested ?? null,
    open: input.open ?? null,
    counts,
  })
  return render(
    <SessionScreenFrame
      screen={value}
      testId="chat-page"
      avatar={<span data-testid="avatar" />}
      title="Fix the login"
      subtitle="idle · Alex's MacBook"
      onTab={props.onTab ?? (() => {})}
      chat={<div data-testid="chat-body" />}
      pane={<div data-testid="tab-body" />}
      item={value.open ? <div data-testid="item-body" /> : null}
      onCloseItem={props.onCloseItem}
      itemTitle="Settings"
    />
  )
}

beforeEach(() => {
  mocks.isMobile.current = false
})

describe(`the header`, () => {
  it(`draws THE tabs of the kind, each count muted after its label (0 = none)`, () => {
    renderFrame({ kind: `session` })
    const tabs = screen.getByTestId(`session-tabs`)
    const labels = [...tabs.querySelectorAll(`[data-tab]`)]
    expect(labels.map((node) => node.getAttribute(`data-tab`))).toEqual([`chat`, `issues`, `sessions`, `results`])
    expect(labels.map((node) => node.textContent)).toEqual([`Chat`, `Issues2`, `Sessions3`, `Results`])
    expect(tabs.querySelectorAll(`[data-slot="session-tab-count"]`)).toHaveLength(2)
    expect(screen.getByTestId(`chat-title`).textContent).toBe(`Fix the login`)
    expect(screen.getByTestId(`chat-header-subtitle`).textContent).toBe(`idle · Alex's MacBook`)
  })

  it(`an app: Chat · Preview · Data · Versions on phones; wide drops Chat (the chat is the left pane)`, () => {
    renderFrame({ kind: `app` })
    const wide = [...screen.getByTestId(`session-tabs`).querySelectorAll(`[data-tab]`)]
    expect(wide.map((node) => node.getAttribute(`data-tab`))).toEqual([`preview`, `data`, `versions`])
    cleanup()
    mocks.isMobile.current = true
    renderFrame({ kind: `app` })
    const phone = [...screen.getByTestId(`session-tabs`).querySelectorAll(`[data-tab]`)]
    expect(phone.map((node) => node.getAttribute(`data-tab`))).toEqual([`chat`, `preview`, `data`, `versions`])
  })

  it(`a wide app collapsed to its chat selects no segment`, () => {
    renderFrame({ kind: `app`, requested: `chat` })
    expect(screen.getByTestId(`chat-page`).getAttribute(`data-layout`)).toBe(`chat`)
    expect(screen.getByTestId(`session-tabs`).querySelector(`[aria-selected="true"]`)).toBeNull()
  })

  it(`no gear on any kind`, () => {
    for (const kind of [`session`, `agent`, `assistant`, `app`]) {
      renderFrame({ kind })
      expect(screen.queryByTestId(`session-gear`)).toBeNull()
      cleanup()
    }
  })

  it(`a pick reports the tab`, () => {
    const onTab = vi.fn()
    renderFrame({ kind: `session` }, { onTab })
    const issues = screen.getByTestId(`session-tabs`).querySelector(`[data-tab="issues"]`)!.closest(`button`)!
    fireEvent.mouseDown(issues, { button: 0 })
    fireEvent.click(issues)
    expect(onTab).toHaveBeenCalledWith(`issues`)
  })
})

describe(`the beside rule (wide)`, () => {
  it(`Chat alone = the chat full width`, () => {
    renderFrame({ kind: `session`, requested: `chat` })
    expect(screen.queryByTestId(`session-beside`)).toBeNull()
    expect(screen.getByTestId(`chat-body`)).toBeTruthy()
    expect(screen.queryByTestId(`tab-body`)).toBeNull()
  })

  it(`another tab = the chat pane beside the tab pane (5fr : 7fr, min 360px)`, () => {
    renderFrame({ kind: `session`, requested: `issues` })
    const beside = screen.getByTestId(`session-beside`)
    expect(beside.style.gridTemplateColumns).toBe(`minmax(360px, 5fr) 7fr`)
    expect(beside.querySelector(`[data-testid="chat-body"]`)).toBeTruthy()
    expect(screen.getByTestId(`session-pane`).querySelector(`[data-testid="tab-body"]`)).toBeTruthy()
  })

  it(`an app opens on Preview beside the chat`, () => {
    renderFrame({ kind: `app` })
    expect(screen.getByTestId(`session-beside`)).toBeTruthy()
    expect(screen.getByTestId(`chat-page`).getAttribute(`data-layout`)).toBe(`beside`)
  })

  it(`an opened item covers the pane, led by THE collapse button (no Back)`, () => {
    const onTab = vi.fn()
    renderFrame({ kind: `agent`, requested: `chat`, open: `settings` }, { onTab })
    const pane = screen.getByTestId(`session-pane`)
    expect(pane.querySelector(`[data-testid="item-body"]`)).toBeTruthy()
    expect(pane.querySelector(`[data-testid="tab-body"]`)).toBeNull()
    expect(screen.queryByTestId(`session-item-back`)).toBeNull()
    const collapse = screen.getByTestId(`session-collapse`)
    expect(collapse.getAttribute(`aria-label`)).toBe(`Collapse`)
    fireEvent.click(collapse)
    expect(onTab).toHaveBeenCalledWith(`chat`)
  })

  it(`a tab's pane leads with THE collapse button; Chat alone has none`, () => {
    renderFrame({ kind: `session`, requested: `issues` })
    expect(screen.getByTestId(`session-pane`).querySelector(`[data-testid="session-collapse"]`)).toBeTruthy()
    cleanup()
    renderFrame({ kind: `session`, requested: `chat` })
    expect(screen.queryByTestId(`session-collapse`)).toBeNull()
  })

  it(`pressing the ACTIVE tab closes the opened item`, () => {
    const onTab = vi.fn()
    renderFrame({ kind: `session`, requested: `sessions`, open: `session` }, { onTab })
    const active = screen.getByTestId(`session-tabs`).querySelector(`[data-tab="sessions"]`)!.closest(`button`)!
    fireEvent.click(active)
    expect(onTab).toHaveBeenCalledWith(`sessions`)
  })
})

describe(`phones (one pane)`, () => {
  beforeEach(() => {
    mocks.isMobile.current = true
  })

  it(`chat, then the tab replaces it, then an opened item over it`, () => {
    const { unmount } = renderFrame({ kind: `session` })
    expect(screen.getByTestId(`chat-page`).getAttribute(`data-layout`)).toBe(`chat`)
    expect(screen.getByTestId(`chat-body`)).toBeTruthy()
    unmount()
    const second = renderFrame({ kind: `session`, requested: `sessions` })
    expect(screen.getByTestId(`chat-page`).getAttribute(`data-layout`)).toBe(`tab`)
    expect(screen.queryByTestId(`chat-body`)).toBeNull()
    expect(screen.getByTestId(`tab-body`)).toBeTruthy()
    second.unmount()
    renderFrame({ kind: `session`, requested: `sessions`, open: `session` })
    expect(screen.getByTestId(`chat-page`).getAttribute(`data-layout`)).toBe(`item`)
    expect(screen.getByTestId(`item-body`)).toBeTruthy()
    expect(screen.queryByTestId(`session-beside`)).toBeNull()
  })

  it(`the header keeps THE tabs`, () => {
    renderFrame({ kind: `session` })
    expect(screen.getByTestId(`mobile-header`).querySelector(`[data-testid="session-tabs"]`)).toBeTruthy()
  })

  it(`an opened item is a pushed page: its own header (‹ + title) replaces the screen header and THE tabs`, () => {
    const onCloseItem = vi.fn()
    renderFrame({ kind: `agent`, open: `settings` }, { onCloseItem })
    const header = screen.getByTestId(`mobile-header`)
    expect(screen.getByTestId(`session-item-title`).textContent).toBe(`Settings`)
    expect(screen.queryByTestId(`session-tabs`)).toBeNull()
    expect(screen.queryByTestId(`chat-title`)).toBeNull()
    expect(screen.queryByTestId(`session-item-back`)).toBeNull()
    expect(screen.getByTestId(`item-body`)).toBeTruthy()
    fireEvent.click(header.querySelector(`[data-testid="mobile-header-back"]`)!)
    expect(onCloseItem).toHaveBeenCalled()
  })
})
