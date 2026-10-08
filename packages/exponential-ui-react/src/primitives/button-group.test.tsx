import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Button } from "./button"
import { ButtonGroup, ButtonGroupSeparator, ButtonGroupText } from "./button-group"

describe(`ButtonGroup`, () => {
  it(`renders a role=group with data-slot and its parts`, () => {
    const { container, getByRole } = render(
      <ButtonGroup orientation="vertical">
        <Button>One</Button>
        <ButtonGroupSeparator />
        <ButtonGroupText>Two</ButtonGroupText>
      </ButtonGroup>
    )
    const group = getByRole(`group`)
    expect(group.dataset.slot).toBe(`button-group`)
    expect(group.dataset.orientation).toBe(`vertical`)
    expect(container.querySelector(`[data-slot="button-group-separator"]`)).not.toBeNull()
    expect(container.querySelector(`[data-slot="button-group-text"]`)).not.toBeNull()
  })
})
