// TEMPORARY (never committed): #EXP-1820 prod cleanup of never-dispatched
// `ecode …` runs on Niach/exponential. MODE=select writes the id list;
// MODE=apply deletes exactly those ids in one transaction.
import { readFileSync, writeFileSync } from "node:fs"
import pg from "pg"
import { aggregateChecksState } from "@exp/domain-contract/pr-checks"

const REPO = `b52d5808-9045-43cf-8334-e0d0df2d4385`
const MODE = process.env.MODE ?? `select`
const IDS = process.env.IDS_FILE!
const client = new pg.Client({ connectionString: process.env.DATABASE_URL })
await client.connect()

async function q(text: string, params: unknown[] = []) {
  console.log(`SQL: ${text.replace(/\s+/g, ` `).trim()} -- params: ${JSON.stringify(params).slice(0, 300)}${JSON.stringify(params).length > 300 ? `…` : ``}`)
  const r = await client.query(text, params)
  console.log(`  -> rowCount ${r.rowCount}`)
  return r
}

type Ids = { runs: string[]; jobs: string[]; commitChecks: string[]; prChecks: string[]; prs: string[] }

async function select(): Promise<Ids> {
  const repo = await q(`SELECT id, full_name, provider FROM repositories WHERE id = $1`, [REPO])
  if (repo.rows[0]?.full_name !== `Niach/exponential`) throw new Error(`unexpected repo ${JSON.stringify(repo.rows)}`)
  // Never dispatched: no job of the run ever got a device, a dispatch or a start.
  const runs = await q(
    `SELECT r.id, r.sha, r.pull_request_id, r.state, r.error, r.started_at FROM ci_runs r
     WHERE r.repository_id = $1 AND r.started_at IS NULL AND r.error IS NULL
       AND NOT EXISTS (SELECT 1 FROM ci_jobs j WHERE j.run_id = r.id AND (j.runner_device_id IS NOT NULL OR j.dispatched_device_id IS NOT NULL OR j.dispatched_at IS NOT NULL OR j.started_at IS NOT NULL OR j.log_key IS NOT NULL))`,
    [REPO]
  )
  const runIds = runs.rows.map((r) => r.id as string)
  const jobs = runIds.length
    ? await q(`SELECT j.id, j.run_id, j."check", j.status FROM ci_jobs j WHERE j.run_id = ANY($1::uuid[])`, [runIds])
    : { rows: [] as any[] }
  for (const j of jobs.rows) if (!String(j.check).startsWith(`ecode `)) throw new Error(`job ${j.id} check ${j.check} is not an ecode check`)
  const cc = runIds.length
    ? await q(
        `SELECT c.id FROM commit_checks c JOIN ci_runs r ON r.sha = c.sha AND r.repository_id = c.repository_id
         JOIN ci_jobs j ON j.run_id = r.id AND j."check" = c.name
         WHERE r.id = ANY($1::uuid[]) AND c.repository_id = $2 AND c.name LIKE 'ecode %'`,
        [runIds, REPO]
      )
    : { rows: [] as any[] }
  const pc = runIds.length
    ? await q(
        `SELECT c.id, c.pull_request_id FROM pull_request_checks c JOIN ci_runs r ON r.pull_request_id = c.pull_request_id AND r.sha = c.head_sha
         JOIN ci_jobs j ON j.run_id = r.id AND j."check" = c.name
         WHERE r.id = ANY($1::uuid[]) AND c.name LIKE 'ecode %'`,
        [runIds]
      )
    : { rows: [] as any[] }
  const ids: Ids = {
    runs: runIds,
    jobs: jobs.rows.map((j) => j.id),
    commitChecks: [...new Set(cc.rows.map((c) => c.id as string))],
    prChecks: [...new Set(pc.rows.map((c) => c.id as string))],
    prs: [...new Set(pc.rows.map((c) => c.pull_request_id as string))],
  }
  // Also any other runs of the repo (started / failed): reported, never touched.
  const other = await q(`SELECT count(*)::int AS n FROM ci_runs WHERE repository_id = $1 AND NOT (id = ANY($2::uuid[]))`, [REPO, runIds])
  console.log(`COUNTS runs=${ids.runs.length} jobs=${ids.jobs.length} commit_checks=${ids.commitChecks.length} pull_request_checks=${ids.prChecks.length} prs=${ids.prs.length} other_runs_untouched=${other.rows[0].n}`)
  return ids
}

async function counts(ids: Ids) {
  const c = async (table: string, list: string[]) =>
    list.length ? (await q(`SELECT count(*)::int AS n FROM ${table} WHERE id = ANY($1::uuid[])`, [list])).rows[0].n : 0
  return { runs: await c(`ci_runs`, ids.runs), jobs: await c(`ci_jobs`, ids.jobs), commitChecks: await c(`commit_checks`, ids.commitChecks), prChecks: await c(`pull_request_checks`, ids.prChecks) }
}

if (MODE === `select`) {
  const ids = await select()
  writeFileSync(IDS, JSON.stringify(ids))
} else if (MODE === `apply`) {
  const ids = JSON.parse(readFileSync(IDS, `utf8`)) as Ids
  await q(`BEGIN`)
  try {
    // Re-verify inside the transaction: the same selection must give the same ids.
    await q(`SELECT id FROM ci_runs WHERE id = ANY($1::uuid[]) FOR UPDATE`, [ids.runs])
    const again = await select()
    for (const k of [`runs`, `jobs`, `commitChecks`, `prChecks`] as const) {
      const a = new Set(again[k])
      if (ids[k].some((id) => !a.has(id))) throw new Error(`${k}: an id is no longer a never-dispatched ecode row`)
    }
    const before = await counts(ids)
    console.log(`BEFORE`, JSON.stringify(before))
    if (before.runs !== ids.runs.length || before.jobs !== ids.jobs.length || before.commitChecks !== ids.commitChecks.length || before.prChecks !== ids.prChecks.length) throw new Error(`count mismatch before`)
    const del = async (table: string, list: string[]) => {
      if (!list.length) return
      const r = await q(`DELETE FROM ${table} WHERE id = ANY($1::uuid[])`, [list])
      if (r.rowCount !== list.length) throw new Error(`${table}: deleted ${r.rowCount} of ${list.length}`)
    }
    await del(`pull_request_checks`, ids.prChecks)
    await del(`commit_checks`, ids.commitChecks)
    await del(`ci_jobs`, ids.jobs)
    await del(`ci_runs`, ids.runs)
    // Rollups: the server's own aggregate over the PR's remaining rows.
    for (const pr of ids.prs) {
      const rows = (await q(`SELECT name, status, conclusion FROM pull_request_checks WHERE pull_request_id = $1`, [pr])).rows
      const cur = (await q(`SELECT checks_state FROM pull_requests WHERE id = $1`, [pr])).rows[0]?.checks_state
      const next = aggregateChecksState(rows)
      console.log(`ROLLUP ${pr}: ${cur} -> ${next}`)
      if (next !== cur) await q(`UPDATE pull_requests SET checks_state = $2, updated_at = now() WHERE id = $1`, [pr, next])
    }
    const after = await counts(ids)
    console.log(`AFTER (in tx)`, JSON.stringify(after))
    if (Object.values(after).some((n) => n !== 0)) throw new Error(`rows remain`)
    await q(`COMMIT`)
  } catch (err) {
    await q(`ROLLBACK`)
    throw err
  }
  console.log(`AFTER (committed)`, JSON.stringify(await counts(ids)))
}
await client.end()
