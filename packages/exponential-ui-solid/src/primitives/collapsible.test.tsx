// FEED-131: the Solid Collapsible's Radix behaviour.
import { describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen } from "@solidjs/testing-library"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "./collapsible"

describe(`Collapsible`, () => {
  it(`toggles the content and the states on every part`, () => {
    const onOpenChange = vi.fn()
    const { container } = render(() => (
      <Collapsible onOpenChange={onOpenChange}>
        <CollapsibleTrigger>Toggle</CollapsibleTrigger>
        <CollapsibleContent>Body</CollapsibleContent>
      </Collapsible>
    ))
    const trigger = screen.getByRole(`button`)
    const content = container.querySelector(`[data-slot="collapsible-content"]`) as HTMLElement
    expect(trigger.getAttribute(`aria-expanded`)).toBe(`false`)
    expect(trigger.getAttribute(`aria-controls`)).toBe(content.id)
    expect(content.hidden).toBe(true)
    expect(content.textContent).toBe(``)
    fireEvent.click(trigger)
    expect(onOpenChange).toHaveBeenCalledWith(true)
    expect(trigger.getAttribute(`aria-expanded`)).toBe(`true`)
    expect(content.hidden).toBe(false)
    expect(content.textContent).toBe(`Body`)
    expect(container.querySelector(`[data-slot="collapsible"]`)!.getAttribute(`data-state`)).toBe(`open`)
    fireEvent.click(trigger)
    expect(content.hidden).toBe(true)
  })

  it(`disabled: the trigger is disabled and both carry data-disabled`, () => {
    const { container } = render(() => (
      <Collapsible disabled>
        <CollapsibleTrigger>Toggle</CollapsibleTrigger>
        <CollapsibleContent>Body</CollapsibleContent>
      </Collapsible>
    ))
    expect((screen.getByRole(`button`) as HTMLButtonElement).disabled).toBe(true)
    expect(container.querySelector(`[data-slot="collapsible-content"]`)!.hasAttribute(`data-disabled`)).toBe(true)
  })

  it(`forceMount keeps the children mounted while closed`, () => {
    render(() => (
      <Collapsible>
        <CollapsibleTrigger>Toggle</CollapsibleTrigger>
        <CollapsibleContent forceMount>Kept</CollapsibleContent>
      </Collapsible>
    ))
    expect(screen.getByText(`Kept`)).toBeTruthy()
  })
})
