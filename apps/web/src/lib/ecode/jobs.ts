// #EXP-1616: eCode's git jobs run on a TEAM DEVICE, never here. A job is a
// `device_commands` row of kind `ecode_job` (+ the relay nudge); the daemon
// (`crates/cli` `ecode_jobs.rs`) runs it with `ecode::LocalRepo` on a cached
// clone of the repo it fetches from the bucket, and completes the row with a
// JSON answer (`{ok, result}` or `{ok: false, reason, message}`). Jobs only
// write OBJECTS (a pack: the squash commit, a rebase); every ref move stays
// the server's CAS.
//
// Which device: an online one of the team that advertises the `ecode-jobs`
// cap: a server shared with the team first, then any member's daemon. The
// payload carries a short-lived JOB token (`repo-token.ts`, scope `job`) for
// `/api/ecode/git/*`: the device needs no other credential.
import { and, arrayContains, desc, eq, gt, sql } from "drizzle-orm"
import type { ForgeReason } from "@exp/ecode"
import { db } from "@/db/connection"
import { deviceCommands, devices, teamMembers } from "@/db/schema"
import { ForgeError, type ForgeRepo } from "@/lib/ecode/forge"
import type { EcodeJob, EcodeJobOutcome } from "@/lib/ecode/forge-ecode"
import { mintRepoToken } from "@/lib/ecode/repo-token"

export const ECODE_JOB_KIND = `ecode_job`
export const ECODE_JOBS_CAP = `ecode-jobs`
/** A device seen this recently is online enough to run a job. */
const ONLINE_MS = 2 * 60_000

/** The maintenance jobs (fire-and-forget: nobody waits for them). */
export type EcodeMaintenanceJob =
  | { op: `backup` }
  | { op: `mirror`; url: string; username?: string; token?: string }
  | { op: `repack` }
  | { op: `restore`; bundle: string }

/** How long a caller waits for each job's answer. */
const WAIT_MS: Record<EcodeJob[`op`], number> = {
  mergeability: 60_000,
  squash: 120_000,
  files: 60_000,
  rebase_onto: 120_000,
  is_ancestor: 30_000,
  read_file: 60_000,
}

export interface JobDevice {
  rowId: string
  deviceId: string
  ownerId: string
}

/** The device that runs the team's jobs now, null = none online. */
export async function pickJobDevice(teamId: string, now = new Date()): Promise<JobDevice | null> {
  const [row] = await db
    .select({ rowId: devices.id, deviceId: devices.deviceId, ownerId: devices.userId })
    .from(devices)
    .innerJoin(teamMembers, and(eq(teamMembers.userId, devices.userId), eq(teamMembers.teamId, teamId)))
    .where(and(gt(devices.lastSeenAt, new Date(now.getTime() - ONLINE_MS)), sql`${devices.caps} @> ${JSON.stringify([ECODE_JOBS_CAP])}::jsonb`))
    .orderBy(desc(sql`coalesce(${arrayContains(devices.sharedTeamIds, [teamId])}, false)`), desc(devices.lastSeenAt))
    .limit(1)
  return row ?? null
}

/** Queues one job on `device` (+ the nudge); returns the command id. */
export async function queueJob(
  repo: { id: string; teamId: string },
  device: JobDevice,
  job: EcodeJob | EcodeMaintenanceJob
): Promise<string> {
  const { token } = mintRepoToken({ repositoryId: repo.id, userId: device.ownerId, scope: `job` })
  const [command] = await db
    .insert(deviceCommands)
    .values({
      deviceRowId: device.rowId,
      userId: device.ownerId,
      kind: ECODE_JOB_KIND,
      payload: { repositoryId: repo.id, op: job.op, args: JSON.stringify(job), token },
    })
    .returning({ id: deviceCommands.id })
  const { nudgeDevice } = await import(`@/lib/trpc/devices`)
  nudgeDevice(device.ownerId, device.deviceId)
  return command!.id
}

/** Parses a device's answer (the completed row's `result`). */
export function parseJobAnswer<K extends EcodeJob[`op`]>(status: string, result: string | null): EcodeJobOutcome<K> {
  try {
    const parsed = JSON.parse(result ?? ``) as { ok?: boolean; result?: unknown; reason?: ForgeReason; message?: string }
    if (status === `done` && parsed.ok === true) return { ok: true, result: parsed.result as never }
    if (parsed.ok === false && parsed.reason) return { ok: false, reason: parsed.reason, message: parsed.message ?? parsed.reason }
  } catch {
    // A failure without a JSON answer (a crash, an old daemon).
  }
  throw new ForgeError(502, `The device's eCode job failed: ${result ?? status}`)
}

/** One job, awaited: the device's answer, or ForgeError 503 (no device / no
 *  answer in time; the row is withdrawn so a late device skips it). */
export async function runJob<K extends EcodeJob[`op`]>(
  repo: ForgeRepo & { teamId: string },
  job: Extract<EcodeJob, { op: K }>
): Promise<EcodeJobOutcome<K>> {
  const device = await pickJobDevice(repo.teamId)
  if (!device) throw new ForgeError(503, `No team device with eCode jobs is online to run ${job.op}`, `runner`)
  const id = await queueJob(repo, device, job)
  const { awaitDeviceCommand } = await import(`@/lib/trpc/devices`)
  const outcome = await awaitDeviceCommand(db, id, WAIT_MS[job.op])
  if (!outcome) {
    await db
      .update(deviceCommands)
      .set({ status: `failed`, result: `timed out`, completedAt: new Date() })
      .where(and(eq(deviceCommands.id, id), eq(deviceCommands.status, `pending`)))
    throw new ForgeError(503, `The team device did not answer the ${job.op} job in time`, `runner`)
  }
  return parseJobAnswer<K>(outcome.status, outcome.result)
}

/** A maintenance job, not awaited. One pending per (repo, op): a second
 *  landing while a backup is queued reuses it. Null = no device online. */
export async function queueMaintenance(repo: { id: string; teamId: string }, job: EcodeMaintenanceJob): Promise<string | null> {
  const [pending] = await db
    .select({ id: deviceCommands.id })
    .from(deviceCommands)
    .where(
      and(
        eq(deviceCommands.kind, ECODE_JOB_KIND),
        eq(deviceCommands.status, `pending`),
        sql`${deviceCommands.payload}->>'repositoryId' = ${repo.id}`,
        sql`${deviceCommands.payload}->>'op' = ${job.op}`
      )
    )
    .limit(1)
  if (pending) return pending.id
  const device = await pickJobDevice(repo.teamId)
  if (!device) return null
  return queueJob(repo, device, job)
}
