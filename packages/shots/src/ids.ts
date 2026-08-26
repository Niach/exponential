/**
 * The seeded demo instance's runtime ids (EXP-566).
 *
 * The catalog is written against the seed but never hardcodes a uuid, because a
 * uuid only exists after `seed:screenshots` has run and changes on every reseed.
 * Desktop drives therefore carry PLACEHOLDERS — `issue:$APP-5`, `pr:$APP-14`,
 * `support:$thread`, `$emptyBoard` — and this module is the lookup that turns
 * them into the `EXP_DEV_*` values the app actually parses.
 *
 * The ids themselves come from `apps/web`'s `screenshots:ids` script, which is
 * the only thing that can see the database.
 */
import { run } from "./lib/proc.ts"
import { repoRoot } from "./paths.ts"
import { join } from "node:path"

export interface DemoIds {
  teamId: string
  boardId: string
  /** The seeded board with nothing on it — `$emptyBoard`. */
  emptyBoardId?: string
  /** Keyed by human identifier: `{ "APP-5": "<uuid>" }`. */
  issues: Record<string, string>
  supportThreadId?: string
  supportReporterThreadId?: string
  /**
   * The reporter's magic link — a CREDENTIAL, not an id. It rides this struct
   * so the browser lane can address `/support/<token>`; nothing in the pipeline
   * may print it, and it is absent whenever the host cannot mint one.
   */
  supportToken?: string
  actionId?: string
  deviceId?: string
  automationId?: string
}

/**
 * Placeholders that name ONE well-known seeded row rather than an issue.
 * Anything not in here is looked up as an issue identifier (`$APP-5`).
 */
const NAMED: Record<string, (ids: DemoIds) => string | undefined> = {
  thread: (ids) => ids.supportThreadId,
  action: (ids) => ids.actionId,
  device: (ids) => ids.deviceId,
  automation: (ids) => ids.automationId,
  board: (ids) => ids.boardId,
  emptyBoard: (ids) => ids.emptyBoardId,
  team: (ids) => ids.teamId,
  supportToken: (ids) => ids.supportToken,
}

/**
 * Pull the ids out of a script's stdout. The script prints JSON, but bun (and
 * anything it imports) is free to log first, so this takes the OUTERMOST JSON
 * object rather than assuming the whole stream parses.
 */
export function parseDemoIds(stdout: string): DemoIds {
  const start = stdout.indexOf(`{`)
  const end = stdout.lastIndexOf(`}`)
  if (start === -1 || end <= start) {
    throw new Error(`screenshots:ids printed no JSON object:\n${stdout.trim().slice(-2000)}`)
  }
  const parsed = JSON.parse(stdout.slice(start, end + 1)) as Partial<DemoIds>
  if (!parsed.teamId) throw new Error(`screenshots:ids output has no teamId`)
  return {
    teamId: parsed.teamId,
    boardId: parsed.boardId ?? ``,
    emptyBoardId: parsed.emptyBoardId,
    issues: parsed.issues ?? {},
    supportThreadId: parsed.supportThreadId,
    supportReporterThreadId: parsed.supportReporterThreadId,
    supportToken: parsed.supportToken,
    actionId: parsed.actionId,
    deviceId: parsed.deviceId,
    automationId: parsed.automationId,
  }
}

/** Run `bun run screenshots:ids` in `apps/web` and parse its output. */
export async function fetchDemoIds(): Promise<DemoIds> {
  const result = await run({
    cmd: [`bun`, `run`, `screenshots:ids`],
    cwd: join(repoRoot(), `apps/web`),
    timeoutMs: 120_000,
  })
  if (result.code !== 0) {
    throw new Error(
      `screenshots:ids failed (exit ${result.code}). Is the backend up and seeded?\n${result.stderr.trim().slice(-2000)}`
    )
  }
  return parseDemoIds(result.stdout)
}

/**
 * Substitute `$NAME` placeholders in a `DesktopDrive.value`.
 *
 * A handful of names (`$thread`, `$action`, `$device`, `$automation`, `$board`,
 * `$emptyBoard`, `$team`) point at one well-known seeded row; anything else is
 * looked up as an issue IDENTIFIER (`$APP-5`). The same substitution runs over a
 * view's `desktop.env` values, so `EXP_DEV_BOARD_ID=$emptyBoard` resolves too.
 * Returns `undefined` when a placeholder has no id — the
 * caller skips that view with a note, which is the honest outcome: a desktop
 * launched with an unresolvable `EXP_DEV_SCREEN` silently falls back to the
 * default screen and would photograph the wrong view under the right filename.
 */
export function resolveDriveValue(value: string, ids: DemoIds): string | undefined {
  if (!value.includes(`$`)) return value
  let missing: string | undefined
  const resolved = value.replace(/\$([A-Za-z][A-Za-z0-9_-]*)/g, (_match, name: string) => {
    const named = NAMED[name]
    const id = named ? named(ids) : ids.issues[name]
    if (!id) {
      missing = name
      return ``
    }
    return id
  })
  return missing ? undefined : resolved
}

/** Human-readable reason a placeholder could not be resolved. */
export function missingPlaceholder(value: string, ids: DemoIds): string {
  const names = [...value.matchAll(/\$([A-Za-z][A-Za-z0-9_-]*)/g)].map((match) => match[1]!)
  const unresolved = names.filter((name) => {
    const named = NAMED[name]
    return named ? !named(ids) : !ids.issues[name]
  })
  return unresolved.map((name) => `$${name}`).join(`, `)
}
