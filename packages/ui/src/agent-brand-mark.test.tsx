import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { AgentBrandMark, AgentRunMark } from "./agent-brand-mark"
import { CLAUDE_SPINNER_FRAMES } from "./claude-spinner.generated"

// EXP-850 §5: the brand mark beside the working caption.
describe(`AgentBrandMark`, () => {
  const markOf = (agent: string | null) =>
    render(<AgentBrandMark agent={agent} />).container.querySelector(`svg`)

  it(`draws claude's own mark for a claude run (and for a row with no agent)`, () => {
    for (const agent of [`claude`, null, `Claude `]) {
      const svg = markOf(agent)
      expect(svg?.getAttribute(`viewBox`)).toBe(`0 0 100 100`)
      expect(svg?.getAttribute(`fill`)).toBe(`hsl(14.8, 63.1%, 59.6%)`)
    }
  })

  it(`draws codex's mark in the text colour`, () => {
    const svg = markOf(`codex`)
    expect(svg?.getAttribute(`viewBox`)).toBe(`0 0 24 24`)
    expect(svg?.getAttribute(`fill`)).toBe(`currentColor`)
  })

  it(`an agent this build does not know falls back to the agents concept`, () => {
    const svg = markOf(`external-thing`)
    // The lucide concept glyph, not one of the two brand paths.
    expect(svg?.getAttribute(`viewBox`)).toBe(`0 0 24 24`)
    expect(svg?.getAttribute(`fill`)).toBe(`none`)
  })

  it(`pulses only when asked, and only with motion allowed`, () => {
    const { container } = render(<AgentBrandMark agent="claude" pulse />)
    expect(container.querySelector(`svg`)?.getAttribute(`class`)).toContain(
      `motion-safe:animate-agent-pulse`
    )
    const steady = render(<AgentBrandMark agent="claude" />)
    expect(
      steady.container.querySelector(`svg`)?.getAttribute(`class`)
    ).not.toContain(`animate-agent-pulse`)
  })
})

// EXP-1184: a live run's mark, per the ×4 `session-display.json` states.
describe(`AgentRunMark`, () => {
  const badgeOf = (container: HTMLElement) =>
    container.querySelector(`[data-slot="run-mark-badge"]`)

  it(`a working claude run steps Claude's spark, with no badge`, () => {
    const { container } = render(<AgentRunMark agent="claude" state="working" />)
    const spinner = container.querySelector(`[data-slot="claude-spinner"]`)
    expect(spinner).not.toBeNull()
    expect(spinner?.querySelectorAll(`path`).length).toBe(
      CLAUDE_SPINNER_FRAMES.length + 1
    )
    expect(
      spinner?.querySelector(`svg`)?.getAttribute(`class`)
    ).toContain(`motion-safe:animate-claude-writing`)
    expect(badgeOf(container)).toBeNull()
  })

  it(`a working codex run pulses its own mark`, () => {
    const { container } = render(<AgentRunMark agent="codex" state="working" />)
    expect(container.querySelector(`[data-slot="claude-spinner"]`)).toBeNull()
    expect(container.querySelector(`svg`)?.getAttribute(`class`)).toContain(
      `motion-safe:animate-agent-pulse`
    )
  })

  it(`parked states wear their badge`, () => {
    for (const [state, cls] of [
      [`needs_input`, `bg-amber-500`],
      [`review`, `bg-emerald-500`],
      [`done`, `bg-sky-500`],
    ] as const) {
      const { container } = render(<AgentRunMark agent="claude" state={state} />)
      expect(badgeOf(container)?.getAttribute(`class`)).toContain(cls)
      expect(container.querySelector(`[data-slot="claude-spinner"]`)).toBeNull()
    }
  })

  it(`a paused run (no state) is the bare mark`, () => {
    const { container } = render(<AgentRunMark agent="claude" />)
    expect(badgeOf(container)).toBeNull()
    expect(container.querySelector(`[data-slot="claude-spinner"]`)).toBeNull()
  })
})
