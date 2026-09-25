// EXP-630: the import wizard's router. Owner-only on every procedure (a
// tracker migration creates boards, statuses, labels and hundreds of issues
// under the owner's name). Jobs are server-only rows (never an Electric
// shape); the wizard polls `get`.
//
// The pasted credential lives on the job row ONLY while the job is live: it
// is never part of any selection here (`jobSelection`, the
// `inviteListSelection` pattern), nulled by the UPDATE that completes or
// cancels the job (a `failed` job keeps it so a resume can still download
// files), and swept after 24 h regardless.
import { z } from "zod"
import { and, desc, eq, inArray, isNull } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure } from "@/lib/trpc"
import { importJobs } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/team-membership"
import { TokenBucketLimiter } from "@/lib/widget/rate-limit"
import {
  importBundleSchema,
  importCountsSchema,
  importPlanSchema,
  importPreviewSchema,
  importProgressSchema,
  IMPORT_TERMINAL_STATUSES,
  type DryRunResult,
  type ImportJobStatus,
} from "@/lib/import/bundle"
import { evaluatePlan } from "@/lib/import/plan"
import { getImportSource } from "@/lib/import/sources"
import { loadImportTeamState } from "@/lib/import/team-state"
import { kickImportWorker } from "@/lib/import/kick"

// Per-user bound on discovery runs: each one is a full workspace fetch
// against a third-party API under the user's own key.
const connectLimiter = new TokenBucketLimiter({ capacity: 5, refillPerHour: 20 })

export const jobSelection = {
  id: importJobs.id,
  teamId: importJobs.teamId,
  createdByUserId: importJobs.createdByUserId,
  source: importJobs.source,
  status: importJobs.status,
  preview: importJobs.preview,
  plan: importJobs.plan,
  progress: importJobs.progress,
  counts: importJobs.counts,
  error: importJobs.error,
  startedAt: importJobs.startedAt,
  finishedAt: importJobs.finishedAt,
  createdAt: importJobs.createdAt,
  updatedAt: importJobs.updatedAt,
} as const

async function assertOwner(userId: string, teamId: string) {
  await resolveTeamAccess(userId, teamId, `mutate_resources`, { roles: [`owner`] })
}

function shapeJob(row: {
  id: string
  teamId: string
  createdByUserId: string | null
  source: string
  status: string
  preview: unknown
  plan: unknown
  progress: unknown
  counts: unknown
  error: string | null
  startedAt: Date | null
  finishedAt: Date | null
  createdAt: Date
  updatedAt: Date
}) {
  return {
    ...row,
    status: row.status as ImportJobStatus,
    preview: importPreviewSchema.safeParse(row.preview).data ?? null,
    plan: importPlanSchema.safeParse(row.plan).data ?? null,
    progress: importProgressSchema.safeParse(row.progress).data ?? null,
    counts: importCountsSchema.safeParse(row.counts).data ?? null,
  }
}

// Loads a job for its team's owner. Never returns the credential.
async function loadJobForOwner(userId: string, jobId: string) {
  const { db } = await import(`@/db/connection`)
  const [row] = await db.select(jobSelection).from(importJobs).where(eq(importJobs.id, jobId))
  if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Import not found` })
  await assertOwner(userId, row.teamId)
  return row
}

async function loadPayload(jobId: string): Promise<unknown> {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select({ payload: importJobs.payload })
    .from(importJobs)
    .where(eq(importJobs.id, jobId))
  return row?.payload ?? null
}

async function dryRun(job: Awaited<ReturnType<typeof loadJobForOwner>>): Promise<DryRunResult> {
  const plan = importPlanSchema.safeParse(job.plan)
  if (!plan.success) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Save a mapping first` })
  }
  const source = getImportSource(job.source)
  const payload = await loadPayload(job.id)
  if (payload === null) {
    // The worker purges a finished job's snapshot after its retention
    // window (worker.ts); a `failed` job past it can no longer resume.
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This import's data has expired. Connect again to start over.`,
    })
  }
  const bundle = source.toBundle(payload, plan.data)
  const boardIdsForNumbers = Object.values(plan.data.boards)
    .filter((entry) => entry.mode === `existing`)
    .map((entry) => (entry as { boardId: string }).boardId)
  const state = await loadImportTeamState(job.teamId, {
    namespace: bundle.source,
    boardIdsForNumbers,
  })
  return evaluatePlan(bundle, plan.data, state)
}

export const importsRouter = router({
  // Validates the credential, creates the job and hands it to the worker
  // for discovery (the full paged fetch, which can take a while).
  connect: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        source: z.literal(`linear`),
        apiKey: z.string().min(1).max(500),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertOwner(userId, input.teamId)
      const limit = connectLimiter.tryTake(userId)
      if (!limit.ok) {
        throw new TRPCError({
          code: `TOO_MANY_REQUESTS`,
          message: `Too many import attempts. Retry in ${limit.retryAfterSeconds}s.`,
        })
      }
      const source = getImportSource(input.source)
      const check = await source.validateCredential(input.apiKey)
      if (!check.ok) throw new TRPCError({ code: `BAD_REQUEST`, message: check.reason })
      const [job] = await ctx.db
        .insert(importJobs)
        .values({
          teamId: input.teamId,
          createdByUserId: userId,
          source: input.source,
          status: `previewing`,
          credential: input.apiKey.trim(),
        })
        .returning({ id: importJobs.id })
      kickImportWorker()
      return { jobId: job!.id, connectedAs: check.who }
    }),

  // The no-Linear seam: a raw bundle from a script, a CLI or an agent.
  ingest: authedProcedure
    .input(z.object({ teamId: z.string().uuid(), bundle: importBundleSchema }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertOwner(userId, input.teamId)
      const limit = connectLimiter.tryTake(userId)
      if (!limit.ok) {
        throw new TRPCError({
          code: `TOO_MANY_REQUESTS`,
          message: `Too many import attempts. Retry in ${limit.retryAfterSeconds}s.`,
        })
      }
      const [job] = await ctx.db
        .insert(importJobs)
        .values({
          teamId: input.teamId,
          createdByUserId: userId,
          source: `bundle`,
          status: `previewing`,
          payload: input.bundle,
        })
        .returning({ id: importJobs.id })
      kickImportWorker()
      return { jobId: job!.id }
    }),

  get: authedProcedure
    .input(z.object({ jobId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      return shapeJob(await loadJobForOwner(ctx.session.user.id, input.jobId))
    }),

  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid(), limit: z.number().int().min(1).max(50).default(10) }))
    .query(async ({ ctx, input }) => {
      await assertOwner(ctx.session.user.id, input.teamId)
      const rows = await ctx.db
        .select(jobSelection)
        .from(importJobs)
        .where(
          and(
            eq(importJobs.teamId, input.teamId),
            // EXP-1076: a cleared card is gone from Recent imports; its
            // import_entity_map rows (the re-import idempotency key) stay.
            isNull(importJobs.dismissedAt)
          )
        )
        .orderBy(desc(importJobs.createdAt))
        .limit(input.limit)
      return rows.map(shapeJob)
    }),

  savePlan: authedProcedure
    .input(z.object({ jobId: z.string().uuid(), plan: importPlanSchema }))
    .mutation(async ({ ctx, input }) => {
      const job = await loadJobForOwner(ctx.session.user.id, input.jobId)
      if (job.status !== `draft` && job.status !== `failed`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The mapping can only change before the import starts`,
        })
      }
      const [row] = await ctx.db
        .update(importJobs)
        .set({ plan: input.plan })
        .where(and(eq(importJobs.id, job.id), inArray(importJobs.status, [`draft`, `failed`])))
        .returning({ id: importJobs.id })
      if (!row) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The import already started` })
      return { ok: true }
    }),

  dryRun: authedProcedure
    .input(z.object({ jobId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const job = await loadJobForOwner(ctx.session.user.id, input.jobId)
      if (job.status !== `draft` && job.status !== `failed`) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Nothing to dry-run in this state` })
      }
      return dryRun(job)
    }),

  // Re-runs the dry run and refuses on blockers; a `failed` job restarts
  // from where the entity map says it stopped.
  start: authedProcedure
    .input(z.object({ jobId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const job = await loadJobForOwner(ctx.session.user.id, input.jobId)
      if (job.status !== `draft` && job.status !== `failed`) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The import cannot start from this state` })
      }
      const result = await dryRun(job)
      if (result.blockers.length > 0) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The dry run has blockers: ${result.blockers[0]}`,
        })
      }
      const [row] = await ctx.db
        .update(importJobs)
        .set({ status: `ready`, error: null, claimToken: null, claimedAt: null })
        .where(and(eq(importJobs.id, job.id), inArray(importJobs.status, [`draft`, `failed`])))
        .returning({ id: importJobs.id })
      if (!row) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The import already started` })
      kickImportWorker()
      return { ok: true }
    }),

  // Stops a live job at its next batch boundary (rows already written stay);
  // wipes the credential right away. Nulling the claim token is what makes
  // the cancel stick: every worker write is fenced on `status IN
  // (previewing, running) AND claim_token = <its token>`, so neither a
  // finishing discovery nor a failure can write over `cancelled`.
  //
  // EXP-1076: backing out BEFORE the import started leaves no trace at all.
  // A `draft`/`previewing` job has written nothing outside its own row, so it
  // is DELETED rather than parked as a cancelled card (import_entity_map
  // cascades, and in those states it is empty). The delete is fenced on the
  // same two statuses, so a job that reached `ready`/`running` between the
  // read and the write falls through to the UPDATE below instead. The worker
  // tolerates a row that vanished under it: its writes are fenced UPDATEs
  // that simply match 0 rows, and the resulting `ImportAborted` is swallowed.
  cancel: authedProcedure
    .input(z.object({ jobId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const job = await loadJobForOwner(ctx.session.user.id, input.jobId)
      const [discarded] = await ctx.db
        .delete(importJobs)
        .where(
          and(
            eq(importJobs.id, job.id),
            inArray(importJobs.status, [`draft`, `previewing`])
          )
        )
        .returning({ id: importJobs.id })
      if (discarded) return { ok: true, discarded: true }
      const [row] = await ctx.db
        .update(importJobs)
        .set({ status: `cancelled`, credential: null, claimToken: null, finishedAt: new Date() })
        .where(
          and(
            eq(importJobs.id, job.id),
            inArray(importJobs.status, [`draft`, `previewing`, `ready`, `running`, `failed`])
          )
        )
        .returning({ id: importJobs.id })
      if (!row) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The import already finished` })
      return { ok: true, discarded: false }
    }),

  // EXP-1076: clear the Recent imports list. Only FINISHED jobs (a live one
  // would reappear the moment it writes) and only the card: `dismissed_at`
  // hides the row from `list` and stays out of `jobSelection`, while
  // import_entity_map is untouched — a re-import must still recognise what
  // this job already created.
  clearHistory: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertOwner(ctx.session.user.id, input.teamId)
      const rows = await ctx.db
        .update(importJobs)
        .set({ dismissedAt: new Date() })
        .where(
          and(
            eq(importJobs.teamId, input.teamId),
            inArray(importJobs.status, [...IMPORT_TERMINAL_STATUSES]),
            isNull(importJobs.dismissedAt)
          )
        )
        .returning({ id: importJobs.id })
      return { cleared: rows.length }
    }),
})

