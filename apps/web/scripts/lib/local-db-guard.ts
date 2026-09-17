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
