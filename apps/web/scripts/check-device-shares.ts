/**
 * FEED-149: the release train's duplicate-share check (read-only). Lists the
 * (machine, team) pairs more than one account shares, every sharing row of
 * each pair first sharer first (that one hosts teammates' starts). Exits 1
 * when there is any, so the train stops on it; a team owner then drops the
 * extra shares (`devices.dropShare`, the Team devices row's Remove).
 *
 * Usage (from apps/web, DATABASE_URL = the target database):
 *   bun run check:device-shares
 */
import { db } from "@/db/connection"
import { duplicateDeviceShares } from "@/lib/device-share-duplicates"

const duplicates = await duplicateDeviceShares(db)
if (duplicates.length === 0) {
  console.log(`[device-shares] 0 duplicate (machine, team) sharing pairs`)
  process.exit(0)
}
console.error(`[device-shares] ${duplicates.length} duplicate (machine, team) sharing pair(s):`)
for (const pair of duplicates) {
  console.error(`  machine ${pair.deviceId} in team ${pair.teamSlug ?? pair.teamId} (${pair.teamId})`)
  pair.rows.forEach((row, index) =>
    console.error(`    ${index === 0 ? `host` : `drop`} row ${row.id} user ${row.userId} ${row.email ?? ``} since ${row.createdAt}`)
  )
}
process.exit(1)
