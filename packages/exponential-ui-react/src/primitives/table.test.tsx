import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Table, TableBody, TableCaption, TableCell, TableFooter, TableHead, TableHeader, TableRow } from "./table"

// FEED-131: shadcn's Table at the Table native's look.

describe(`Table`, () => {
  it(`renders every slot inside its scroll container`, () => {
    const { container } = render(
      <Table>
        <TableCaption>Seats</TableCaption>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow>
            <TableCell>Ada</TableCell>
          </TableRow>
        </TableBody>
        <TableFooter>
          <TableRow>
            <TableCell>1 seat</TableCell>
          </TableRow>
        </TableFooter>
      </Table>
    )
    const outer = container.firstElementChild as HTMLElement
    expect(outer.getAttribute(`data-slot`)).toBe(`table-container`)
    expect(outer.className).toContain(`overflow-x-auto`)
    const table = outer.firstElementChild as HTMLElement
    expect(table.localName).toBe(`table`)
    expect(table.getAttribute(`data-slot`)).toBe(`table`)
    for (const [slot, tag] of [[`table-caption`, `caption`], [`table-header`, `thead`], [`table-body`, `tbody`], [`table-footer`, `tfoot`], [`table-head`, `th`], [`table-cell`, `td`], [`table-row`, `tr`]] as const) {
      expect(container.querySelector(`[data-slot="${slot}"]`)?.localName).toBe(tag)
    }
    expect(container.querySelector(`th`)!.className).toContain(`text-muted-foreground`)
  })

  it(`a selected row carries data-state and the active wash`, () => {
    const { container } = render(
      <Table className="min-w-96">
        <TableBody>
          <TableRow data-state="selected">
            <TableCell>Ada</TableCell>
          </TableRow>
          <TableRow>
            <TableCell>Grace</TableCell>
          </TableRow>
        </TableBody>
      </Table>
    )
    const rows = container.querySelectorAll(`tr`)
    expect(rows[0]!.getAttribute(`data-state`)).toBe(`selected`)
    expect(rows[1]!.hasAttribute(`data-state`)).toBe(false)
    expect(rows[0]!.className).toContain(`data-[state=selected]:bg-glass-active`)
    expect(rows[0]!.className).toContain(`hover:bg-glass-row`)
    expect(container.querySelector(`table`)!.className).toContain(`min-w-96`)
  })
})
