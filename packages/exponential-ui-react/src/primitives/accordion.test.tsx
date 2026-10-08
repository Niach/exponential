import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "./accordion"

describe(`Accordion`, () => {
  it(`renders its parts with data-slot and opens the default item`, () => {
    const { container, getByText } = render(
      <Accordion type="single" defaultValue="a" collapsible>
        <AccordionItem value="a">
          <AccordionTrigger>First</AccordionTrigger>
          <AccordionContent>Body</AccordionContent>
        </AccordionItem>
      </Accordion>
    )
    for (const slot of [`accordion`, `accordion-item`, `accordion-trigger`, `accordion-content`]) {
      expect(container.querySelector(`[data-slot="${slot}"]`), slot).not.toBeNull()
    }
    expect(getByText(`Body`)).toBeTruthy()
  })
})
