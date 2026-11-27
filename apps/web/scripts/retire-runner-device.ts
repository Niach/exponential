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
 * Usage (from apps/web; dry run unless --apply):
 *   bun scripts/retire-runner-device.ts --from <device uuid> --to <device uuid>
 *   bun scripts/retire-runner-device.ts --from <uuid> --to <uuid> --apply
 */
import { and, eq, inArray, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { ciJobs, devices } from "@/db/schema"

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
  if (!from || !to || from === to) {
    console.error(`usage: --from <device uuid> --to <device uuid> [--apply] [--keep-target-settings] [--allow-running]`)
    process.exit(2)
  }

  const rows = await db
    .select({
      id: devices.id,
      userId: devices.userId,
      label: devices.label,
      kind: devices.kind,
      caps: devices.caps,
      runner: devices.runner,
    })
    .from(devices)
    .where(inArray(devices.id, [from, to]))
  const source = rows.find((row) => row.id === from)
  const target = rows.find((row) => row.id === to)
  if (!source || !target) throw new Error(`no such device: ${!source ? from : to}`)
  if (source.userId !== target.userId) throw new Error(`the devices belong to different people`)
  if (!(target.caps ?? []).includes(`ci-runner`)) {
    throw new Error(`"${target.label}" cannot run CI jobs yet (no ci-runner cap: update its app, on Linux)`)
  }

  const [running] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(ciJobs)
    .where(and(eq(ciJobs.runnerDeviceId, from), eq(ciJobs.status, `in_progress`)))
  if ((running?.count ?? 0) > 0 && !allowRunning) {
    throw new Error(`"${source.label}" runs ${running!.count} job(s) now; retry when they finished`)
  }
  const [history] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(ciJobs)
    .where(or(eq(ciJobs.runnerDeviceId, from), eq(ciJobs.dispatchedDeviceId, from)))

  const settings = keepTarget ? target.runner : { ...(target.runner ?? {}), ...(source.runner ?? {}) }
  console.log(`[retire] "${source.label}" (${from}, ${source.kind}) → "${target.label}" (${to}, ${target.kind})`)
  console.log(`[retire] runner settings now: ${JSON.stringify(target.runner)}`)
  console.log(`[retire] runner settings after: ${JSON.stringify(settings)}`)
  console.log(`[retire] ci_jobs to re-point: ${history?.count ?? 0}`)
  if (!apply) {
    console.log(`[retire] dry run: nothing written (pass --apply)`)
    process.exit(0)
  }

  await db.transaction(async (tx) => {
    if (settings) await tx.update(devices).set({ runner: settings }).where(eq(devices.id, to))
    await tx.update(ciJobs).set({ runnerDeviceId: to }).where(eq(ciJobs.runnerDeviceId, from))
    await tx.update(ciJobs).set({ dispatchedDeviceId: to }).where(eq(ciJobs.dispatchedDeviceId, from))
    if (source.runner) {
      await tx
        .update(devices)
        .set({ runner: { ...source.runner, enabled: false } })
        .where(eq(devices.id, from))
    }
  })
  console.log(`[retire] done. Stop the source's daemon, then its owner removes "${source.label}" in Settings › Devices.`)
  process.exit(0)
}

main().catch((err) => {
  console.error(`[retire] ${err instanceof Error ? err.message : String(err)}`)
  process.exit(1)
})
