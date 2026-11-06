// #VAPP-282: fixtures/month-grid.json through the TS reference the React and
// Solid grids paint (`monthView`, `openingMonth`, `isoMonth`).

import { describe, expect, it } from "bun:test"
import fixture from "../../fixtures/month-grid.json"
import { addMonths, isoMonth, monthView, openingMonth } from "./dates"

interface Case {
  name: string
  props: { value?: string; today?: string; firstDayOfWeek: number; marks?: { date: string; tone?: string }[] }
  steps?: string[]
  expect: { month: string; first: string; last: string; leading: number; trailing: number; today: string | null; selected: string | null; marks: Record<string, string>; events?: { name: string; payload: { month: string } }[] }
}

const at = (i: number) => `${Math.floor(i / 7)}.${i % 7}`

describe(`month grid (fixtures/month-grid.json)`, () => {
  for (const c of fixture.cases as unknown as Case[]) {
    it(c.name, () => {
      let view = openingMonth(c.props.value ?? ``, c.props.today ?? ``, ``)
      const events: { name: string; payload: { month: string } }[] = []
      for (const step of c.steps ?? []) {
        view = addMonths(view, step === `next` ? 1 : -1)
        events.push({ name: `month`, payload: { month: isoMonth(view) } })
      }
      const cells = monthView(view, c.props.firstDayOfWeek, { value: c.props.value, today: c.props.today, marks: c.props.marks })
      expect(cells).toHaveLength(42)
      expect(isoMonth(view)).toBe(c.expect.month)
      expect(cells[0]!.date).toBe(c.expect.first)
      expect(cells[41]!.date).toBe(c.expect.last)
      expect(cells.findIndex((d) => !d.outside)).toBe(c.expect.leading)
      expect([...cells].reverse().findIndex((d) => !d.outside)).toBe(c.expect.trailing)
      const today = cells.findIndex((d) => d.today)
      expect(today < 0 ? null : at(today)).toBe(c.expect.today)
      const selected = cells.findIndex((d) => d.selected && !d.outside)
      expect(selected < 0 ? null : at(selected)).toBe(c.expect.selected)
      const marks: Record<string, string> = {}
      cells.forEach((d, i) => d.mark && (marks[at(i)] = d.mark))
      expect(marks).toEqual(c.expect.marks)
      expect(events).toEqual(c.expect.events ?? [])
    })
  }
})
