// Probe 1: what does the StyleX authoring API return WITHOUT the compiler?
import * as stylex from "@stylexjs/stylex"

const show = (label: string, v: unknown) =>
  console.log(`--- ${label}\n${typeof v === `function` ? `[function]` : JSON.stringify(v, null, 2)}`)
const attempt = (label: string, fn: () => unknown) => {
  try {
    show(label, fn())
  } catch (e) {
    console.log(`--- ${label}\nTHROWS: ${(e as Error).message.split(`\n`)[0]}`)
  }
}

const input = {
  a: {
    display: `grid`,
    gridTemplateAreas: `"nav main"`,
    width: `50%`,
    ":hover": { opacity: 0.5 },
    "@media (min-width: 600px)": { gap: 12 },
  },
}

console.log(`=== @stylexjs/stylex 0.19.1, plain runtime (no babel plugin)`)
show(`exports`, Object.keys(stylex))
let styles: any
attempt(`stylex.create(input)`, () => (styles = stylex.create(input as any)))
attempt(`stylex.props(styles.a)`, () => stylex.props(styles?.a))
attempt(`stylex.defineVars({card:'#111'})`, () => stylex.defineVars({ card: `#111` }))

console.log(`\n=== @stylexjs/dev-runtime (latest on npm) inject()`)
const rules: Array<[string, string, number]> = []
attempt(`require('@stylexjs/dev-runtime')`, () => {
  const mod = require(`@stylexjs/dev-runtime`)
  const inject = mod.default ?? mod
  const rt = inject({
    classNamePrefix: `x`,
    dev: true,
    test: false,
    useRemForFontSize: false,
    styleResolution: `application-order`,
    insert: (key: string, ltr: string, priority: number) => rules.push([key, ltr, priority]),
  })
  const s = rt.create(input)
  show(`dev create(input)`, s)
  show(`dev props(s.a)`, rt.props(s.a))
  show(`dev defineVars`, rt.defineVars({ card: `#111` }))
  show(`inserted rules`, rules)
  return `ok`
})

console.log(`\n=== dev-runtime's create() alone (lib/stylex-create, bypassing the broken StyleXSheet import)`)
attempt(`getStyleXCreate(...)`, () => {
  const mod = require(`@stylexjs/dev-runtime/lib/stylex-create`)
  const make = mod.default ?? mod
  const create = make({
    classNamePrefix: `x`,
    dev: false,
    test: false,
    useRemForFontSize: false,
    styleResolution: `application-order`,
    insert: (key: string, ltr: string, priority: number) => rules.push([key, ltr, priority]),
  })
  const s = create(input)
  show(`create(input)`, s)
  show(`stylex.props(s.a) with the 0.19.1 runtime`, stylex.props(s.a))
  show(`inserted rules`, rules)
  return `ok`
})
