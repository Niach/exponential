// EXP-792/EXP-1030: on the shared `Picker` primitive (multi). Readiness is
// the CALLER's own `connection` (the server holds member credentials): a
// server they have not connected is greyed with "Connect first" UNDER its
// name, and picking it opens the settings deep link instead of adding it.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { McpServerRow } from "@/lib/mcp-servers"
import {
  McpServerPicker,
  mcpPickSummary,
} from "@/components/launch-dialog/mcp-server-picker"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const connection = (status: string, error: string | null = null) => ({
  status,
  expiresAt: null,
  error,
})

const SERVERS = [
  {
    id: `srv-1`,
    name: `Linear`,
    url: `https://mcp.linear.app/mcp`,
    connection: connection(`connected`),
  },
  { id: `srv-2`, name: `Sentry`, connection: connection(`not_connected`) },
  { id: `srv-3`, name: `Docs`, connection: connection(`not_needed`) },
  { id: `srv-4`, name: `Notion`, connection: connection(`error`, `refresh failed`) },
] as unknown as McpServerRow[]

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

describe(`McpServerPicker`, () => {
  const onToggle = vi.fn()
  const openSpy = vi.fn()
  beforeEach(() => {
    onToggle.mockReset()
    openSpy.mockReset()
    vi.stubGlobal(`open`, openSpy)
  })

  const open = (selected: string[] = []) => {
    render(
      <McpServerPicker
        servers={SERVERS}
        selectedIds={selected}
        onToggle={onToggle}
        connectHref={(id) => `/t/acme/settings/mcp-servers?connect=${id}`}
      />
    )
    fireEvent.click(screen.getAllByRole(`button`)[0]!)
  }

  it(`is the shared picker in multi mode, and marks the picked rows`, () => {
    open([`srv-1`])
    const marker = document.querySelector(`[data-slot="picker"]`)
    expect(marker?.getAttribute(`data-picker-mode`)).toBe(`multi`)
    expect(rows()[0]!.getAttribute(`data-picked`)).toBe(`true`)
    expect(rows()[1]!.getAttribute(`data-picked`)).toBeNull()
  })

  it(`toggles a connected or sign-in-free server`, () => {
    open()
    fireEvent.click(rows()[0]!)
    expect(onToggle).toHaveBeenCalledWith(`srv-1`)
    fireEvent.click(rows()[2]!)
    expect(onToggle).toHaveBeenCalledWith(`srv-3`)
    expect(
      rows()[2]!.querySelector(`[data-slot=picker-description]`)
    ).toBeNull()
    expect(openSpy).not.toHaveBeenCalled()
  })

  it(`greys an unconnected server and sends the pick to connect it`, () => {
    open()
    const blocked = rows()[1]!
    expect(blocked.getAttribute(`data-disabled`)).not.toBe(`true`)
    expect(
      blocked.querySelector(`[data-slot=picker-description]`)?.textContent
    ).toBe(`Connect first`)
    expect(blocked.querySelector(`[data-mcp-ready=false]`)).toBeTruthy()

    fireEvent.click(blocked)
    expect(onToggle).not.toHaveBeenCalled()
    expect(openSpy).toHaveBeenCalledWith(
      `/t/acme/settings/mcp-servers?connect=srv-2`,
      `_blank`,
      `noopener`
    )
  })

  it(`asks a failed or expired sign-in to reconnect`, () => {
    open()
    expect(
      rows()[3]!.querySelector(`[data-slot=picker-description]`)?.textContent
    ).toBe(`Reconnect first`)
  })

  it(`draws a catalog server's brand mark, a plug for the rest`, () => {
    open()
    const linear = rows()[0]!.querySelector(`svg`)!
    expect(linear.getAttribute(`viewBox`)).toBe(`0 0 512 512`)
    expect(linear.getAttribute(`class`)).toContain(`size-4`)
    expect(linear.querySelector(`path`)?.getAttribute(`fill`)).toBe(`#fff`)
    const docs = rows()[2]!.querySelector(`svg`)!
    expect(docs.getAttribute(`viewBox`)).toBe(`0 0 24 24`)
  })

  it(`hides the filter field for a short list`, () => {
    open()
    expect(document.querySelector(`[data-slot=command-input]`)).toBeNull()
  })

  it(`summarises the pick on the trigger`, () => {
    expect(mcpPickSummary(SERVERS, [])).toBe(`None`)
    expect(mcpPickSummary(SERVERS, [`srv-1`, `srv-3`])).toBe(`Linear, Docs`)
  })
})
