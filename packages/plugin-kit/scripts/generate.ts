// bun run --filter @exp/plugin-kit generate           rewrite every adapter output
// bun run --filter @exp/plugin-kit generate --check   fail on drift
import { drift, write } from "../src/generate"

if (process.argv.includes(`--check`)) {
  const stale = drift()
  if (stale.length > 0) {
    console.error(`Stale adapter outputs (run bun run --filter @exp/plugin-kit generate):\n${stale.map((path) => `  ${path}`).join(`\n`)}`)
    process.exit(1)
  }
  console.log(`plugin-kit: every adapter output is current`)
} else {
  // Twice: an output may be copied from another one written in this pass.
  const written = [...write(), ...write()]
  console.log(written.length ? written.map((path) => `wrote ${path}`).join(`\n`) : `plugin-kit: nothing to write`)
}
