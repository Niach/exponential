import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// Isolate from a real DB: bootstrap-cloud only touches db.execute (custom SQL)
// and db.update().set().where() (admin promotion).
const execute = vi.fn()
const updateWhere = vi.fn()
vi.mock(`@/db/connection`, () => ({
  db: {
    execute: (...args: unknown[]) => execute(...args),
    update: () => ({
      set: () => ({ where: (...args: unknown[]) => updateWhere(...args) }),
    }),
  },
}))
vi.mock(`@/lib/email-enabled`, () => ({ emailEnabled: false }))

// bootstrapCloud memoizes its promise in module state — re-import per test.
async function importModule() {
  vi.resetModules()
  return await import(`@/lib/bootstrap-cloud`)
}

describe(`bootstrap-cloud`, () => {
  beforeEach(() => {
    vi.clearAllMocks()
    execute.mockResolvedValue(undefined)
    updateWhere.mockResolvedValue(undefined)
    delete process.env.INITIAL_ADMIN_EMAILS
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
    delete process.env.INITIAL_ADMIN_EMAILS
  })

  it(`propagates a custom-SQL failure instead of swallowing it (REV-18)`, async () => {
    // Every statement in the file is idempotent, so any error is a real one —
    // the old catch-and-warn turned e.g. a missing TRIGGER privilege into a
    // silently trigger-less instance.
    execute.mockRejectedValueOnce(new Error(`permission denied`))
    const mod = await importModule()
    await expect(mod.runBootstrapPass()).rejects.toThrow(
      `applying 0001_triggers.sql failed`
    )
  })

  it(`resolves after one clean pass without logging errors`, async () => {
    const errorSpy = vi.spyOn(console, `error`).mockImplementation(() => {})
    const mod = await importModule()
    await mod.bootstrapCloud()
    expect(execute).toHaveBeenCalledTimes(1)
    expect(errorSpy).not.toHaveBeenCalled()
  })

  it(`retries a failed custom-SQL pass with backoff until it applies cleanly`, async () => {
    vi.useFakeTimers()
    const errorSpy = vi.spyOn(console, `error`).mockImplementation(() => {})
    execute
      .mockRejectedValueOnce(new Error(`connection refused`))
      .mockRejectedValueOnce(new Error(`connection refused`))
      .mockResolvedValue(undefined)
    const mod = await importModule()
    const promise = mod.bootstrapCloud()
    // Attempt 1 fails → 5s backoff, attempt 2 fails → 10s backoff, attempt 3
    // succeeds and the promise finally resolves.
    await vi.advanceTimersByTimeAsync(5_000)
    expect(execute).toHaveBeenCalledTimes(2)
    await vi.advanceTimersByTimeAsync(10_000)
    await promise
    expect(execute).toHaveBeenCalledTimes(3)
    expect(errorSpy).toHaveBeenCalledTimes(2)
  })

  it(`retries when admin promotion fails, re-running the idempotent SQL too`, async () => {
    vi.useFakeTimers()
    vi.spyOn(console, `error`).mockImplementation(() => {})
    process.env.INITIAL_ADMIN_EMAILS = `admin@example.com`
    updateWhere
      .mockRejectedValueOnce(new Error(`pool exhausted`))
      .mockResolvedValue(undefined)
    const mod = await importModule()
    const promise = mod.bootstrapCloud()
    await vi.advanceTimersByTimeAsync(5_000)
    await promise
    expect(updateWhere).toHaveBeenCalledTimes(2)
    expect(execute).toHaveBeenCalledTimes(2)
  })

  it(`returns the same in-flight promise on repeat calls`, async () => {
    const mod = await importModule()
    expect(mod.bootstrapCloud()).toBe(mod.bootstrapCloud())
    await mod.bootstrapCloud()
  })
})
