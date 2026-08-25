/**
 * Delete the device rows the desktop capture lane leaves behind (EXP-566).
 *
 * Every launch of the desktop app registers itself: a fresh `EXP_DATA_DIR`
 * means a fresh `device_id`, and the row outlives the process. One lane run is
 * therefore one extra machine in the demo team forever, and the NEXT run
 * photographs "My machines 3" with two identical Mac minis in it — on the
 * Agents screen, the Add-server dialog, the launcher's device picker and every
 * mobile mirror of them.
 *
 * The seed's own machines are pinned by `device_id` (screenshot-demo.ts), so
 * "not one of ours" is an exact, safe predicate. Anything else belonging to the
 * demo user is lane debris.
 *
 * Usage (from apps/web; the desktop lane calls it automatically when it ends):
 *   bun run screenshots:prune-devices
 */
import { and, eq, inArray, not } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, users } from "@/db/schema"
import { DEMO_DEVICE_ID, DEMO_EMAIL, DEMO_SERVER_DEVICE_ID } from "./screenshot-demo"

/** The machines the SEED owns. Everything else on the demo user is debris. */
const SEEDED = [DEMO_DEVICE_ID, DEMO_SERVER_DEVICE_ID]

async function main() {
  const [demo] = await db
    .select({ id: users.id })
    .from(users)
    .where(eq(users.email, DEMO_EMAIL))
    .limit(1)
  if (!demo) {
    console.log(`no ${DEMO_EMAIL} — nothing to prune`)
    process.exit(0)
  }

  const removed = await db
    .delete(devices)
    .where(and(eq(devices.userId, demo.id), not(inArray(devices.deviceId, SEEDED))))
    .returning({ label: devices.label, deviceId: devices.deviceId })

  console.log(
    removed.length === 0
      ? `no stray devices`
      : `pruned ${removed.length} stray device(s): ${removed.map((row) => `${row.label} (${row.deviceId})`).join(`, `)}`
  )
  process.exit(0)
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err)
  process.exit(1)
})
