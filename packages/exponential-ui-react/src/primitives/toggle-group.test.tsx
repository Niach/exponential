import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { ToggleGroup, ToggleGroupItem } from "./toggle-group"

describe(`ToggleGroup`, () => {
  it(`renders items with data-slot and passes the group variant down`, () => {
    const { container } = render(
      <ToggleGroup type="single" variant="outline" defaultValue="a">
        <ToggleGroupItem value="a">A</ToggleGroupItem>
        <ToggleGroupItem value="b">B</ToggleGroupItem>
      </ToggleGroup>
    )
    expect(container.querySelector(`[data-slot="toggle-group"]`)).not.toBeNull()
    const items = container.querySelectorAll<HTMLElement>(`[data-slot="toggle-group-item"]`)
    expect(items).toHaveLength(2)
    expect(items[0].dataset.variant).toBe(`outline`)
    expect(items[0].dataset.state).toBe(`on`)
  })
})
