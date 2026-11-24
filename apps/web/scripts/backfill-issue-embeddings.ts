/**
 * #NEW-134: embeds every issue whose embedding is missing or stale, in small
 * batches. The server's own sweep (lib/issue-embeddings/recompute.ts) does the
 * same in the background (~20 issues per pass); this script is for filling a
 * database NOW, e.g. right after its Postgres image gained pgvector.
 *
 * The embeddings are computed IN THIS PROCESS, on the machine that runs the
 * script, which reads issue text from DATABASE_URL: run it on the server
 * that owns the data (issue text must not leave it), never from a laptop
 * against a remote database.
 *
 * Resumable by construction: it only ever asks for what is still missing or
 * stale, so a second run continues where the first stopped.
 *
 * Usage (from apps/web, DATABASE_URL = the target database, model fetched
 * with `bun run embeddings:model`):
 *   bun scripts/backfill-issue-embeddings.ts --dry-run          # counts only
 *   bun scripts/backfill-issue-embeddings.ts                    # embed everything
 *   bun scripts/backfill-issue-embeddings.ts --batch 50 --sleep-ms 500 --limit 1000 [--team <uuid>]
 */
import { sql } from "drizzle-orm"
import { db, pool } from "@/db/connection"
import { embedderConfigured, embedderStats, stopEmbedder } from "@/lib/issue-embeddings/embedder"
import { parseBackfillArgs } from "@/lib/issue-embeddings/backfill-args"
import { ISSUE_EMBEDDING_MODEL } from "@/lib/issue-embeddings/input"
import { runIssueEmbeddingSweep } from "@/lib/issue-embeddings/recompute"
import { issueEmbeddingsTableExists } from "@/lib/issue-embeddings/store"

const args = parseBackfillArgs(process.argv.slice(2))
const log = (line: string) => console.log(`[issue-embeddings] ${line}`)

async function staleCount(): Promise<number> {
  const team = args.teamId ? sql`and i.team_id = ${args.teamId}::uuid` : sql``
  const result = await db.execute<{ n: number }>(sql`
    select count(*)::int as n
    from issues i left join issue_embeddings e on e.issue_id = i.id
    where i.deleted_at is null and i.board_deleted_at is null ${team}
      and (e.issue_id is null or e.model <> ${ISSUE_EMBEDDING_MODEL} or e.updated_at < i.updated_at)`)
  return result.rows[0]?.n ?? 0
}

let exitCode = 0
try {
  if (!(await issueEmbeddingsTableExists())) {
    log(`this database has no issue_embeddings table (no pgvector, or the server has not booted on it yet): nothing to do`)
  } else if (args.dryRun) {
    log(`dry run: ${await staleCount()} issue(s) missing or stale for ${ISSUE_EMBEDDING_MODEL}; nothing written`)
  } else if (!embedderConfigured()) {
    log(`the embedding model is not on disk (bun run embeddings:model) or ISSUE_EMBEDDINGS is off`)
    exitCode = 1
  } else {
    const before = await staleCount()
    log(`${before} issue(s) to embed, batches of ${args.batch}, ${args.sleepMs} ms between batches`)
    let embedded = 0
    let unchanged = 0
    let processed = 0
    while (processed < args.limit) {
      const pass = await runIssueEmbeddingSweep({
        limit: Math.min(args.batch, args.limit - processed),
        // Everything, including what changed a second ago.
        settleSeconds: 0,
        teamId: args.teamId,
      })
      if (pass.unavailable) {
        log(`the embedder is unavailable, stopping (run again to continue)`)
        exitCode = 1
        break
      }
      if (pass.candidates === 0) break
      embedded += pass.embedded
      unchanged += pass.unchanged
      processed += pass.candidates
      log(`${processed} done (${embedded} embedded, ${unchanged} unchanged)`)
      if (args.sleepMs > 0) await new Promise((resolve) => setTimeout(resolve, args.sleepMs))
    }
    const stats = embedderStats()
    const perEmbed = stats.embeds > 0 ? Math.round(stats.totalMs / stats.embeds) : 0
    log(`finished: ${embedded} embedded, ${unchanged} unchanged, ${await staleCount()} left, ${perEmbed} ms per embed`)
  }
} finally {
  stopEmbedder()
  await pool.end().catch(() => {})
}
process.exit(exitCode)
