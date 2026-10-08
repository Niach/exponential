import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "./table"

describe(`Table`, () => {
  it(`renders every part with data-slot`, () => {
    const { container } = render(
      <Table>
        <TableCaption>Caption</TableCaption>
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
            <TableCell>1</TableCell>
          </TableRow>
        </TableFooter>
      </Table>
    )
    for (const slot of [`table-container`, `table`, `table-caption`, `table-header`, `table-body`, `table-footer`, `table-row`, `table-head`, `table-cell`]) {
      expect(container.querySelector(`[data-slot="${slot}"]`), slot).not.toBeNull()
    }
  })
})
