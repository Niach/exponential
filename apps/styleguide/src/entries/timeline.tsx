import { CalendarCase } from "./calendar.tsx"
import type { StyleguideEntry } from "./types.ts"

// VAPP-232 (eApps U3): THE timeline (a roadmap, Gantt-like rows over
// time): Exponential UI's `Timeline` macro over a records `view=timeline`
// binding, painted by the real React renderer from calendar.specimens.json.

export const entry: StyleguideEntry = {
  id: `timeline`,
  section: `general`,
  owner: `VAPP-232`,
  title: `Timeline`,
  blurb: `Records over time (a roadmap, on-call rotas): Exponential UI's Timeline macro over a records binding with \`view=timeline\`. A label column, then week, month or quarter columns under month (or year) bands with today's column marked; one row per \`row\` value, each record a bar in the full chart colour of its \`tone\`, stacked in lanes when they overlap; a hairline marks today. Pressing a bar fires \`open\` in its scope (\`{path: "id"}\`).`,
  status: {
    web: { state: `ok`, symbol: `Timeline (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiTree (packages/ui) over calendar.specimens.json` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/timeline.rs` },
    ios: { state: `ok`, symbol: `Timeline (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Timeline (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <CalendarCase id="timeline" />,
}
