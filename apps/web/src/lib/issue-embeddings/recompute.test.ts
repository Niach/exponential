// @vitest-environment node
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { parseBackfillArgs } from "./backfill-args"
import { ISSUE_EMBEDDING_DEBOUNCE_MS, ISSUE_EMBEDDING_MAX_WAIT_MS, KeyedDebouncer } from "./recompute"

describe(`KeyedDebouncer (the per-issue recompute debounce)`, () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it(`runs once, the delay after the LAST touch`, () => {
    const ran: string[] = []
    const debouncer = new KeyedDebouncer((key) => ran.push(key), 30_000, 120_000)
    debouncer.touch(`a`)
    vi.advanceTimersByTime(20_000)
    debouncer.touch(`a`)
    vi.advanceTimersByTime(29_999)
    expect(ran).toEqual([])
    vi.advanceTimersByTime(1)
    expect(ran).toEqual([`a`])
    expect(debouncer.size).toBe(0)
  })

  it(`keeps issues apart`, () => {
    const ran: string[] = []
    const debouncer = new KeyedDebouncer((key) => ran.push(key), 30_000, 120_000)
    debouncer.touch(`a`)
    vi.advanceTimersByTime(10_000)
    debouncer.touch(`b`)
    expect(debouncer.size).toBe(2)
    vi.advanceTimersByTime(20_000)
    expect(ran).toEqual([`a`])
    vi.advanceTimersByTime(10_000)
    expect(ran).toEqual([`a`, `b`])
  })

  it(`an issue under a steady stream of changes still runs at the ceiling`, () => {
    const ran: string[] = []
    const debouncer = new KeyedDebouncer((key) => ran.push(key), 30_000, 120_000)
    for (let elapsed = 0; elapsed < 120_000; elapsed += 10_000) {
      debouncer.touch(`a`)
      vi.advanceTimersByTime(10_000)
    }
    expect(ran).toEqual([`a`])
    // The next burst starts a new window.
    debouncer.touch(`a`)
    vi.advanceTimersByTime(30_000)
    expect(ran).toEqual([`a`, `a`])
  })

  it(`cancelAll drops what is pending`, () => {
    const ran: string[] = []
    const debouncer = new KeyedDebouncer((key) => ran.push(key), 30_000, 120_000)
    debouncer.touch(`a`)
    debouncer.cancelAll()
    vi.advanceTimersByTime(200_000)
    expect(ran).toEqual([])
  })

  it(`ships with a 30 s debounce under a 2 min ceiling`, () => {
    expect(ISSUE_EMBEDDING_DEBOUNCE_MS).toBe(30_000)
    expect(ISSUE_EMBEDDING_MAX_WAIT_MS).toBe(120_000)
  })
})

describe(`parseBackfillArgs`, () => {
  it(`defaults to everything, in batches of 25`, () => {
    expect(parseBackfillArgs([])).toEqual({ batch: 25, sleepMs: 250, limit: Number.POSITIVE_INFINITY, dryRun: false })
  })

  it(`reads every flag`, () => {
    expect(
      parseBackfillArgs([`--dry-run`, `--batch`, `50`, `--sleep-ms`, `0`, `--limit`, `1000`, `--team`, `0b6f3f0e-8a52-4a55-9d53-1c1f6f3f2b11`])
    ).toEqual({ batch: 50, sleepMs: 0, limit: 1000, dryRun: true, teamId: `0b6f3f0e-8a52-4a55-9d53-1c1f6f3f2b11` })
  })

  it(`refuses what it does not understand`, () => {
    expect(() => parseBackfillArgs([`--apply`])).toThrow(`unknown flag --apply`)
    expect(() => parseBackfillArgs([`--batch`, `0`])).toThrow(`--batch`)
    expect(() => parseBackfillArgs([`--batch`])).toThrow(`--batch`)
    expect(() => parseBackfillArgs([`--sleep-ms`, `1.5`])).toThrow(`--sleep-ms`)
    expect(() => parseBackfillArgs([`--team`, `acme`])).toThrow(`--team`)
  })
})
