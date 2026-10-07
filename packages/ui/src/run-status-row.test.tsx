import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { RUN_STATUS_ROW_TONE_CLASS, RunStatusRow } from "./run-status-row"

// EXP-1175: the Run face's status row.
describe(`RunStatusRow`, () => {
  it(`draws the mark, the caption in its tone, the tool line and the toggle`, () => {
    const onToggle = vi.fn()
    render(
      <RunStatusRow
        agent="claude"
        markState="working"
        caption="Building on Mac · 2m"
        tone="muted"
        toolLine="Read apps/web/src/lib/work-faces.ts"
        showWork={false}
        toggleLabel="Show work"
        onToggle={onToggle}
      />
    )
    const row = screen.getByTestId(`run-status-row`)
    expect(row.getAttribute(`data-show-work`)).toBe(`false`)
    expect(row.querySelector(`[data-slot="run-mark"]`)?.getAttribute(`data-state`)).toBe(`working`)
    expect(screen.getByText(`Building on Mac · 2m`).className).toContain(
      RUN_STATUS_ROW_TONE_CLASS.muted
    )
    expect(screen.getByText(`Read apps/web/src/lib/work-faces.ts`)).toBeTruthy()
    fireEvent.click(screen.getByRole(`button`, { name: `Show work` }))
    expect(onToggle).toHaveBeenCalledOnce()
  })

  it(`omits the tool line when there is none and paints the tone`, () => {
    render(
      <RunStatusRow
        agent="codex"
        markState="needs_input"
        caption="Needs input · Mac"
        tone="amber"
        toolLine={null}
        showWork
        toggleLabel="Hide work"
        onToggle={() => {}}
      />
    )
    expect(document.querySelector(`[data-slot="run-status-tool"]`)).toBeNull()
    expect(screen.getByText(`Needs input · Mac`).className).toContain(`text-amber-400`)
    expect(screen.getByTestId(`run-status-row`).getAttribute(`data-show-work`)).toBe(`true`)
    expect(screen.getByRole(`button`, { name: `Hide work` })).toBeTruthy()
  })
})
