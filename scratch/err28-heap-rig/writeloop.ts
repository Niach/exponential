// C2 write loop: ~1 synced write burst per second against the ISOLATED stack only.
// bun writeloop.ts <seconds>   (DATABASE_URL = the rig's DB; RIG = its folder)
import { SQL } from "bun"
const URL_ = process.env.DATABASE_URL ?? ``
if (!URL_.includes(`:55432/`)) throw new Error(`refusing: DATABASE_URL is not the isolated :55432 stack`)
const sql = new SQL(URL_, { max: 1 })
const ids = JSON.parse(await Bun.file(`${process.env.RIG ?? `/tmp/exp-dcd08adb`}/seeded-ids.json`).text())
const secs = Number(process.argv[2] ?? 600)
const big = (await sql`SELECT id, root_id, team_id FROM sessions WHERE id = ${ids.bigRootId}`)[0]
const others = await sql`SELECT id, root_id, team_id FROM sessions WHERE id::text = ANY(string_to_array(${(ids.openChats ?? ids.longRootIds.slice(0, 4)).join(`,`)}, ','))`
const issueIds = (await sql`SELECT id FROM issues ORDER BY random() LIMIT 400`).map((r: any) => r.id)
const checkIds = (await sql`SELECT id FROM pull_request_checks ORDER BY random() LIMIT 2000`).map((r: any) => r.id)
const end = Date.now() + secs * 1000
let n = 0
while (Date.now() < end) {
  const s = n % 5 === 4 && others.length ? others[n % others.length] : big
  await sql`INSERT INTO messages (id, session_id, root_id, team_id, author_kind, body, created_at, updated_at)
    VALUES (gen_random_uuid(), ${s.id}, ${s.root_id}, ${s.team_id}, 'agent', ${`Write-loop turn ${n}: the shape keeps streaming while the chat is open.`}, now(), now())`
  await sql`UPDATE sessions SET updated_at = now(), busy = ${n % 2 === 0} WHERE id = ${big.id}`
  await sql`UPDATE issues SET description = description || ${` ` + n}, updated_at = now() WHERE id = ${issueIds[n % issueIds.length]}`
  for (let k = 0; k < 3; k++) await sql`UPDATE pull_request_checks SET updated_at = now() WHERE id = ${checkIds[(n * 3 + k) % checkIds.length]}`
  n++
  await Bun.sleep(1000)
}
console.log(`writes`, n)
process.exit(0)
