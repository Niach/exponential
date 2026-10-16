import { fireEvent, render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
  Trace,
  TraceCollapsible,
  TraceCollapsibleContent,
  TraceCollapsibleTrigger,
  TraceDescription,
  TraceIcon,
  TraceLine,
  TraceTitle,
} from "./trace"

// EXP-1324: shadcn's Trace on our Collapsible, sizes per fixtures/trace.json.

const slot = (root: HTMLElement, name: string) =>
  root.querySelector(`[data-slot="${name}"]`) as HTMLElement | null

describe(`Trace`, () => {
  it(`paints a line: icon, title, description, status`, () => {
    const { container } = render(
      <Trace>
        <TraceLine status="running">
          <TraceIcon>
            <svg />
          </TraceIcon>
          <TraceTitle>Running bun test</TraceTitle>
          <TraceDescription>31s</TraceDescription>
        </TraceLine>
      </Trace>
    )
    const line = slot(container, `trace-line`)!
    expect(line.getAttribute(`data-status`)).toBe(`running`)
    expect(line.className).toContain(`data-[status=running]:*:data-[slot=trace-title]:shimmer`)
    expect(slot(container, `trace-title`)!.textContent).toBe(`Running bun test`)
    expect(slot(container, `trace-title`)!.className).toContain(`motion-reduce:shimmer-none`)
    expect(slot(container, `trace-description`)!.className).toContain(`text-muted-foreground/70`)
    expect(slot(container, `trace-icon`)!.getAttribute(`aria-hidden`)).toBe(`true`)
  })

  it(`states its size once for every part`, () => {
    const { container } = render(
      <Trace size="sm">
        <TraceLine>
          <TraceTitle>Read a file</TraceTitle>
        </TraceLine>
      </Trace>
    )
    expect(slot(container, `trace`)!.getAttribute(`data-size`)).toBe(`sm`)
    expect(slot(container, `trace`)!.className).toContain(`group/trace`)
    expect(slot(container, `trace-line`)!.className).toContain(`group-data-[size=sm]/trace:text-xs`)
  })

  it(`a null status stamps nothing`, () => {
    const { container } = render(<TraceLine status={null}>x</TraceLine>)
    expect(slot(container, `trace-line`)!.hasAttribute(`data-status`)).toBe(false)
  })

  it(`a collapsible line toggles its railed content`, () => {
    const { container, getByRole } = render(
      <Trace>
        <TraceCollapsible>
          <TraceCollapsibleTrigger>
            <TraceLine>
              <TraceTitle>Ran ./gradlew</TraceTitle>
            </TraceLine>
          </TraceCollapsibleTrigger>
          <TraceCollapsibleContent>BUILD SUCCESSFUL</TraceCollapsibleContent>
        </TraceCollapsible>
      </Trace>
    )
    const trigger = getByRole(`button`)
    expect(trigger.getAttribute(`aria-expanded`)).toBe(`false`)
    expect(slot(container, `trace-chevron`)).not.toBeNull()
    expect(container.textContent).not.toContain(`BUILD SUCCESSFUL`)
    fireEvent.click(trigger)
    expect(trigger.getAttribute(`aria-expanded`)).toBe(`true`)
    const inner = slot(container, `trace-collapsible-content-inner`)!
    expect(inner.textContent).toBe(`BUILD SUCCESSFUL`)
    expect(inner.className).toContain(`border-l`)
  })

  it(`a controlled collapsible starts open`, () => {
    const { container } = render(
      <TraceCollapsible open onOpenChange={() => {}}>
        <TraceCollapsibleTrigger>todo</TraceCollapsibleTrigger>
        <TraceCollapsibleContent>item</TraceCollapsibleContent>
      </TraceCollapsible>
    )
    expect(slot(container, `trace-collapsible-content-inner`)!.textContent).toBe(`item`)
  })
})
