/**
 * `bun run shots:index` — rebuild `shots/index.json` from what is on disk.
 *
 * The capture orchestrator already does this at the end of every run. This is
 * the standalone door for the two cases that do not involve a capture: after a
 * merge that touched the store (the manifest is derived, so regenerating beats
 * resolving a JSON conflict by hand), and after renaming or deleting views in
 * the catalog, where `--prune` clears out the images nothing claims any more.
 */
import { indexStore } from "./store.ts"

const argv = process.argv.slice(2)
const prune = argv.includes(`--prune`)
const dryRun = argv.includes(`--dry-run`)

const result = await indexStore({ prune, dryRun })

console.log(
  `${result.entries} entr${result.entries === 1 ? `y` : `ies`} · index.json ${
    result.changed ? (dryRun ? `WOULD be rewritten` : `rewritten`) : `unchanged`
  }`
)
for (const orphan of result.orphans) {
  console.log(
    `orphan: ${orphan.viewId}/${orphan.platform} — ${orphan.reason}${
      prune ? (dryRun ? ` (would delete)` : ` (deleted)`) : ` (run with --prune to delete)`
    }`
  )
}
if (result.orphans.length > 0 && !prune) {
  console.log(`\n${result.orphans.length} orphan(s) left in place.`)
}
