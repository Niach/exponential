import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Context } from "@/lib/trpc"
import {
  deriveClientPlatform,
  resetClientPlatformThrottle,
  touchUserClientPlatform,
} from "./client-platforms"

function req(headers: Record<string, string>): Request {
  return new Request(`https://app.example/api/trpc/x`, { headers })
}

describe(`deriveClientPlatform`, () => {
  it(`reads the native header`, () => {
    expect(
      deriveClientPlatform(
        req({ authorization: `Bearer s`, "x-client-version": `ios/0.14.24` })
      )
    ).toEqual({ platform: `ios`, version: `0.14.24` })
    expect(
      deriveClientPlatform(req({ "x-client-version": `cli/0.14.31` }))
    ).toEqual({ platform: `cli`, version: `0.14.31` })
  })

  it(`treats a cookie-only request as the web app`, () => {
    expect(deriveClientPlatform(req({ cookie: `a=b` }))).toEqual({
      platform: `web`,
      version: null,
    })
    expect(deriveClientPlatform(req({}))).toEqual({
      platform: `web`,
      version: null,
    })
  })

  it(`skips token credentials without a header (api keys, MCP agents)`, () => {
    expect(
      deriveClientPlatform(req({ authorization: `Bearer expu_abc` }))
    ).toBeNull()
    expect(deriveClientPlatform(req({ "x-api-key": `expu_abc` }))).toBeNull()
  })

  it(`never lets a web/<v> header pose as a native platform`, () => {
    expect(
      deriveClientPlatform(
        req({ "x-client-version": `web/1.0.0`, cookie: `a=b` })
      )
    ).toEqual({ platform: `web`, version: null })
  })
})

function fakeDb() {
  const calls: unknown[] = []
  let reject = false
  const chain = {
    insert: vi.fn(() => chain),
    values: vi.fn((v: unknown) => {
      calls.push(v)
      return chain
    }),
    onConflictDoUpdate: vi.fn(() =>
      reject ? Promise.reject(new Error(`boom`)) : Promise.resolve()
    ),
  }
  return {
    db: chain as unknown as Context[`db`],
    calls,
    failNext: () => {
      reject = true
    },
  }
}

describe(`touchUserClientPlatform`, () => {
  beforeEach(() => {
    resetClientPlatformThrottle()
    vi.spyOn(console, `error`).mockImplementation(() => {})
  })

  it(`writes once per user+platform per interval`, () => {
    const { db, calls } = fakeDb()
    const t0 = 1_000_000
    expect(
      touchUserClientPlatform(db, {
        userId: `u1`,
        platform: `ios`,
        version: `1.0.0`,
        now: t0,
      })
    ).toBe(true)
    expect(
      touchUserClientPlatform(db, {
        userId: `u1`,
        platform: `ios`,
        version: `1.0.0`,
        now: t0 + 60_000,
      })
    ).toBe(false)
    // Another platform for the same user is its own slot.
    expect(
      touchUserClientPlatform(db, {
        userId: `u1`,
        platform: `web`,
        version: null,
        now: t0 + 60_000,
      })
    ).toBe(true)
    expect(
      touchUserClientPlatform(db, {
        userId: `u1`,
        platform: `ios`,
        version: `1.0.1`,
        now: t0 + 16 * 60_000,
      })
    ).toBe(true)
    expect(calls).toHaveLength(3)
    expect(calls[0]).toEqual({
      userId: `u1`,
      platform: `ios`,
      lastVersion: `1.0.0`,
    })
  })

  it(`clears the throttle slot when the write fails`, async () => {
    const { db, calls, failNext } = fakeDb()
    failNext()
    expect(
      touchUserClientPlatform(db, {
        userId: `u2`,
        platform: `android`,
        version: `2.0.0`,
        now: 5,
      })
    ).toBe(true)
    await Promise.resolve()
    await Promise.resolve()
    expect(
      touchUserClientPlatform(db, {
        userId: `u2`,
        platform: `android`,
        version: `2.0.0`,
        now: 6,
      })
    ).toBe(true)
    expect(calls).toHaveLength(2)
  })
})
