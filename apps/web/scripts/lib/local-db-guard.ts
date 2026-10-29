// EXP-913: the screenshot scripts WRITE — `seed:screenshots` destroys and
// re-creates the demo team, and `screenshots:desktop` upserts a device row and
// shifts the seeded timestamps on every beat. Pointed at staging or production
// either one would touch a real team and real accounts, so both refuse
// anything that is not obviously a local dev database: the loopback hosts, or
// the dev compose Postgres port (54321) on whatever host name resolves to it.
// `--allow-remote` is the deliberate, typed-out override.
//
// One guard, shared: a second copy is a second thing to forget to call.

export const ALLOW_REMOTE_FLAG = `--allow-remote`

export function databaseIsLocal(raw: string | undefined): boolean {
  if (!raw) return false
  let url: URL
  try {
    url = new URL(raw)
  } catch {
    return false
  }
  // `new URL('postgres://h@[::1]:5432/db').hostname` keeps the brackets.
  const host = url.hostname.replace(/^\[|\]$/g, ``).toLowerCase()
  return (
    host === `localhost` ||
    host === `127.0.0.1` ||
    host === `::1` ||
    url.port === `54321`
  )
}

/**
 * Throw unless `DATABASE_URL` is a local dev database.
 *
 * `consequence` is the sentence the refusal leads with — say what this script
 * would DO to the database it is pointed at, so the person reading the error
 * knows what they nearly did.
 */
export function assertLocalDatabase(consequence: string): void {
  if (process.argv.includes(ALLOW_REMOTE_FLAG)) return
  const raw = process.env.DATABASE_URL
  if (!raw) throw new Error(`DATABASE_URL is not set`)
  try {
    new URL(raw)
  } catch {
    throw new Error(`DATABASE_URL is not a parsable URL`)
  }
  if (databaseIsLocal(raw)) return
  const host = new URL(raw).hostname.replace(/^\[|\]$/g, ``).toLowerCase()
  throw new Error(
    `Refusing to run against "${host}": ${consequence} ` +
      `Point DATABASE_URL at the local dev database (localhost, 127.0.0.1, ::1, or port 54321), or pass ${ALLOW_REMOTE_FLAG} if you truly mean this one.`
  )
}

// F27 (2026-10-29): "local" is not enough. The SHARED dev DB (the compose
// Postgres on :54321, database `exponential`) is every worktree's and every
// lane's at once: a seed tears its demo team down under a live check, a prune
// deletes device rows another run registered (twice on 2026-10-28). The
// capture lanes run on temp DBs (`exp_<x>` on the same port) or a throwaway
// stack; only the default `bun run shots` path, which seeds the shared DB on
// purpose, types the override. packages/shots/src/capture-db.ts is the same
// rule seen from the orchestrator (it refuses a throwaway stack pointed here).

export const SHARED_DEV_DB_FLAG = `--shared-dev-db`

/** Loopback (or any host) on :54321, database `exponential`: the shared dev DB. */
export function databaseIsSharedDev(raw: string | undefined): boolean {
  if (!raw) return false
  let url: URL
  try {
    url = new URL(raw)
  } catch {
    return false
  }
  const host = url.hostname.replace(/^\[|\]$/g, ``).toLowerCase()
  const loopback = host === `localhost` || host === `127.0.0.1` || host === `::1`
  return loopback && url.port === `54321` && decodeURIComponent(url.pathname.replace(/^\//, ``)) === `exponential`
}

/** Throw when `DATABASE_URL` is the shared dev DB, unless `--shared-dev-db`. */
export function assertNotSharedDevDatabase(consequence: string): void {
  if (process.argv.includes(SHARED_DEV_DB_FLAG)) return
  if (!databaseIsSharedDev(process.env.DATABASE_URL)) return
  throw new Error(
    `Refusing to run against the SHARED dev DB (localhost:54321/exponential): ${consequence} ` +
      `Point DATABASE_URL at a temp DB (e.g. .../exp_<you> on the same port) or a throwaway stack, or pass ${SHARED_DEV_DB_FLAG} if you truly mean the shared one.`
  )
}
