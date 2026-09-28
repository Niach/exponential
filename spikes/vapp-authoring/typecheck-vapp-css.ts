// Compile-time check of the typed keys (`bunx tsc -p .`): the @ts-expect-error
// lines MUST error, the rest must not.
import * as css from "./vapp-css"
css.create({ ok: { display: `grid`, gap: 12, "@media (min-width: 600px)": { gap: 16 }, ":pressed": { opacity: 0.6 } } })
// @ts-expect-error not whitelisted
css.create({ bad: { zIndex: 3 } })
// @ts-expect-error bad keyword
css.create({ bad: { display: `inline-grid` } })
// @ts-expect-error colours are #hex or tokens
css.create({ bad: { backgroundColor: `red` } })
