/**
 * VAPP-289 (round 9 contract §14.1): the A2UI 1.0 stored-data migration, by
 * hand. The logic lives in `lib/apps/a2ui10-migrate.ts`, which the flag-day PR
 * also runs at boot. Idempotent; safe beside a live server and after a crash.
 *
 * Usage (from apps/web):
 *   bun scripts/a2ui10-migrate.ts                          # dry run: counts, writes nothing
 *   bun scripts/a2ui10-migrate.ts --apply --report out.json
 *   bun scripts/a2ui10-migrate.ts --batch-size 50 --apply
 *
 * The signed package files of the repo (§18.1), not the database. Without
 * `--apply` it only says what would happen. A signed file is written only when
 * `EXP_APP_TEMPLATES_SEED` holds its key: an unsigned template is refused.
 *   EXP_APP_TEMPLATES_SEED=<seed> bun scripts/a2ui10-migrate.ts --apply --files ../../samples/apps/*.app.json
 *
 * Exit 1: `--apply` left a failed or conflicted row or file. A dry run exits 0;
 * read its report.
 */
import { readFileSync, writeFileSync } from "node:fs"
import { base64UrlDecode } from "@exponential_app/eui"
import { db, pool } from "@/db/connection"
import { formatMigrationReport, runA2ui10Migration, upgradePackageFile } from "@/lib/apps/a2ui10-migrate"

const SIGNER_NAME = `Exponential`

function valueOf(args: string[], flag: string): string | undefined {
  const at = args.indexOf(flag)
  if (at < 0) return undefined
  const value = args[at + 1]
  if (!value || value.startsWith(`--`)) throw new Error(`${flag} needs a value`)
  return value
}

async function migrateFiles(paths: string[], apply: boolean): Promise<boolean> {
  const seed = process.env.EXP_APP_TEMPLATES_SEED ? base64UrlDecode(process.env.EXP_APP_TEMPLATES_SEED) : null
  if (process.env.EXP_APP_TEMPLATES_SEED && seed?.length !== 32) throw new Error(`EXP_APP_TEMPLATES_SEED is 32 bytes, unpadded base64url`)
  const signer = seed ? { name: SIGNER_NAME, seed } : undefined
  let ok = true
  for (const path of paths) {
    const upgraded = await upgradePackageFile(JSON.parse(readFileSync(path, `utf8`)), signer)
    const stranded = upgraded.status === `valid` && !upgraded.resigned
    if (upgraded.file === null || stranded) ok = false
    const verdict = upgraded.file === null ? `signature does not verify` : stranded ? `signed with a key we do not hold: left as it is` : upgraded.resigned ? `upgraded, re-signed` : `upgraded, unsigned`
    console.log(`${path}: ${verdict}`)
    if (apply && upgraded.file !== null && !stranded) writeFileSync(path, `${JSON.stringify(upgraded.file, null, 2)}\n`)
  }
  return ok
}

async function main(): Promise<number> {
  const args = process.argv.slice(2)
  const apply = args.includes(`--apply`)
  if (apply && args.includes(`--dry-run`)) throw new Error(`--apply or --dry-run, not both`)
  const filesAt = args.indexOf(`--files`)
  if (filesAt >= 0) return (await migrateFiles(args.slice(filesAt + 1), apply)) || !apply ? 0 : 1
  const reportPath = valueOf(args, `--report`)
  const batch = valueOf(args, `--batch-size`)
  const batchSize = batch === undefined ? undefined : Number(batch)
  if (batchSize !== undefined && (!Number.isInteger(batchSize) || batchSize < 1)) throw new Error(`--batch-size is a positive integer`)
  const report = await runA2ui10Migration(db, { apply, batchSize })
  console.log(formatMigrationReport(report))
  if (reportPath) writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`)
  return apply && !report.ok ? 1 : 0
}

main()
  .then(async (code) => {
    await pool.end()
    process.exit(code)
  })
  .catch(async (error) => {
    console.error(error)
    await pool.end().catch(() => {})
    process.exit(1)
  })
