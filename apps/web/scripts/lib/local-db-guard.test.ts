import { afterEach, describe, expect, it } from "vitest"
import {
  assertLocalDatabase,
  databaseIsLocal,
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
