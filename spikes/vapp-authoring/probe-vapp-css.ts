// Probe 3: vapp-css against the kitchen sink.
import fixture from "../../packages/domain-contract/fixtures/vapp-kitchen-sink.json"
import * as css from "./vapp-css"

type Node = { id: string; style?: css.VappStyle; children?: Array<Node> }
const all: Record<string, css.VappStyle> = {}
const visit = (n: Node) => {
  if (n.style) all[n.id] = n.style
  n.children?.forEach(visit)
}
visit(fixture as unknown as Node)
const styles = css.create(all)
console.log(`create(): validated ${Object.keys(styles).length} kitchen-sink styles, returned the same objects: ${Object.values(styles).every((s, i) => s === Object.values(all)[i])}`)

// Author-side composition, the way a vApp would write it.
const s = css.create({
  base: { display: `flex`, gap: 8, "@media (min-width: 600px)": { gap: 12 }, ":pressed": { opacity: 0.6 } },
  grid: {
    display: `grid`,
    gridTemplateColumns: `1fr`,
    gridTemplateAreas: [`nav`, `main`, `footer`],
    "@media (min-width: 600px)": { gridTemplateColumns: `minmax(180px, 1fr) 2fr`, gridTemplateAreas: [`nav main`, `nav footer`] },
  },
  dim: { opacity: 0.8, ":pressed": { opacity: 0.4 } },
})
const selected = true
console.log(`props(s.base, s.grid, selected && s.dim) =`)
console.log(JSON.stringify(css.props(s.base, s.grid, selected && s.dim), null, 2))
console.log(`JSON round-trip identical: ${JSON.stringify(JSON.parse(JSON.stringify(css.props(s.base, s.grid)))) === JSON.stringify(css.props(s.base, s.grid))}`)

for (const bad of [{ zIndex: 3 }, { ":hover": { opacity: 0.5 } }, { "@media (min-width: 600px)": { ":pressed": { opacity: 1 } } }]) {
  try {
    css.create({ bad: bad as any })
    console.log(`accepted ${JSON.stringify(bad)} (unexpected)`)
  } catch (e) {
    console.log(`rejects ${JSON.stringify(bad)}: ${(e as Error).message}`)
  }
}
