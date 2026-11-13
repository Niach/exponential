// #EXP-1617 eCode's own CI (`lib/ci`). Two audiences:
// - the RUNNER (a team device's daemon, its owner's bearer): claim a
//   dispatched job, beat, append log chunks, take its secrets once, finish.
//   Every call after the claim carries the job's one-time token.
// - PEOPLE: read a job's log (on demand + tail; logs are never synced),
//   re-run, cancel, start a run by hand, the repo's CI settings and secrets,
//   and a device's runner settings (its owner).

import { TRPCError } from "@trpc/server"
import { and, eq } from "drizzle-orm"
import { z } from "zod"
import { LABEL_MAX, checkConclusionValues } from "@exp/ecode"
import { ciJobs, devices, repositories, type DeviceRunnerSettings, type RepositoryCiConfig } from "@/db/schema"
import { authedProcedure, router } from "@/lib/trpc"
import { assertTeamMember } from "@/lib/team-membership"
import { assertRepoManager } from "@/lib/trpc/repositories"
import { forgeFor } from "@/lib/ecode/forge"
import { repoInstallationId } from "@/lib/ecode/provider-config"
import {
  beatJob,
  cancelRun,
  claimJob,
  dispatchQueued,
  finishJob,
  jobForToken,
  loadRunForMember,
  rerunRun,
  startRun,
} from "@/lib/ci/runs"
import { appendLog, CI_LOG_CHUNK_MAX, readLog } from "@/lib/ci/logs"
import { actionsRunnerTargets, actionsRunnerToken } from "@/lib/ci/actions-runners"
import { CI_SECRET_NAME, CI_SECRET_VALUE_MAX, deleteSecret, listSecretNames, setSecret, takeJobSecrets } from "@/lib/ci/secrets"

const jobToken = z.object({ jobId: z.string().uuid(), token: z.string().min(16).max(128) })
const stepsInput = z.array(z.unknown()).max(256).optional()
const LABEL = new RegExp(`^[a-z0-9][a-z0-9._-]{0,${LABEL_MAX - 1}}$`)

async function memberRepo(userId: string, repositoryId: string) {
  const [repo] = await (await import(`@/db/connection`)).db
    .select({
      id: repositories.id,
      teamId: repositories.teamId,
      fullName: repositories.fullName,
      provider: repositories.provider,
      installationId: repoInstallationId(),
      sharedByUserId: repositories.sharedByUserId,
      ciConfig: repositories.ciConfig,
    })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  if (!repo) throw new TRPCError({ code: `NOT_FOUND`, message: `Repository not found` })
  await assertTeamMember(userId, repo.teamId)
  return repo
}

async function runningJob(jobId: string, token: string) {
  const job = await jobForToken(jobId, token)
  if (!job) throw new TRPCError({ code: `NOT_FOUND`, message: `Job not found` })
  return job
}

export const ciRouter = router({
  // ── the runner ────────────────────────────────────────────────────────
  claim: authedProcedure
    .input(z.object({ deviceId: z.string().min(1).max(128), jobId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const claimed = await claimJob({ userId: ctx.session.user.id, deviceId: input.deviceId, jobId: input.jobId })
      return { claimed }
    }),

  beat: authedProcedure.input(jobToken.extend({ steps: stepsInput })).mutation(async ({ input }) => {
    return beatJob({ jobId: input.jobId, token: input.token, steps: input.steps })
  }),

  appendLog: authedProcedure
    .input(jobToken.extend({ seq: z.number().int().min(0).max(1_000_000), text: z.string().max(CI_LOG_CHUNK_MAX) }))
    .mutation(async ({ input }) => {
      const job = await runningJob(input.jobId, input.token)
      return appendLog(job, input.seq, input.text)
    }),

  jobSecrets: authedProcedure.input(jobToken).mutation(async ({ input }) => {
    const job = await runningJob(input.jobId, input.token)
    const run = await loadRunForMember(job.runId)
    const values = await takeJobSecrets({ jobId: job.id, token: input.token, repositoryId: run.repositoryId })
    if (!values) throw new TRPCError({ code: `CONFLICT`, message: `This job's secrets were already handed out` })
    return { values }
  }),

  finish: authedProcedure
    .input(jobToken.extend({ conclusion: z.enum(checkConclusionValues), steps: stepsInput }))
    .mutation(async ({ input }) => {
      return finishJob({ jobId: input.jobId, token: input.token, conclusion: input.conclusion, steps: input.steps })
    }),

  // ── people ────────────────────────────────────────────────────────────
  log: authedProcedure
    .input(z.object({ jobId: z.string().uuid(), offset: z.number().int().min(0).default(0) }))
    .query(async ({ ctx, input }) => {
      const db = (await import(`@/db/connection`)).db
      const [job] = await db.select().from(ciJobs).where(eq(ciJobs.id, input.jobId)).limit(1)
      if (!job) throw new TRPCError({ code: `NOT_FOUND`, message: `Job not found` })
      await assertTeamMember(ctx.session.user.id, job.teamId)
      return readLog(job, input.offset)
    }),

  rerun: authedProcedure
    .input(z.object({ runId: z.string().uuid(), failedOnly: z.boolean().default(true) }))
    .mutation(async ({ ctx, input }) => {
      const run = await loadRunForMember(input.runId)
      await assertTeamMember(ctx.session.user.id, run.teamId)
      const runId = await rerunRun({ runId: run.id, failedOnly: input.failedOnly, userId: ctx.session.user.id })
      return { runId }
    }),

  cancel: authedProcedure.input(z.object({ runId: z.string().uuid() })).mutation(async ({ ctx, input }) => {
    const run = await loadRunForMember(input.runId)
    await assertTeamMember(ctx.session.user.id, run.teamId)
    await cancelRun(run.id)
    return { ok: true }
  }),

  /** A run by hand (`on.manual`) at a branch's head. */
  start: authedProcedure
    .input(z.object({ repositoryId: z.string().uuid(), branch: z.string().min(1).max(255) }))
    .mutation(async ({ ctx, input }) => {
      const repo = await memberRepo(ctx.session.user.id, input.repositoryId)
      const fr = { id: repo.id, fullName: repo.fullName, provider: repo.provider, installationId: repo.installationId }
      const sha = await (await forgeFor(fr)).readBranch(fr, input.branch)
      if (!sha) throw new TRPCError({ code: `NOT_FOUND`, message: `No branch ${input.branch}` })
      const result = await startRun({
        repositoryId: repo.id,
        sha,
        trigger: `manual`,
        ref: input.branch,
        createdByUserId: ctx.session.user.id,
      })
      if (!result.started) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: result.reason === `no_pipeline` ? `This repository has no .exponential/ci.json` : `The pipeline has no manual trigger`,
        })
      }
      return { runId: result.runId }
    }),

  repoSettings: authedProcedure.input(z.object({ repositoryId: z.string().uuid() })).query(async ({ ctx, input }) => {
    const repo = await memberRepo(ctx.session.user.id, input.repositoryId)
    return {
      config: repo.ciConfig as RepositoryCiConfig,
      secrets: await listSecretNames(repo.id),
    }
  }),

  setRepoConfig: authedProcedure
    .input(
      z.object({
        repositoryId: z.string().uuid(),
        requiredChecks: z.array(z.string().min(1).max(255)).max(64).optional(),
        githubStatuses: z.boolean().optional(),
        actionsRunners: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const repo = await memberRepo(ctx.session.user.id, input.repositoryId)
      await assertRepoManager(ctx.session.user.id, repo)
      const next: RepositoryCiConfig = {
        ...(repo.ciConfig as RepositoryCiConfig),
        ...(input.requiredChecks ? { requiredChecks: [...new Set(input.requiredChecks)] } : {}),
        ...(input.githubStatuses !== undefined ? { githubStatuses: input.githubStatuses } : {}),
        ...(input.actionsRunners !== undefined ? { actionsRunners: input.actionsRunners } : {}),
      }
      await ctx.db.update(repositories).set({ ciConfig: next }).where(eq(repositories.id, repo.id))
      return { config: next }
    }),

  setSecret: authedProcedure
    .input(
      z.object({
        repositoryId: z.string().uuid(),
        name: z.string().regex(CI_SECRET_NAME).max(128),
        value: z.string().min(1).max(CI_SECRET_VALUE_MAX),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const repo = await memberRepo(ctx.session.user.id, input.repositoryId)
      await assertRepoManager(ctx.session.user.id, repo)
      await setSecret({ teamId: repo.teamId, repositoryId: repo.id, name: input.name, value: input.value, userId: ctx.session.user.id })
      return { ok: true }
    }),

  deleteSecret: authedProcedure
    .input(z.object({ repositoryId: z.string().uuid(), name: z.string().min(1).max(128) }))
    .mutation(async ({ ctx, input }) => {
      const repo = await memberRepo(ctx.session.user.id, input.repositoryId)
      await assertRepoManager(ctx.session.user.id, repo)
      return { ok: await deleteSecret(repo.id, input.name) }
    }),

  /** #EXP-1617: the GitHub repos this device serves Actions for (its daemon). */
  actionsRunners: authedProcedure
    .input(z.object({ deviceId: z.string().min(1).max(128) }))
    .query(async ({ ctx, input }) => ({ targets: await actionsRunnerTargets(ctx.session.user.id, input.deviceId) })),

  /** #EXP-1617: a runner registration (or removal) token for one target. */
  actionsRunnerToken: authedProcedure
    .input(z.object({ deviceId: z.string().min(1).max(128), repositoryId: z.string().uuid(), kind: z.enum([`registration`, `remove`]) }))
    .mutation(async ({ ctx, input }) =>
      actionsRunnerToken({ userId: ctx.session.user.id, deviceId: input.deviceId, repositoryId: input.repositoryId, kind: input.kind })
    ),

  /** The device owner's runner settings (`devices.runner`). */
  setRunner: authedProcedure
    .input(
      z.object({
        deviceId: z.string().min(1).max(128),
        enabled: z.boolean(),
        labels: z.array(z.string().regex(LABEL)).max(16),
        slots: z.number().int().min(1).max(16),
        timeoutMin: z.number().int().min(1).max(360),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const runner: DeviceRunnerSettings = {
        enabled: input.enabled,
        labels: [...new Set(input.labels)].sort(),
        slots: input.slots,
        timeoutMin: input.timeoutMin,
      }
      const updated = await ctx.db
        .update(devices)
        .set({ runner })
        .where(and(eq(devices.userId, ctx.session.user.id), eq(devices.deviceId, input.deviceId)))
        .returning({ id: devices.id })
      if (updated.length === 0) throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
      if (runner.enabled) void dispatchQueued().catch((err) => console.error(`[ci] dispatch failed`, err))
      return { runner }
    }),
})
