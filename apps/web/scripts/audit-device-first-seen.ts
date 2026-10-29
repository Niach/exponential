/**
 * FEED-165: the one-off audit right after the deploy that ships 0206 + 0213
 * (read-only, in a READ ONLY transaction). Lists the machine ids whose
 * first-seen account (`device_first_seen`, the FEED-159 pin-owner signal)
 * is not the earliest-evidence account, and the ids whose pin another
 * account holds, each with every candidate's evidence and the admin call
 * that settles it. Exits 1 when it lists any mismatch.
 *
 * Usage (from apps/web, DATABASE_URL = the target database):
 *   bun run audit:device-first-seen
 * It prints the host and database it reads, and refuses the local dev
 * database (localhost:54321) unless `--dev` says that is the target.
 */
import { sql } from "drizzle-orm"
import { db, pool } from "@/db/connection"
import { deviceFirstSeenFindings, settleCall, type FirstSeenFinding } from "@/lib/device-first-seen-audit"

const target = new URL(process.env.DATABASE_URL ?? ``)
const where = `${target.hostname}:${target.port || `5432`}${target.pathname}`
console.log(`[device-first-seen] reading ${where}`)
if ([`localhost`, `127.0.0.1`, `::1`, `[::1]`].includes(target.hostname) && target.port === `54321` && !process.argv.includes(`--dev`)) {
  console.error(`[device-first-seen] ${where} is the local dev database; point DATABASE_URL at the target, or pass --dev`)
  process.exit(2)
}

const findings = await db.transaction(async (tx) => {
  await tx.execute(sql`SET TRANSACTION READ ONLY`)
  return deviceFirstSeenFindings(tx)
})
await pool.end()

const who = (account: { userId: string; email: string | null } | null) =>
  account ? `${account.userId}${account.email ? ` <${account.email}>` : ``}` : `none`

function print(finding: FirstSeenFinding) {
  const { earliest } = finding
  console.error(`  machine ${finding.deviceId}`)
  console.error(`    first-seen: ${who(finding.firstSeen)}${finding.firstSeen ? ` since ${finding.firstSeen.at}` : ``}`)
  console.error(`    earliest:   ${who(earliest)} by ${earliest.source} at ${earliest.at}`)
  console.error(`    pin:        ${who(finding.pin)}${finding.pin ? ` since ${finding.pin.at}` : ``}`)
  for (const item of finding.evidence) console.error(`      evidence ${item.source} ${item.userId} at ${item.at}`)
  if (!earliest.hasRow) {
    console.error(`    settle: ${earliest.userId} has no devices row for it (pinTo refuses): it must register the machine first`)
  } else {
    console.error(`    settle: ${settleCall(finding)}`)
  }
}

const mismatches = findings.filter((finding) => finding.kind === `mismatch`)
const pins = findings.filter((finding) => finding.kind === `pin`)
console.log(`[device-first-seen] ${mismatches.length} first-seen mismatch(es), ${pins.length} pin(s) held by another account`)
if (mismatches.length > 0) {
  console.error(`[device-first-seen] first-seen is not the earliest-evidence account:`)
  mismatches.forEach(print)
}
if (pins.length > 0) {
  console.error(`[device-first-seen] first-seen agrees, another account holds the pin (the owner's next proven register takes it over; review, settle now or keep the holder with pinTo = the holder):`)
  pins.forEach(print)
}
process.exit(mismatches.length > 0 ? 1 : 0)
