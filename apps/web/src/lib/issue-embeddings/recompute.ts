// #NEW-134: keeping `issue_embeddings` current.
//
// Two ways an issue is (re)embedded, both ending in `recomputeIssueEmbedding`:
//
//   1. the DEBOUNCE: the issue and comment routers (tRPC, and MCP through
//      them) call `scheduleIssueEmbedding(issueId)` after a title/description
//      write or a comment create/update/delete. Calls for one issue coalesce:
//      it is embedded once, DEBOUNCE_MS after its last change (at the latest
//      MAX_WAIT_MS after the first), so typing in a description or a burst of
//      comments costs one inference.
//   2. the SWEEP (every few minutes, small batches): every live issue with no
//      row, a row of another model, or a row made from an older version of
//      the issue (`issue_embeddings.updated_at` < `issues.updated_at`). That is what survives a restart inside the debounce
//      window, backfills a database on its first boot with pgvector, and
//      covers the writers that do not pass the routers (the widget, a
//      reporter's reply, an import): every comment write bumps its issue's
//      `updated_at` (trigger bump_issue_updated_at_from_comment).
//
// A recompute first hashes the input; an unchanged hash only moves the row's
// `updated_at` forward, so a status change or a new label (which bump
// `issues.updated_at` too) costs two small queries and no inference.
//
// Everything is a silent no-op when the feature is off, the model is not on
// disk or the database has no `issue_embeddings` table.
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { embedText, embedderConfigured } from "./embedder"
import {
  buildIssueEmbeddingInput,
  ISSUE_EMBEDDING_DIMENSIONS,
  ISSUE_EMBEDDING_MODEL,
  issueEmbeddingHash,
  vectorLiteral,
} from "./input"
import { issueEmbeddingsAvailable, issueEmbeddingsTableExists, type EmbeddingExecutor } from "./store"

export const ISSUE_EMBEDDING_DEBOUNCE_MS = 30 * 1000
export const ISSUE_EMBEDDING_MAX_WAIT_MS = 2 * 60 * 1000
/** Comments read per issue: far more than the input budget can hold. */
const MAX_COMMENTS_READ = 40

export type RecomputeResult =
  /** A new vector was stored. */
  | `embedded`
  /** Same hash: only the row's stamp moved. */
  | `unchanged`
  /** The issue is gone or trashed. */
  | `skipped`
  /** No table, no model, or the embedder answered null: try again later. */
  | `unavailable`

export interface RecomputeOptions {
  executor?: EmbeddingExecutor
  /** Injected in tests; defaults to the worker (`embedText("passage", …)`). */
  embed?: (input: string) => Promise<ArrayLike<number> | null>
}

/**
 * Brings one issue's embedding up to date. Never throws for an issue that
 * vanished meanwhile (a purge between the read and the write).
 */
export async function recomputeIssueEmbedding(issueId: string, options: RecomputeOptions = {}): Promise<RecomputeResult> {
  const executor = options.executor ?? db
  const embed = options.embed ?? ((input: string) => embedText(`passage`, input))

  // One statement = one snapshot: the text and the `updated_at` it belongs
  // to. The row stores THAT stamp, so "stale" is exactly "the issue was
  // written again since", whatever the clocks of two transactions say.
  const read = await executor.execute<{
    team_id: string
    title: string
    description: string | null
    comments: string[] | null
    read_at: string
    stored_hash: string | null
    stored_model: string | null
  }>(sql`
    select i.team_id, i.title, i.description, i.updated_at::text as read_at,
           e.content_hash as stored_hash, e.model as stored_model,
           (select array_agg(c.body order by c.created_at, c.id)
              from (select body, created_at, id from comments
                     where issue_id = i.id and board_deleted_at is null
                     order by created_at, id limit ${MAX_COMMENTS_READ}) c) as comments
    from issues i
    left join issue_embeddings e on e.issue_id = i.id
    where i.id = ${issueId}::uuid and i.deleted_at is null and i.board_deleted_at is null`)
  const row = read.rows[0]
  if (!row) return `skipped`

  const input = buildIssueEmbeddingInput({
    title: row.title,
    description: row.description,
    comments: row.comments ?? [],
  })
  const hash = issueEmbeddingHash(input)
  if (row.stored_hash === hash && row.stored_model === ISSUE_EMBEDDING_MODEL) {
    await executor.execute(sql`
      update issue_embeddings set updated_at = ${row.read_at}::timestamptz, team_id = ${row.team_id}::uuid
      where issue_id = ${issueId}::uuid`)
    return `unchanged`
  }

  const vector = await embed(input)
  if (!vector || vector.length !== ISSUE_EMBEDDING_DIMENSIONS) return `unavailable`
  try {
    await executor.execute(sql`
      insert into issue_embeddings (issue_id, team_id, model, embedding, content_hash, updated_at)
      values (${issueId}::uuid, ${row.team_id}::uuid, ${ISSUE_EMBEDDING_MODEL}, ${vectorLiteral(vector)}::vector, ${hash}, ${row.read_at}::timestamptz)
      on conflict (issue_id) do update
        set team_id = excluded.team_id, model = excluded.model, embedding = excluded.embedding,
            content_hash = excluded.content_hash, updated_at = excluded.updated_at`)
  } catch (err) {
    // 23503: the issue was hard-deleted between the read and the write.
    const code = (err as { code?: string; cause?: { code?: string } }).code ?? (err as { cause?: { code?: string } }).cause?.code
    if (code === `23503`) return `skipped`
    throw err
  }
  return `embedded`
}

// ---------------------------------------------------------------------------
// The debounce
// ---------------------------------------------------------------------------

interface DebounceTimers {
  set: (run: () => void, ms: number) => unknown
  clear: (handle: unknown) => void
  now: () => number
}

const realTimers: DebounceTimers = {
  set: (run, ms) => {
    const handle = setTimeout(run, ms)
    // A pending embed never keeps a script or a test run alive.
    handle.unref?.()
    return handle
  },
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  now: () => Date.now(),
}

/**
 * Per-key trailing debounce with a ceiling: `run(key)` fires `delayMs` after
 * the key's LAST `touch`, but never later than `maxWaitMs` after its first
 * one, so an issue under a steady stream of comments is still embedded.
 */
export class KeyedDebouncer {
  private readonly pending = new Map<string, { handle: unknown; firstAt: number }>()

  constructor(
    private readonly run: (key: string) => void,
    private readonly delayMs: number,
    private readonly maxWaitMs: number,
    private readonly timers: DebounceTimers = realTimers
  ) {}

  touch(key: string): void {
    const now = this.timers.now()
    const existing = this.pending.get(key)
    if (existing) this.timers.clear(existing.handle)
    const firstAt = existing?.firstAt ?? now
    const wait = Math.max(0, Math.min(this.delayMs, firstAt + this.maxWaitMs - now))
    const handle = this.timers.set(() => {
      this.pending.delete(key)
      this.run(key)
    }, wait)
    this.pending.set(key, { handle, firstAt })
  }

  get size(): number {
    return this.pending.size
  }

  /** Drops everything pending without running it (tests, shutdown). */
  cancelAll(): void {
    for (const { handle } of this.pending.values()) this.timers.clear(handle)
    this.pending.clear()
  }
}

const debouncer = new KeyedDebouncer(
  (issueId) => {
    void (async () => {
      try {
        if (!(await issueEmbeddingsAvailable())) return
        await recomputeIssueEmbedding(issueId)
      } catch (err) {
        // The sweep finds the issue again; a failed background embed must
        // never surface in the request that scheduled it.
        console.error(`[issue-embeddings] recompute of ${issueId} failed:`, err)
      }
    })()
  },
  ISSUE_EMBEDDING_DEBOUNCE_MS,
  ISSUE_EMBEDDING_MAX_WAIT_MS
)

/**
 * The routers' hook: this issue's title, description or comments changed.
 * Returns at once and never throws; the embed happens ~30 s later, once per
 * burst. A no-op when this process cannot embed.
 */
export function scheduleIssueEmbedding(issueId: string): void {
  try {
    if (!embedderConfigured()) return
    debouncer.touch(issueId)
  } catch {
    // never the write's problem
  }
}

// ---------------------------------------------------------------------------
// The sweep
// ---------------------------------------------------------------------------

/** Live issues whose embedding is missing, of another model, or older than
 *  the issue. Issues changed in the last `settleSeconds` are left to the
 *  debounce. Most recently updated first: what people look at now. */
export async function staleIssueEmbeddingIds(
  options: { limit: number; settleSeconds?: number; executor?: EmbeddingExecutor; teamId?: string }
): Promise<string[]> {
  const executor = options.executor ?? db
  const settle = options.settleSeconds ?? ISSUE_EMBEDDING_DEBOUNCE_MS / 1000
  const team = options.teamId ? sql`and i.team_id = ${options.teamId}::uuid` : sql``
  const result = await executor.execute<{ id: string }>(sql`
    select i.id
    from issues i
    left join issue_embeddings e on e.issue_id = i.id
    where i.deleted_at is null and i.board_deleted_at is null ${team}
      and i.updated_at <= now() - make_interval(secs => ${settle})
      and (e.issue_id is null or e.model <> ${ISSUE_EMBEDDING_MODEL} or e.updated_at < i.updated_at)
    order by i.updated_at desc
    limit ${options.limit}`)
  return result.rows.map((row) => row.id)
}

export interface SweepResult {
  candidates: number
  embedded: number
  unchanged: number
  skipped: number
  /** True when the pass stopped because nothing could be embedded. */
  unavailable: boolean
}

/**
 * One pass over at most `limit` stale issues, one at a time. Stops at the
 * first issue that cannot be embedded right now (the embedder is down or
 * busy): the next pass starts over. `pauseMs` rests between issues.
 */
export async function runIssueEmbeddingSweep(
  options: { limit?: number; pauseMs?: number; settleSeconds?: number; teamId?: string } & RecomputeOptions = {}
): Promise<SweepResult> {
  const executor = options.executor ?? db
  const result: SweepResult = { candidates: 0, embedded: 0, unchanged: 0, skipped: 0, unavailable: false }
  const ready = executor === db ? await issueEmbeddingsAvailable() : await issueEmbeddingsTableExists(executor)
  if (!ready) return { ...result, unavailable: true }
  const ids = await staleIssueEmbeddingIds({
    limit: options.limit ?? SWEEP_BATCH,
    settleSeconds: options.settleSeconds,
    teamId: options.teamId,
    executor,
  })
  result.candidates = ids.length
  for (const id of ids) {
    const outcome = await recomputeIssueEmbedding(id, options)
    if (outcome === `unavailable`) {
      result.unavailable = true
      break
    }
    result[outcome] += 1
    if (options.pauseMs) await new Promise((resolve) => setTimeout(resolve, options.pauseMs))
  }
  return result
}

const SWEEP_BATCH = 20
const INITIAL_DELAY_MS = 2 * 60 * 1000
const SWEEP_INTERVAL_MS = 5 * 60 * 1000
/** After a full batch (a backfill is under way) the next pass comes sooner. */
const CATCH_UP_DELAY_MS = 10 * 1000
/** Between two issues of one pass: background work, never a busy loop. */
const SWEEP_PAUSE_MS = 100

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  let full = false
  try {
    if (!(await issueEmbeddingsAvailable())) return
    const result = await runIssueEmbeddingSweep({ pauseMs: SWEEP_PAUSE_MS })
    full = !result.unavailable && result.candidates >= SWEEP_BATCH
    reportSchedulerRun(`issue-embedding-sweep`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.candidates} stale, ${result.embedded} embedded, ${result.unchanged} unchanged, ${result.skipped} skipped${result.unavailable ? `, embedder unavailable` : ``}`,
    })
    if (result.embedded > 0) console.log(`[issue-embeddings] sweep embedded ${result.embedded} issue(s)`)
  } catch (err) {
    reportSchedulerRun(`issue-embedding-sweep`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[issue-embeddings] sweep failed:`, err)
  } finally {
    running = false
    if (full) setTimeout(() => void sweep(), CATCH_UP_DELAY_MS).unref?.()
  }
}

/**
 * Starts the in-process sweep; call once at boot (server-bun.ts).
 * Double-start-guarded. Two instances racing on one issue write the same
 * row twice (an upsert), nothing worse.
 */
export function startIssueEmbeddingScheduler(): void {
  if (started) return
  started = true
  setTimeout(() => void sweep(), INITIAL_DELAY_MS)
  setInterval(() => void sweep(), SWEEP_INTERVAL_MS)
}
