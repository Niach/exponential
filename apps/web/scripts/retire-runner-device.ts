/**
 * #EXP-2174: retire a separate CI daemon device (mint-ci) into the desktop
 * device of the same computer (mint), whose app now runs the jobs itself.
 *
 * In ONE transaction:
 *   1. the target device takes the source's runner settings (`devices.runner`:
 *      enabled, labels, slots, timeout; the new `memoryMaxMb` / `pauseWhenBusy`
 *      keep the target's values or the defaults), unless --keep-target-settings;
 *   2. every `ci_jobs` row the source ran or was dispatched moves to the target
 *      (`runner_device_id`, `dispatched_device_id`), so its history reads as
 *      the target's in the runner section and the run pages;
 *   3. the source's runner is turned off (`enabled: false`), so nothing is
 *      dispatched to it again. The device row itself stays: its owner removes
 *      it in Settings › Devices once its daemon is stopped.
 *
 * Refuses when the two devices belong to different people, when the target
 * cannot run jobs (no `ci-runner` cap), or while the source has a job in
 * progress (run again once it finished, or pass --allow-running to let the
 * server's clock re-queue it).
 *
 * Refuses --apply while the source serves GitHub Actions self-hosted runners
 * for opted-in repositories (the desktop app hosts none) unless
 * --accept-actions-loss.
 *
 * Usage (from apps/web; dry run unless --apply; ids as `exponential_devices`
 * lists them):
 *   bun scripts/retire-runner-device.ts --from <device uuid> --to <device uuid>
 *   bun scripts/retire-runner-device.ts --from <uuid> --to <uuid> --apply
 */
import { and, eq, inArray, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { ciJobs, devices, repositories, teamMembers, type DeviceRunnerSettings } from "@/db/schema"

// What a read-only check on mint found on 2026-11-27 (session 3586166b, for
// the coordinator): the main repo's `exp-linux` runners mint-1 and mint-2 are
// INDEPENDENT of the mint-ci daemon. They are the Docker compose project
// ~/exp-runners (services runner-1/runner-2, registered with their own token
// in compose.yaml, config in their own volumes, restart: unless-stopped,
// cgroup expci.slice); the gaming hook (exp-gaming.service → gaming-ci.sh)
// only stops/starts those containers. The daemon's own `actions-runner` dir
// holds just the downloaded runner tarball (no configured runner), so its
// `actions-runner` cap serves whatever the query below lists, nothing else.
const MINT_FINDING = [
  `mint-1 / mint-2 (exp-linux, the main repo's CI) are Docker containers under ~/exp-runners, registered and restarted on their own; retiring mint-ci leaves them untouched.`,
  `exponential-ci-daemon.service (mint-ci) is only the eCode runner + GitHub Actions runners for repos that opted in (listed below).`,
]

function arg(name: string): string | null {
  const at = process.argv.indexOf(name)
  return at >= 0 ? (process.argv[at + 1] ?? null) : null
}

async function main() {
  const from = arg(`--from`)
  const to = arg(`--to`)
  const apply = process.argv.includes(`--apply`)
  const keepTarget = process.argv.includes(`--keep-target-settings`)
  const allowRunning = process.argv.includes(`--allow-running`)
  const acceptActionsLoss = process.argv.includes(`--accept-actions-loss`)
  if (!from || !to || from === to) {
    console.error(`usage: --from <device uuid> --to <device uuid> [--apply] [--keep-target-settings] [--allow-running] [--accept-actions-loss]`)
    process.exit(2)
  }

  const rows = await db
    .select({
      id: devices.id,
      deviceId: devices.deviceId,
      userId: devices.userId,
      label: devices.label,
      kind: devices.kind,
      caps: devices.caps,
      runner: devices.runner,
    })
    .from(devices)
    // A device is named by its client id (`exponential_devices` lists it as
    // `deviceId`) or its row id.
    .where(or(inArray(devices.deviceId, [from, to]), inArray(devices.id, [from, to])))
  if (rows.length > 2) throw new Error(`a device id matches more than one device row: pass the row ids`)
  const source = rows.find((row) => row.deviceId === from || row.id === from)
  const target = rows.find((row) => row.deviceId === to || row.id === to)
  if (!source || !target) throw new Error(`no such device: ${!source ? from : to}`)
  if (source.userId !== target.userId) throw new Error(`the devices belong to different people`)
  if (!(target.caps ?? []).includes(`ci-runner`)) {
    throw new Error(`"${target.label}" cannot run CI jobs yet (no ci-runner cap: update its app, on Linux)`)
  }

  const [running] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(ciJobs)
    .where(and(eq(ciJobs.runnerDeviceId, source.id), eq(ciJobs.status, `in_progress`)))
  if ((running?.count ?? 0) > 0 && !allowRunning) {
    throw new Error(`"${source.label}" runs ${running!.count} job(s) now; retry when they finished`)
  }
  const [history] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(ciJobs)
    .where(or(eq(ciJobs.runnerDeviceId, source.id), eq(ciJobs.dispatchedDeviceId, source.id)))

  // GitHub Actions the source serves as self-hosted runners
  // (lib/ci/actions-runners.ts `actionsRunnerTargets`): the desktop app hosts
  // none, so these lose their runner on this computer.
  const servesActions = Boolean(source.runner?.enabled) && (source.caps ?? []).includes(`actions-runner`)
  const actionsRepos = servesActions
    ? await db
        .select({ fullName: repositories.fullName })
        .from(repositories)
        .where(
          and(
            inArray(repositories.teamId, db.select({ id: teamMembers.teamId }).from(teamMembers).where(eq(teamMembers.userId, source.userId))),
            eq(repositories.provider, `github`),
            sql`(${repositories.ciConfig}->>'actionsRunners')::boolean IS TRUE`,
            sql`${repositories.archivedAt} IS NULL`
          )
        )
    : []

  const settings: DeviceRunnerSettings | null = keepTarget
    ? target.runner
    : source.runner
      ? { ...(target.runner ?? {}), ...source.runner }
      : target.runner
  console.log(`[retire] "${source.label}" (${from}, ${source.kind}) → "${target.label}" (${to}, ${target.kind})`)
  console.log(`[retire] runner settings now: ${JSON.stringify(target.runner)}`)
  console.log(`[retire] runner settings after: ${JSON.stringify(settings)}`)
  console.log(`[retire] ci_jobs to re-point: ${history?.count ?? 0}`)
  for (const line of MINT_FINDING) console.log(`[retire] finding: ${line}`)
  console.log(
    `[retire] GitHub Actions served by "${source.label}": ${actionsRepos.length ? [...new Set(actionsRepos.map((r) => r.fullName))].join(`, `) : `none`}`
  )
  if (actionsRepos.length > 0 && !acceptActionsLoss) {
    console.log(`[retire] those repositories lose this computer's self-hosted runner: --apply also needs --accept-actions-loss`)
    if (apply) process.exit(1)
  }
  if (!apply) {
    console.log(`[retire] dry run: nothing written (pass --apply)`)
    process.exit(0)
  }

  await db.transaction(async (tx) => {
    if (settings) await tx.update(devices).set({ runner: settings }).where(eq(devices.id, target.id))
    await tx.update(ciJobs).set({ runnerDeviceId: target.id }).where(eq(ciJobs.runnerDeviceId, source.id))
    await tx.update(ciJobs).set({ dispatchedDeviceId: target.id }).where(eq(ciJobs.dispatchedDeviceId, source.id))
    if (source.runner) {
      await tx
        .update(devices)
        .set({ runner: { ...source.runner, enabled: false } })
        .where(eq(devices.id, source.id))
    }
  })
  console.log(`[retire] done. Stop the source's daemon, then its owner removes "${source.label}" in Settings › Devices.`)
  process.exit(0)
}

main().catch((err) => {
  console.error(`[retire] ${err instanceof Error ? err.message : String(err)}`)
  process.exit(1)
})
