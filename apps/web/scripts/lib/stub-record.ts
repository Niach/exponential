/**
 * EXP-1558 / #EXP-1886: the running `screenshots:desktop` stub's record,
 * `$TMPDIR/exp-screenshot-stub-<worktree key>.json`. `bun run shots
 * --skip-seed` (packages/shots capture-all `runningStub`) reuses the stub it
 * names instead of starting a second one on the same relay.
 *
 * Written the moment the stub starts (`online: false`), not once it is up:
 * its eNet node takes minutes to compile and announce, and a lane that looked
 * in that window found no record, started a second stub, and that one's exit
 * deleted the record of the first, so every later lane started (and lost) a
 * stub of its own. A stub only ever removes its OWN record.
 */
import { readFileSync, renameSync, rmSync, writeFileSync } from "node:fs"
import { worktreeTmpPath } from "./worktree-tmp"

export interface StubRecord {
  pid: number
  relayUrl: string
  databaseUrl: string
  /** The banner printed: the demo machine is online and its feed replays. */
  online: boolean
}

/** The record's path for the checkout at `root`. */
export function stubRecordPath(root: string, dir?: string): string {
  return worktreeTmpPath(root, `exp-screenshot-stub`, `.json`, dir)
}

export function readStubRecord(file: string): StubRecord | undefined {
  try {
    const record = JSON.parse(readFileSync(file, `utf8`)) as Partial<StubRecord>
    if (typeof record.pid !== `number` || typeof record.relayUrl !== `string`) return undefined
    return {
      pid: record.pid,
      relayUrl: record.relayUrl,
      databaseUrl: record.databaseUrl ?? ``,
      // A record from before #EXP-1886 was only written once online.
      online: record.online ?? true,
    }
  } catch {
    return undefined
  }
}

/** Atomic: a lane polling the record never reads half of it. */
export function writeStubRecord(file: string, record: StubRecord): void {
  const tmp = `${file}.${record.pid}.tmp`
  writeFileSync(tmp, JSON.stringify(record))
  renameSync(tmp, file)
}

/** Remove the record only while it is still `pid`'s; another stub's stays. */
export function releaseStubRecord(file: string, pid: number): boolean {
  if (readStubRecord(file)?.pid !== pid) return false
  rmSync(file, { force: true })
  return true
}

/** Is `pid` alive? */
export function processAlive(pid: number): boolean {
  try {
    process.kill(pid, 0)
    return true
  } catch (error) {
    // EPERM = alive under another user.
    return (error as NodeJS.ErrnoException).code === `EPERM`
  }
}
