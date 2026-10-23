// Release train 2026-10-23 (N-12 F1 + F2): THE Dock's band on a phone and
// the open sheet's id when its card vanishes (fixtures/dock.json).
import { act, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { dockRows, type DockSource } from "@exp/domain-contract/dock"
import { Dock } from "./dock"

const queued = (count: number): DockSource => ({
  kind: `queued`,
  messages: Array.from({ length: count }, (_, index) => ({ id: `m${index}`, text: `Message ${index}` })),
})

type Observed = { callback: () => void; targets: Element[]; disconnected: boolean }
const observers: Observed[] = []

beforeEach(() => {
  observers.length = 0
  vi.stubGlobal(
    `ResizeObserver`,
    class {
      entry: Observed
      constructor(callback: () => void) {
        this.entry = { callback, targets: [], disconnected: false }
        observers.push(this.entry)
      }
      observe(target: Element) {
        this.entry.targets.push(target)
      }
      unobserve() {}
      disconnect() {
        this.entry.disconnected = true
      }
    }
  )
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe(`Dock band (phone)`, () => {
  it(`stays mounted with no cards on a phone and re-measures when a card arrives`, () => {
    const onHeight = vi.fn()
    const props = { openId: null, onOpenChange: vi.fn(), renderSheet: () => null, onHeight, phoneClearance: true }
    const { rerender } = render(<Dock rows={[]} {...props} />)
    expect(screen.getByTestId(`dock`).dataset.mode).toBe(`none`)
    expect(onHeight).toHaveBeenCalledTimes(1)
    rerender(<Dock rows={dockRows([queued(1)])} {...props} />)
    expect(screen.getByTestId(`dock`).dataset.mode).toBe(`single`)
    const band = screen.getByTestId(`dock`)
    const live = observers.filter((entry) => !entry.disconnected && entry.targets.includes(band))
    expect(live).toHaveLength(1)
    const before = onHeight.mock.calls.length
    act(() => live[0]!.callback())
    expect(onHeight.mock.calls.length).toBe(before + 1)
  })

  it(`is absent with no cards, no composer and no phone clearance, and reports 0`, () => {
    const onHeight = vi.fn()
    render(<Dock rows={[]} openId={null} onOpenChange={vi.fn()} renderSheet={() => null} onHeight={onHeight} />)
    expect(screen.queryByTestId(`dock`)).toBeNull()
    expect(onHeight).toHaveBeenLastCalledWith(0)
  })
})

describe(`Dock open sheet`, () => {
  it(`forgets the open id when its card vanishes, so a returning card stays closed`, () => {
    const onOpenChange = vi.fn()
    const props = { onOpenChange, renderSheet: () => <p>sheet</p> }
    const { rerender } = render(<Dock rows={dockRows([queued(2)])} openId="queued" {...props} />)
    expect(screen.getByTestId(`dock-sheet`)).toBeTruthy()
    expect(onOpenChange).not.toHaveBeenCalled()
    rerender(<Dock rows={[]} openId="queued" {...props} />)
    expect(onOpenChange).toHaveBeenCalledWith(null)
  })
})
