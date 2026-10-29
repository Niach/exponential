import { afterEach, describe, expect, it } from "vitest"
import {
  assertLocalDatabase,
  assertNotSharedDevDatabase,
  databaseIsLocal,
  databaseIsSharedDev,
  SHARED_DEV_DB_FLAG,
} from "./local-db-guard"

// EXP-913: both screenshot scripts write, and one of them deletes. The guard
// is the only thing between a mistyped DATABASE_URL and a real team.

const CONSEQUENCE = `this test does nothing at all.`

describe(`databaseIsLocal`, () => {
  it(`accepts the loopback hosts and the dev compose port`, () => {
    expect(databaseIsLocal(`postgres://u:p@localhost:5432/exp`)).toBe(true)
    expect(databaseIsLocal(`postgres://u:p@127.0.0.1:5432/exp`)).toBe(true)
    expect(databaseIsLocal(`postgres://u:p@[::1]:5432/exp`)).toBe(true)
    // Any host name that resolves to the dev compose Postgres port.
    expect(databaseIsLocal(`postgres://u:p@docker.internal:54321/exp`)).toBe(true)
  })

  it(`rejects anything else, including a missing or broken URL`, () => {
    expect(databaseIsLocal(`postgres://u:p@db.staging.example:5432/exp`)).toBe(false)
    expect(databaseIsLocal(undefined)).toBe(false)
    expect(databaseIsLocal(`not a url`)).toBe(false)
  })
})

describe(`assertLocalDatabase`, () => {
  const original = process.env.DATABASE_URL
  afterEach(() => {
    if (original === undefined) delete process.env.DATABASE_URL
    else process.env.DATABASE_URL = original
  })

  it(`passes for a local database`, () => {
    process.env.DATABASE_URL = `postgres://u:p@localhost:54321/exp`
    expect(() => assertLocalDatabase(CONSEQUENCE)).not.toThrow()
  })

  it(`names the host and the consequence when it refuses`, () => {
    process.env.DATABASE_URL = `postgres://u:p@db.prod.example:5432/exp`
    expect(() => assertLocalDatabase(CONSEQUENCE)).toThrow(/db\.prod\.example/)
    expect(() => assertLocalDatabase(CONSEQUENCE)).toThrow(/does nothing at all/)
  })

  it(`refuses an unset DATABASE_URL rather than guessing`, () => {
    delete process.env.DATABASE_URL
    expect(() => assertLocalDatabase(CONSEQUENCE)).toThrow(/not set/)
  })
})

// F27: the shared dev DB is local, so the local guard lets it through; the
// seed and the device prune must not touch it without the typed override.
describe(`databaseIsSharedDev`, () => {
  it(`matches the compose Postgres on :54321, database exponential, any loopback spelling`, () => {
    expect(databaseIsSharedDev(`postgresql://postgres:pw@localhost:54321/exponential`)).toBe(true)
    expect(databaseIsSharedDev(`postgresql://postgres:pw@127.0.0.1:54321/exponential`)).toBe(true)
    expect(databaseIsSharedDev(`postgresql://postgres:pw@[::1]:54321/exponential?sslmode=disable`)).toBe(true)
  })

  it(`allows temp DBs on the same port, other ports and other hosts`, () => {
    expect(databaseIsSharedDev(`postgresql://postgres:pw@localhost:54321/exp_rt1029`)).toBe(false)
    expect(databaseIsSharedDev(`postgresql://postgres:pw@localhost:5432/exponential`)).toBe(false)
    expect(databaseIsSharedDev(`postgresql://postgres:pw@db.staging.example:54321/exponential`)).toBe(false)
    expect(databaseIsSharedDev(undefined)).toBe(false)
    expect(databaseIsSharedDev(`not a url`)).toBe(false)
  })
})

describe(`assertNotSharedDevDatabase`, () => {
  const original = process.env.DATABASE_URL
  const argv = process.argv
  afterEach(() => {
    if (original === undefined) delete process.env.DATABASE_URL
    else process.env.DATABASE_URL = original
    process.argv = argv
  })

  it(`refuses the shared dev DB and names the override`, () => {
    process.env.DATABASE_URL = `postgresql://postgres:pw@localhost:54321/exponential`
    expect(() => assertNotSharedDevDatabase(CONSEQUENCE)).toThrow(/SHARED dev DB/)
    expect(() => assertNotSharedDevDatabase(CONSEQUENCE)).toThrow(/--shared-dev-db/)
  })

  it(`passes for a temp DB, and for the shared one with --shared-dev-db`, () => {
    process.env.DATABASE_URL = `postgresql://postgres:pw@localhost:54321/exp_rt1029`
    expect(() => assertNotSharedDevDatabase(CONSEQUENCE)).not.toThrow()
    process.env.DATABASE_URL = `postgresql://postgres:pw@localhost:54321/exponential`
    process.argv = [...argv, SHARED_DEV_DB_FLAG]
    expect(() => assertNotSharedDevDatabase(CONSEQUENCE)).not.toThrow()
  })
})
