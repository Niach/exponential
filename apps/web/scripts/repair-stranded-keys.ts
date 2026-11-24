/**
 * #EXP-2123: the repair for chats an install that WAITED for approval keyed
 * or rotated before the server refused that (install-trust.json `keyMaker`).
 * It wrapped the key for itself only: no approved install, no device can
 * open the chat. NOT part of any deploy.
 *
 * Finds every keyed root whose CURRENT epoch no live, approved install
 * holds a wrap of, and flags it (`seal_rotate_epoch`): `sessionKeys.due`
 * names it `stranded` to every approved reader (web from this release, the
 * apps from their next), and the first one online makes the next epoch, so
 * the chat opens again. Messages sealed at the stranded epoch stay readable
 * only where its secret is. A root of kind `waiting` still has its maker:
 * APPROVING that install (Needs you) instead lets it wrap the key for the
 * readers and loses nothing; flag it only when that install will not be
 * approved (`--waiting`).
 *
 * Without `--apply` nothing is written and only the report prints.
 *
 * Usage (from apps/web, DATABASE_URL = the target database):
 *   bun scripts/repair-stranded-keys.ts                      # report
 *   bun scripts/repair-stranded-keys.ts --apply              # flag the roots no live install holds
 *   bun scripts/repair-stranded-keys.ts --apply --waiting    # also those a waiting install holds
 */
import { db } from "@/db/connection"
import { findStrandedKeys, flagStrandedKeysInTx } from "@/lib/sessions/stranded-keys"

const apply = process.argv.includes(`--apply`)
const withWaiting = process.argv.includes(`--waiting`)

const found = await findStrandedKeys(db)
const waiting = found.filter((root) => root.kind === `waiting`)
const noHolder = found.filter((root) => root.kind === `noHolder`)
const sum = (roots: typeof found) => roots.reduce((n, root) => n + root.messages, 0)
console.log(`[stranded-keys] roots whose current key only a WAITING install holds: ${waiting.length} (${sum(waiting)} messages sealed to it only)`)
for (const root of waiting) {
  console.log(`[stranded-keys]   ${root.rootId} epoch ${root.epoch} team ${root.teamId} holders ${root.holders.join(`,`)} messages ${root.messages}${root.flagged ? ` (due)` : ``}`)
}
console.log(`[stranded-keys] roots whose current key NO live install holds: ${noHolder.length} (${sum(noHolder)} messages, ${noHolder.filter((root) => root.flagged).length} already due)`)
if (!apply) {
  console.log(`[stranded-keys] report only: nothing written (pass --apply, and --waiting to rotate away from waiting installs)`)
  process.exit(0)
}
const targets = withWaiting ? found : noHolder
const flagged = await db.transaction((tx) => flagStrandedKeysInTx(tx, targets))
console.log(`[stranded-keys] flagged for rotation: ${flagged} of ${targets.length} (the rest were due already or moved on)`)
process.exit(0)
