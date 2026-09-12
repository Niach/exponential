import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { AgentBrandMark } from "@/components/agent-brand-mark"

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
