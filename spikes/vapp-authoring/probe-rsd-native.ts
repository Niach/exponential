// Probe 2: react-strict-dom's NATIVE css.create/css.props in headless Bun.
import { resolve } from "node:path"

const show = (label: string, v: unknown) => console.log(`--- ${label}\n${JSON.stringify(v, null, 2)}`)

const entry = resolve(import.meta.dir, `node_modules/react-strict-dom/dist/native/index.js`)
const { css } = await import(entry)
console.log(`loaded react-strict-dom native; css exports: ${Object.keys(css).join(`, `)}`)

const input = {
  display: `grid`,
  gridTemplateAreas: `"nav main"`,
  gridTemplateColumns: `minmax(180px, 1fr) 2fr`,
  width: `50%`,
  aspectRatio: `16/9`,
  flexBasis: `30%`,
  marginLeft: `auto`,
  padding: `1rem`,
  borderRadius: `12px`,
  ":hover": { opacity: 0.5 },
  "@media (min-width: 600px)": { gap: 12 },
}

console.log(`\n=== A: the StyleX-subset shape our fixture uses (conditions as TOP-LEVEL keys)`)
const a = css.create({ a: input })
show(`css.create({a})`, a)
show(`css.props(styles.a) with no options (this = undefined)`, (() => {
  try {
    return css.props(a.a)
  } catch (e) {
    return `THROWS: ${(e as Error).message}`
  }
})())
show(`css.props.call({viewportWidth: 900}, styles.a)`, css.props.call({ viewportWidth: 900 }, a.a))

console.log(`\n=== B: StyleX's canonical shape (conditions INSIDE the value)`)
const b = css.create({
  a: {
    display: `flex`,
    width: `50%`,
    padding: `1rem`,
    opacity: { default: 1, ":hover": 0.5 },
    gap: { default: 4, "@media (min-width: 600px)": 12 },
  },
})
show(`css.create({b})`, b)
for (const opts of [{ viewportWidth: 390 }, { viewportWidth: 900, hover: true }, { viewportWidth: 900, fontScale: 1.5 }]) {
  show(`css.props.call(${JSON.stringify(opts)}, styles.a)`, css.props.call(opts, b.a))
}

console.log(`\n=== C: defineVars`)
const vars = css.defineVars({ card: `#111` })
show(`css.defineVars({card})`, vars)
const c = css.create({ a: { backgroundColor: vars.card } })
show(`create with a var`, c)
show(`props with a var`, css.props.call({ viewportWidth: 390 }, c.a))
