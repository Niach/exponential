// #EXP-1617: CI logs live in the bucket as chunk objects
// (`ci-logs/<team>/<job>/<seq>`), appended by the runner (already redacted
// on the device) and read on demand: the run page fetches from an offset
// and tails while the job runs. Never synced. `ci_jobs.log_bytes` counts
// toward the team's storage; `log_chunks` (server-only) holds each chunk's
// byte length so a read finds its first chunk without listing the bucket.

import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { ciJobs, type CiJob } from "@/db/schema"
import { assertWithinStorageLimit } from "@/lib/billing"
import { deleteObject, getObject, uploadObject } from "@/lib/storage"

/** One chunk at most (the runner flushes about every second). */
export const CI_LOG_CHUNK_MAX = 256 * 1024
/** One job's log at most; beyond it the runner's appends are dropped. */
export const CI_LOG_JOB_MAX = 64 * 1024 * 1024
/** One read returns at most this much (the client reads again). */
export const CI_LOG_READ_MAX = 512 * 1024

export function chunkKey(logKey: string, seq: number): string {
  return `${logKey}/${String(seq).padStart(6, `0`)}`
}

export type AppendResult = { ok: true; next: number } | { ok: false; reason: `seq` | `full` | `storage` | `not_running` }

/** Append chunk `seq` (0-based; a repeat of a stored seq is a no-op). */
export async function appendLog(job: CiJob, seq: number, text: string): Promise<AppendResult> {
  if (job.status !== `in_progress` || !job.logKey) return { ok: false, reason: `not_running` }
  const chunks = job.logChunks
  if (seq < chunks.length) return { ok: true, next: chunks.length }
  if (seq !== chunks.length) return { ok: false, reason: `seq` }
  const body = new TextEncoder().encode(text)
  if (body.byteLength === 0) return { ok: true, next: chunks.length }
  if (body.byteLength > CI_LOG_CHUNK_MAX) return { ok: false, reason: `full` }
  if (job.logBytes + body.byteLength > CI_LOG_JOB_MAX) return { ok: false, reason: `full` }
  try {
    await assertWithinStorageLimit(job.teamId, body.byteLength)
  } catch {
    return { ok: false, reason: `storage` }
  }
  await uploadObject({ body, contentLength: body.byteLength, contentType: `text/plain; charset=utf-8`, key: chunkKey(job.logKey, seq) })
  const [moved] = await db
    .update(ciJobs)
    .set({
      logChunks: sql`${ciJobs.logChunks} || ${JSON.stringify([body.byteLength])}::jsonb`,
      logBytes: sql`${ciJobs.logBytes} + ${body.byteLength}`,
    })
    .where(and(eq(ciJobs.id, job.id), sql`jsonb_array_length(${ciJobs.logChunks}) = ${seq}`))
    .returning({ id: ciJobs.id })
  // A concurrent append of the same seq won: ours overwrote the same key
  // with the same bytes.
  return { ok: true, next: moved ? seq + 1 : seq + 1 }
}

export interface LogRead {
  /** Text from `offset` (whole chunks after the first). */
  text: string
  nextOffset: number
  /** The job finished and this is the end of its log. */
  complete: boolean
}

/** The log from byte `offset`, at most CI_LOG_READ_MAX. */
export async function readLog(job: CiJob, offset: number): Promise<LogRead> {
  const complete = job.status === `completed`
  if (!job.logKey) return { text: ``, nextOffset: 0, complete }
  const sizes = job.logChunks
  let start = 0
  let seq = 0
  while (seq < sizes.length && start + sizes[seq] <= offset) {
    start += sizes[seq]
    seq++
  }
  const parts: Uint8Array[] = []
  let read = 0
  let cursor = start
  while (seq < sizes.length && read < CI_LOG_READ_MAX) {
    const object = await getObject(chunkKey(job.logKey, seq))
    const bytes = object?.Body ? await object.Body.transformToByteArray() : new Uint8Array(sizes[seq])
    const skip = Math.max(0, offset - cursor)
    const slice = bytes.subarray(skip)
    parts.push(slice)
    read += slice.byteLength
    cursor += sizes[seq]
    seq++
  }
  const merged = new Uint8Array(read)
  let at = 0
  for (const part of parts) {
    merged.set(part, at)
    at += part.byteLength
  }
  const nextOffset = Math.max(offset, cursor)
  return {
    text: new TextDecoder().decode(merged),
    nextOffset,
    complete: complete && seq >= sizes.length,
  }
}

/** Every chunk object of the given jobs (a team or repo delete, pruning). */
export function logObjectKeys(jobs: ReadonlyArray<Pick<CiJob, `logKey` | `logChunks` | `reason`>>): string[] {
  // A re-run's reused job points at the old job's chunks: never twice.
  return [
    ...new Set(jobs.flatMap((job) => (job.logKey && job.reason !== `reused` ? job.logChunks.map((_, seq) => chunkKey(job.logKey!, seq)) : []))),
  ]
}

export async function deleteLogObjects(keys: readonly string[]): Promise<void> {
  for (const key of keys) await deleteObject(key).catch(() => {})
}
