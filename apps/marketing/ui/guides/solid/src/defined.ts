// The typed DSL from a Solid project: the same `defineSurface` as the
// TypeScript guide, compiled by this project's tsconfig (bundler resolution,
// Solid's JSX settings) with nothing changed. Every component's props
// autocomplete from the catalog; a wrong prop is a type error, a wrong value
// a validator issue. `node src/defined.ts` prints the JSONL a host consumes.
import { bind, call, defineSurface, event, run, ui } from "@exponential_app/ui"

const entries = ui.Stack({ gap: `md` }, [
  ui.Section({ title: `Entries`, icon: `ui-clock`, count: 2 }),
  ui.List(
    { divided: true },
    { template: ui.Row({ title: bind(`label`), subtitle: bind(`detail`), icon: `ui-clock`, chevron: `right`, pressable: true }, undefined, { on: { press: event(`openEntry`, { id: bind(`id`) }) } }), path: `/entries/rows`, key: `id` }
  ),
  ui.EmptyState({ icon: `ui-clock`, title: `No hours yet`, description: `Log the hours you worked and they appear here.`, actionLabel: `Log hours` }, undefined, {
    visible: call(`eq`, { a: 0, b: bind(`/entries/count`) }),
    on: { press: run(`app.toast`, { title: `Soon` }) },
  }),
])

export const defined = defineSurface(entries, {
  surfaceId: `working-hours`,
  data: { "/entries": { count: 2, rows: [{ id: `e1`, label: `support, admin, orgs`, detail: `2026-10-23 · 3 h` }, { id: `e2`, label: `review, onboarding`, detail: `2026-10-22 · 4.5 h` }] } },
})

if (defined.issues.length) throw new Error(defined.issues.map((i) => `${i.id}: ${i.message}`).join(`\n`))
console.log(defined.jsonl)
