import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import specimens from "./calendar.specimens.json"
import type { StyleguideEntry } from "./types.ts"

// VAPP-232 (eApps U3): THE month/week calendar: Exponential UI's `Calendar`
// macro over a records calendar view (by + until), painted by the real
// React renderer from calendar.specimens.json (the desktop twin paints the
// same JSON). Phones (`@media (form-factor: phone)`) get the compact month
// with dots and the selected day's agenda.

export const SPECIMENS = specimens as unknown as { data: Record<string, unknown>; cases: { id: string; title: string; node: NestedNode }[] }

export function CalendarCase({ id, form = `wide` }: { id: string; form?: `phone` | `wide` }) {
  const c = SPECIMENS.cases.find((s) => s.id === id)!
  return (
    <div className="flex flex-col gap-2" style={form === `phone` ? { width: 390 } : undefined}>
      <span className="text-xs text-muted-foreground">{form === `phone` ? `${c.title} · phone` : c.title}</span>
      <ExponentialUiTree id={`${id}-${form}`} node={c.node} data={SPECIMENS.data} formFactor={form} />
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `calendar`,
  section: `general`,
  owner: `VAPP-232`,
  title: `Calendar`,
  blurb: `A month or a week of records (leave, on-call): Exponential UI's Calendar macro over a records binding with \`by\` + \`until\` (the query lays out the weeks, lanes and "+N more"). Mon-first day cells (other months dimmed, the weekend shaded, today in a filled circle); each span is a bar in the full chart colour of its \`tone\` (chart1..8), across the days it covers, square where it goes on past the week; past \`lanes\` (default 3) a day says "+N more". On phones: the compact month, up to three dots a day, the selected day ringed, and its agenda below (Section + Row: a tone bar, the label, the detail, the status). Pressing a day fires \`select\` in the day's scope (\`{path: "date"}\`), a bar or entry \`open\` in its scope (\`{path: "id"}\`).`,
  status: {
    web: { state: `ok`, symbol: `Calendar (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiTree (packages/ui) over calendar.specimens.json` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/calendar.rs` },
    ios: { state: `ok`, symbol: `Calendar (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Calendar (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => (
    <div className="flex flex-col gap-6">
      <CalendarCase id="calendar-month" />
      <CalendarCase id="calendar-week" />
      <CalendarCase id="calendar-month" form="phone" />
    </div>
  ),
}
