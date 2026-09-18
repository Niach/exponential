// EXP-792/EXP-941: on the shared `Combobox` the picker keeps its one
// non-obvious rule — a server the chosen machine is NOT ready for is greyed
// with the reason as a tooltip but stays PICKABLE (the desktop launcher names
// the blocker later), so it must never reach the primitive's `disabled`.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { McpServerRow } from "@/lib/mcp-servers"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const SERVERS = [
  { id: `srv-1`, name: `Linear` },
  { id: `srv-2`, name: `Sentry` },
] as unknown as McpServerRow[]

vi.mock(`@/lib/mcp-servers`, () => ({
  serverBlockReason: (server: { id: string }) =>
    server.id === `srv-2` ? `Sign in to Sentry on this machine` : null,
}))

const { McpServerPicker, mcpPickSummary } = await import(
  `@/components/launch-dialog/mcp-server-picker`
)

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

describe(`McpServerPicker`, () => {
  const onToggle = vi.fn()
  beforeEach(() => onToggle.mockReset())

  const open = (selected: string[] = []) => {
    render(
      <McpServerPicker
        servers={SERVERS}
        selectedIds={selected}
        onToggle={onToggle}
        device={undefined}
        now={new Date(`2026-09-18T00:00:00Z`)}
      />
    )
    fireEvent.click(screen.getAllByRole(`button`)[0]!)
  }

  it(`keeps a not-ready row pickable, greyed and explained`, () => {
    open()
    const blocked = rows()[1]!
    expect(blocked.getAttribute(`data-disabled`)).not.toBe(`true`)
    expect(blocked.textContent).toContain(`Not ready`)
    expect(
      blocked.querySelector(`[title="Sign in to Sentry on this machine"]`)
    ).toBeTruthy()

    fireEvent.click(blocked)
    expect(onToggle).toHaveBeenCalledWith(`srv-2`)
  })

  it(`hides the filter field for a short list`, () => {
    open()
    expect(document.querySelector(`[data-slot=command-input]`)).toBeNull()
  })

  it(`summarises the pick on the trigger`, () => {
    expect(mcpPickSummary(SERVERS, [])).toBe(`None`)
    expect(mcpPickSummary(SERVERS, [`srv-1`, `srv-2`])).toBe(`Linear, Sentry`)
  })
})
