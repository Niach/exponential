// A surface written in TypeScript (Node 24: `node surface.ts`). Every
// component's props autocomplete from the catalog; a wrong prop is a type
// error, a wrong value a validator issue. The result is the same flat A2UI
// an agent sends, so it renders on every platform.
import { bind, call, defineSurface, event, run, ui } from "@exponential_app/ui"

const entries = ui.Stack({ gap: `md` }, [
  ui.Section({ title: `Entries`, icon: `ui-clock`, count: 2 }),
  ui.List(
    { divided: true },
    // One Row per item of /entries/rows; `bind("label")` reads the item.
    { template: ui.Row({ title: bind(`label`), subtitle: bind(`detail`), icon: `ui-clock`, chevron: `right`, pressable: true }, undefined, { on: { press: event(`openEntry`, { id: bind(`id`) }) } }), path: `/entries/rows`, key: `id` }
  ),
  ui.EmptyState({ icon: `ui-clock`, title: `No hours yet`, description: `Log the hours you worked and they appear here.`, actionLabel: `Log hours` }, undefined, {
    visible: call(`eq`, { a: 0, b: bind(`/entries/count`) }),
    on: { press: run(`app.toast`, { title: `Soon` }) },
  }),
])

const surface = defineSurface(entries, {
  surfaceId: `working-hours`,
  data: { "/entries": { count: 2, rows: [{ id: `e1`, label: `support, admin, orgs`, detail: `2026-10-23 · 3 h` }, { id: `e2`, label: `review, onboarding`, detail: `2026-10-22 · 4.5 h` }] } },
})

if (surface.issues.length) {
  console.error(surface.issues.map((i) => `${i.id}: ${i.message}`).join(`\n`))
  process.exit(1)
}
console.log(surface.jsonl) // createSurface, updateComponents, updateDataModel: feed it to any host
