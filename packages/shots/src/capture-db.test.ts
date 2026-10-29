import { describe, expect, test } from "bun:test"

import { databaseUrlFromDotenv, resolveCaptureDb, sameDatabase, sharedDevDbArgs } from "./capture-db.ts"

const SHARED = `postgres://exp:exp@localhost:54321/exponential`
const TEMP = `postgres://exp:exp@localhost:54321/exp_capture_tmp`

describe(`resolveCaptureDb`, () => {
  test(`the default stack keeps the .env database`, () => {
    expect(resolveCaptureDb({ env: {}, sharedUrl: SHARED })).toEqual({ env: {} })
  })

  test(`a throwaway stack hands its exported DATABASE_URL to the helpers`, () => {
    expect(resolveCaptureDb({ env: { SHOTS_DEV_URL: `http://localhost:5181`, DATABASE_URL: TEMP }, sharedUrl: SHARED })).toEqual({
      env: { DATABASE_URL: TEMP },
    })
  })

  test(`SHOTS_DATABASE_URL wins over DATABASE_URL`, () => {
    expect(
      resolveCaptureDb({ env: { CAPTURE_BASE_URL: `http://localhost:5181`, SHOTS_DATABASE_URL: TEMP, DATABASE_URL: SHARED }, sharedUrl: SHARED })
    ).toEqual({ env: { DATABASE_URL: TEMP } })
  })

  test(`a throwaway stack without an exported DB is refused (the helpers would read apps/web/.env)`, () => {
    expect(resolveCaptureDb({ env: { SHOTS_DEV_URL: `http://localhost:5181` }, sharedUrl: SHARED })).toHaveProperty(`refused`)
  })

  test(`a throwaway stack pointed at the shared dev DB is refused, whatever its spelling`, () => {
    const env = { SHOTS_DEV_URL: `http://localhost:5181/`, DATABASE_URL: `postgresql://other:pw@127.0.0.1:54321/exponential?sslmode=disable` }
    expect(resolveCaptureDb({ env, sharedUrl: SHARED })).toHaveProperty(`refused`)
  })

  test(`the default dev URL is not a throwaway stack`, () => {
    expect(resolveCaptureDb({ env: { SHOTS_DEV_URL: `http://localhost:5173` }, sharedUrl: SHARED })).toEqual({ env: {} })
  })
})

describe(`helpers`, () => {
  test(`databaseUrlFromDotenv reads quoted and exported lines`, () => {
    expect(databaseUrlFromDotenv(`# x\nexport DATABASE_URL="${SHARED}"\nOTHER=1`)).toBe(SHARED)
    expect(databaseUrlFromDotenv(`OTHER=1`)).toBeUndefined()
  })

  test(`sameDatabase compares host, port and name`, () => {
    expect(sameDatabase(SHARED, `postgres://a:b@127.0.0.1:54321/exponential`)).toBe(true)
    expect(sameDatabase(SHARED, TEMP)).toBe(false)
  })
})

describe(`sharedDevDbArgs`, () => {
  test(`passes --shared-dev-db on the default stack, which seeds the shared dev DB on purpose`, () => {
    expect(sharedDevDbArgs({})).toEqual([`--shared-dev-db`])
    expect(sharedDevDbArgs({ SHOTS_DEV_URL: `http://localhost:5173` })).toEqual([`--shared-dev-db`])
  })

  test(`never passes it for a throwaway stack, so the helper refuses the shared DB too`, () => {
    expect(sharedDevDbArgs({ SHOTS_DEV_URL: `http://localhost:5181` })).toEqual([])
    expect(sharedDevDbArgs({ CAPTURE_BASE_URL: `http://127.0.0.1:5181/` })).toEqual([])
  })
})
