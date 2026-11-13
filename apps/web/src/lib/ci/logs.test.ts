import { beforeEach, describe, expect, it, vi } from "vitest"
import { ciJobs, teams } from "@/db/schema"

// #EXP-1617: the hourly ci-logs sweep. The bucket is an in-memory key set
// behind the faked `@/lib/storage`; the db answers which teams exist and
// which jobs still own a log prefix.

const bucket = new Set<string>()
vi.mock(`@/lib/storage`, () => ({
  uploadObject: vi.fn(),
  getObject: vi.fn(),
  deleteObject: vi.fn(async (key: string) => {
    bucket.delete(key)
  }),
  listKeys: vi.fn(async (prefix: string) => [...bucket].filter((key) => key.startsWith(prefix)).slice(0, 1000)),
  listPrefixes: vi.fn(async (prefix: string) => {
    const out = new Set<string>()
    for (const key of bucket) {
      if (!key.startsWith(prefix)) continue
      const rest = key.slice(prefix.length)
      const slash = rest.indexOf(`/`)
      if (slash >= 0) out.add(prefix + rest.slice(0, slash + 1))
    }
    return [...out].sort()
  }),
}))
vi.mock(`@/lib/billing`, () => ({ assertWithinStorageLimit: vi.fn() }))

let liveTeams: string[] = []
let liveLogKeys: string[] = []
vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: (table: unknown) => ({
        where: async () =>
          table === teams
            ? liveTeams.map((id) => ({ id }))
            : table === ciJobs
              ? liveLogKeys.map((logKey) => ({ logKey }))
              : [],
      }),
    }),
  },
}))

const { sweepOrphanLogs, chunkKey, CI_LOG_SWEEP_MS } = await import(`@/lib/ci/logs`)

const LIVE_TEAM = `11111111-1111-4111-8111-111111111111`
const GONE_TEAM = `22222222-2222-4222-8222-222222222222`
const LIVE_JOB = `33333333-3333-4333-8333-333333333333`
const GONE_JOB = `44444444-4444-4444-8444-444444444444`
const GONE_TEAM_JOB = `55555555-5555-4555-8555-555555555555`

let clock = 0
/** Each run is past the hourly throttle of the one before. */
const nextHour = () => new Date((clock += CI_LOG_SWEEP_MS + 1))

beforeEach(() => {
  bucket.clear()
  liveTeams = [LIVE_TEAM]
  liveLogKeys = [`ci-logs/${LIVE_TEAM}/${LIVE_JOB}`]
  for (const [team, job] of [
    [LIVE_TEAM, LIVE_JOB],
    [LIVE_TEAM, GONE_JOB],
    [GONE_TEAM, GONE_TEAM_JOB],
  ]) {
    for (const seq of [0, 1]) bucket.add(chunkKey(`ci-logs/${team}/${job}`, seq))
  }
  bucket.add(`issues/unrelated/a.png`)
})

describe(`sweepOrphanLogs`, () => {
  it(`deletes the chunks of an unknown team and an unknown job, keeps a live job's`, async () => {
    expect(await sweepOrphanLogs(nextHour())).toBe(4)
    expect([...bucket].sort()).toEqual(
      [
        chunkKey(`ci-logs/${LIVE_TEAM}/${LIVE_JOB}`, 0),
        chunkKey(`ci-logs/${LIVE_TEAM}/${LIVE_JOB}`, 1),
        `issues/unrelated/a.png`,
      ].sort()
    )
  })

  it(`keeps everything when every team and job is live`, async () => {
    liveTeams = [LIVE_TEAM, GONE_TEAM]
    liveLogKeys = [`ci-logs/${LIVE_TEAM}/${LIVE_JOB}`, `ci-logs/${LIVE_TEAM}/${GONE_JOB}`, `ci-logs/${GONE_TEAM}/${GONE_TEAM_JOB}`]
    expect(await sweepOrphanLogs(nextHour())).toBe(0)
    expect(bucket.size).toBe(7)
  })

  it(`runs at most hourly`, async () => {
    const at = nextHour()
    await sweepOrphanLogs(at)
    bucket.add(chunkKey(`ci-logs/${GONE_TEAM}/${GONE_TEAM_JOB}`, 0))
    expect(await sweepOrphanLogs(new Date(at.getTime() + 60_000))).toBe(0)
    expect(bucket.has(chunkKey(`ci-logs/${GONE_TEAM}/${GONE_TEAM_JOB}`, 0))).toBe(true)
  })
})
