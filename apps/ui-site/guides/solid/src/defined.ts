// The typed DSL from a Solid project: the same `defineSurface` as the
// TypeScript guide, compiled by this project's tsconfig (bundler resolution,
// Solid's JSX settings) with nothing changed. Every component's props
// autocomplete from the catalog; a wrong prop is a type error, a wrong value
// a validator issue. `node src/defined.ts` prints the JSONL a host consumes.
import { bind, call, defineSurface, event, run, ui } from "@exponential_app/ui"

const devices = ui.Stack({ gap: `md` }, [
  ui.Section({ title: `Devices`, icon: `nav-devices`, count: 3 }),
  ui.List(
    { divided: true },
    { template: ui.Row({ title: bind(`label`), subtitle: bind(`detail`), icon: `ui-device`, chevron: `right`, pressable: true }, undefined, { on: { press: event(`openDevice`, { id: bind(`id`) }) } }), path: `/devices/rows`, key: `id` }
  ),
  ui.EmptyState({ icon: `nav-devices`, title: `No devices yet`, description: `Install the desktop app on a machine and it appears here.`, actionLabel: `Add device` }, undefined, {
    visible: call(`eq`, { a: 0, b: bind(`/devices/count`) }),
    on: { press: run(`app.toast`, { title: `Soon` }) },
  }),
])

export const defined = defineSurface(devices, {
  surfaceId: `devices`,
  data: { "/devices": { count: 2, rows: [{ id: `d1`, label: `Alex's Mac mini`, detail: `Desktop · macos · v0.14.71` }, { id: `d2`, label: `Acme build server`, detail: `Server · linux · v0.13.11` }] } },
})

if (defined.issues.length) throw new Error(defined.issues.map((i) => `${i.id}: ${i.message}`).join(`\n`))
console.log(defined.jsonl)
