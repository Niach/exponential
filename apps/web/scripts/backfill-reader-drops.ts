/**
 * #EXP-1951: the one-time backfill for readers dropped BEFORE the rotation
 * rule (sealed-content.json `rotation`). NOT part of any deploy: whether
 * existing chats are forced to rotate is a product decision.
 *
 * Every keyed root is run through the rule a drop runs today
 * (`dropNonReaderKeysInTx`): the wraps of installs that no longer read the
 * tree (revoked, their account off the team, their device no longer the
 * tree's host) are deleted and each root that lost one is flagged
 * (`seal_rotate_epoch`), so its next online holder rotates it. A reader
 * whose rows are gone already (a deleted account's installs cascade away)
 * leaves nothing to find: `--all` flags EVERY keyed root instead.
 *
 * Without `--apply` nothing is written (the transaction is rolled back) and
 * only the counts print.
 *
 * Usage (from apps/web, DATABASE_URL = the target database):
 *   bun scripts/backfill-reader-drops.ts            # dry run
 *   bun scripts/backfill-reader-drops.ts --all      # dry run of flagging every keyed root
 *   bun scripts/backfill-reader-drops.ts --apply [--all]
 */
import { and, isNotNull, isNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { sessionKeys, sessions } from "@/db/schema"
import { dropNonReaderKeysInTx } from "@/lib/sessions/tree-keys"

const apply = process.argv.includes(`--apply`)
const all = process.argv.includes(`--all`)

class DryRun extends Error {}

const count = async (tx: Pick<typeof db, `select`>, table: typeof sessionKeys | typeof sessions, where?: ReturnType<typeof and>) =>
  (await tx.select({ n: sql<number>`count(*)::int` }).from(table).where(where))[0]!.n

try {
  await db.transaction(async (tx) => {
    const keyed = and(isNull(sessions.parentId), isNotNull(sessions.sealEpoch))
    const roots = (await tx.select({ id: sessions.id }).from(sessions).where(keyed)).map((row) => row.id)
    const before = { wraps: await count(tx, sessionKeys), due: await count(tx, sessions, and(keyed, isNotNull(sessions.sealRotateEpoch))) }
    const flagged = await dropNonReaderKeysInTx(tx, roots)
    if (all) {
      await tx.update(sessions).set({ sealRotateEpoch: sql`${sessions.sealEpoch}` }).where(and(keyed, isNull(sessions.sealRotateEpoch)))
    }
    const after = { wraps: await count(tx, sessionKeys), due: await count(tx, sessions, and(keyed, isNotNull(sessions.sealRotateEpoch))) }
    console.log(`[reader-drops] keyed roots: ${roots.length}`)
    console.log(`[reader-drops] wraps of installs that no longer read their tree: ${before.wraps - after.wraps} of ${before.wraps}`)
    console.log(`[reader-drops] roots that lost one (flagged for rotation): ${flagged.length}`)
    console.log(`[reader-drops] roots due for rotation: ${before.due} before, ${after.due} after${all ? ` (--all)` : ``}`)
    if (!apply) throw new DryRun()
  })
  console.log(`[reader-drops] applied`)
} catch (error) {
  if (!(error instanceof DryRun)) throw error
  console.log(`[reader-drops] dry run: nothing written (pass --apply)`)
}
process.exit(0)
