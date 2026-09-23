// EXP-630: the import worker — an in-process scheduler in the shape of the
// other sweeps (device-code-sweep.ts): module `started`/`running` guards,
// `startImportWorkerScheduler()` from server-bun.ts, `runImportSweep(now)`
// with an injectable clock, `reportSchedulerRun` on every pass.
//
// Multi-replica safety is an ATOMIC ROW CLAIM, not a lock: one UPDATE takes
// the oldest claimable job (`ready`, a `previewing` job nobody holds, or a
// `running`/`previewing` job whose heartbeat went stale) and stamps a fresh
// `claim_token`; zero rows = another replica won. While it runs, a 30 s timer
// re-stamps `claimed_at` and every write carries the token, so a replica that
// lost its claim (a GC pause, slow asset downloads past the stale window)
// aborts instead of writing over the new owner. Resumption is the entity
// map's job (apply.ts skips what is already mapped), so a reclaimed job never
// duplicates rows. Credentials are wiped by the same UPDATE that reaches a
// terminal state and, belt-and-braces, on any job older than 24 h.
import { randomUUID } from "crypto"
import { and, eq, inArray, isNotNull, lt, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { importJobs, users } from "@/db/schema"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { applyBundle, ImportAborted } from "@/lib/import/apply"
import { createDbApplyPorts, type ImportUser } from "@/lib/import/apply-db"
import {
  IMPORT_TERMINAL_STATUSES,
  importPlanSchema,
  importPreviewSchema,
  type ImportProgress,
} from "@/lib/import/bundle"
import { IMPORT_BATCH_SIZE } from "@/lib/import/limits"
import { registerImportWorkerKick } from "@/lib/import/kick"
import { buildDefaultPlan } from "@/lib/import/plan"
import { getImportSource } from "@/lib/import/sources"
import { loadImportTeamState } from "@/lib/import/team-state"

export const IMPORT_CREDENTIAL_TTL_MS = 24 * 60 * 60 * 1000
export const IMPORT_STALE_CLAIM_MS = 5 * 60 * 1000
const HEARTBEAT_MS = 30 * 1000
const INITIAL_DELAY_MS = 15 * 1000
const SWEEP_INTERVAL_MS = 5 * 1000

type JobRow = typeof importJobs.$inferSelect

async function wipeCredentials(now: Date): Promise<number> {
  const rows = await db
    .update(importJobs)
    .set({ credential: null })
    .where(
      and(
        isNotNull(importJobs.credential),
        or(
          inArray(importJobs.status, [...IMPORT_TERMINAL_STATUSES]),
          lt(importJobs.createdAt, new Date(now.getTime() - IMPORT_CREDENTIAL_TTL_MS))
        )
      )
    )
    .returning({ id: importJobs.id })
  return rows.length
}

// The atomic claim. `ready` flips to `running`; `previewing` stays (it is the
// discovery state) but takes the token like any other claim.
async function claimJob(now: Date): Promise<JobRow | null> {
  const stale = new Date(now.getTime() - IMPORT_STALE_CLAIM_MS)
  const token = randomUUID()
  const rows = await db
    .update(importJobs)
    .set({
      status: sql`case when ${importJobs.status} = 'ready' then 'running' else ${importJobs.status} end`,
      claimToken: token,
      claimedAt: now,
      startedAt: sql`case when ${importJobs.status} = 'ready' then coalesce(${importJobs.startedAt}, ${now}) else ${importJobs.startedAt} end`,
    })
    .where(
      eq(
        importJobs.id,
        sql`(
          select id from ${importJobs}
          where status = 'ready'
             or (status = 'previewing' and (claimed_at is null or claimed_at < ${stale}))
             or (status = 'running' and (claimed_at is null or claimed_at < ${stale}))
          order by created_at
          limit 1
          for update skip locked
        )`
      )
    )
    .returning()
  return rows[0] ?? null
}

async function loadImporter(userId: string | null): Promise<ImportUser | null> {
  if (!userId) return null
  const [row] = await db
    .select({
      id: users.id,
      email: users.email,
      name: users.name,
      image: users.image,
      emailVerified: users.emailVerified,
      createdAt: users.createdAt,
      updatedAt: users.updatedAt,
    })
    .from(users)
    .where(eq(users.id, userId))
  return row ?? null
}

async function fencedUpdate(
  job: JobRow,
  set: Partial<typeof importJobs.$inferInsert>
): Promise<boolean> {
  const rows = await db
    .update(importJobs)
    .set(set)
    .where(and(eq(importJobs.id, job.id), eq(importJobs.claimToken, job.claimToken!)))
    .returning({ id: importJobs.id })
  return rows.length > 0
}

// A failed job is RESUMABLE, so its credential stays (a resume without it
// would skip every remaining file download); the 24 h sweep still bounds
// its life, and cancel / completion wipe it at once.
async function failJob(job: JobRow, error: string): Promise<void> {
  await fencedUpdate(job, {
    status: `failed`,
    error: error.slice(0, 2000),
    claimToken: null,
    finishedAt: new Date(),
  })
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}

// Discovery: credential → payload + preview + a default plan, then `draft`.
async function runDiscovery(job: JobRow): Promise<void> {
  const source = getImportSource(job.source)
  try {
    const progress = async (done: number, total: number) => {
      const alive = await fencedUpdate(job, {
        progress: { phase: `discovering`, done, total, warnings: [] } satisfies ImportProgress,
        claimedAt: new Date(),
      })
      if (!alive) throw new ImportAborted(`claim_lost`)
    }
    const { payload, preview } = await source.discover({
      credential: job.credential,
      payload: job.payload,
      onProgress: progress,
    })
    const bundle = source.toBundle(payload, { routing: `team`, importArchived: true })
    const state = await loadImportTeamState(job.teamId, { namespace: bundle.source })
    const plan = buildDefaultPlan(preview, state)
    await fencedUpdate(job, {
      status: `draft`,
      payload,
      preview,
      plan,
      progress: {
        phase: `discovering`,
        done: preview.counts.issues,
        total: preview.counts.issues,
        warnings: preview.warnings,
      } satisfies ImportProgress,
      claimToken: null,
    })
  } catch (err) {
    if (err instanceof ImportAborted) return
    console.error(`[import-worker] discovery failed for ${job.id}:`, err)
    await failJob(job, errorMessage(err))
  }
}

async function runApply(job: JobRow): Promise<void> {
  const source = getImportSource(job.source)
  const heartbeat = setInterval(() => {
    void fencedUpdate(job, { claimedAt: new Date() })
  }, HEARTBEAT_MS)
  try {
    const importer = await loadImporter(job.createdByUserId)
    if (!importer) throw new Error(`The user who started this import no longer exists`)
    const plan = importPlanSchema.parse(job.plan)
    const preview = importPreviewSchema.safeParse(job.preview)
    const bundle = source.toBundle(job.payload, plan)
    const ports = createDbApplyPorts({
      job: {
        id: job.id,
        teamId: job.teamId,
        credential: job.credential,
        claimToken: job.claimToken!,
      },
      namespace: bundle.source,
      importer,
      source,
    })
    const result = await applyBundle(bundle, plan, ports, {
      teamId: job.teamId,
      importerId: importer.id,
      batchSize: IMPORT_BATCH_SIZE,
      initialWarnings: preview.success ? preview.data.warnings : [],
    })
    await fencedUpdate(job, {
      status: `completed`,
      counts: result.counts,
      progress: {
        phase: `done`,
        done: result.counts.issues,
        total: result.counts.issues,
        warnings: result.warnings,
      } satisfies ImportProgress,
      error: null,
      credential: null,
      claimToken: null,
      finishedAt: new Date(),
    })
  } catch (err) {
    if (err instanceof ImportAborted) {
      if (err.reason === `cancelled`) {
        await fencedUpdate(job, { credential: null, claimToken: null, finishedAt: new Date() })
      }
      return
    }
    console.error(`[import-worker] job ${job.id} failed:`, err)
    await failJob(job, errorMessage(err))
  } finally {
    clearInterval(heartbeat)
  }
}

// One sweep pass: wipe credentials, then run every claimable job in turn
// (one at a time per process — imports are I/O-bound and few).
export async function runImportSweep(
  now: Date = new Date()
): Promise<{ processed: number; credentialsWiped: number }> {
  const credentialsWiped = await wipeCredentials(now)
  let processed = 0
  while (true) {
    const job = await claimJob(new Date())
    if (!job) break
    processed += 1
    if (job.status === `previewing`) await runDiscovery(job)
    else await runApply(job)
  }
  return { processed, credentialsWiped }
}

let started = false
let running = false

async function sweep(): Promise<void> {
  if (running) return
  running = true
  const startMs = performance.now()
  try {
    const result = await runImportSweep()
    reportSchedulerRun(`import-worker`, {
      ok: true,
      durationMs: performance.now() - startMs,
      detail: `${result.processed} job(s), ${result.credentialsWiped} credential(s) wiped`,
    })
  } catch (err) {
    reportSchedulerRun(`import-worker`, {
      ok: false,
      durationMs: performance.now() - startMs,
      error: String(err),
    })
    console.error(`[import-worker] sweep failed:`, err)
  } finally {
    running = false
  }
}

// Start the in-process worker — call once at boot (server-bun.ts).
export function startImportWorkerScheduler(): void {
  if (started) return
  started = true
  registerImportWorkerKick(() => {
    void sweep()
  })
  setTimeout(() => {
    void sweep()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void sweep()
  }, SWEEP_INTERVAL_MS)
}
