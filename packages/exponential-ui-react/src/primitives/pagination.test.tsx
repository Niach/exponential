import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
  Pagination,
  PaginationContent,
  PaginationEllipsis,
  PaginationItem,
  PaginationLink,
  PaginationNext,
  PaginationPrevious,
} from "./pagination"

describe(`Pagination`, () => {
  it(`renders a nav with data-slot and marks the active page`, () => {
    const { container, getByRole, getByText } = render(
      <Pagination>
        <PaginationContent>
          <PaginationItem>
            <PaginationPrevious href="#" />
          </PaginationItem>
          <PaginationItem>
            <PaginationLink href="#" isActive>
              2
            </PaginationLink>
          </PaginationItem>
          <PaginationItem>
            <PaginationEllipsis />
          </PaginationItem>
          <PaginationItem>
            <PaginationNext href="#" />
          </PaginationItem>
        </PaginationContent>
      </Pagination>
    )
    expect(getByRole(`navigation`).dataset.slot).toBe(`pagination`)
    expect(getByText(`2`).getAttribute(`aria-current`)).toBe(`page`)
    expect(container.querySelectorAll(`[data-slot="pagination-link"]`)).toHaveLength(3)
    expect(container.querySelector(`[data-slot="pagination-ellipsis"]`)).not.toBeNull()
  })
})
