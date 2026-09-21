import { describe, expect, it } from "vitest"
import { runChain, type SessionRow } from "./run-chain"

// EXP-974: the resume chain (`lib/sessions/run-chain.ts`). The natives mirror
// these six cases by name: desktop `queries::run_chain` tests, iOS
// `RunChainTests`, Android `RunChainTest`.

const row = (
  id: string,
  resumedFromId: string | null,
  createdAt: string
): SessionRow => ({ id, resumedFromId, createdAt: new Date(createdAt) })

const ids = (rows: readonly SessionRow[]) => rows.map((r) => r.id)

const a = row(`a`, null, `2026-09-01T10:00:00Z`)
const b = row(`b`, `a`, `2026-09-01T11:00:00Z`)
const c = row(`c`, `b`, `2026-09-01T12:00:00Z`)
// An unrelated run of the same person, interleaved in time.
const x = row(`x`, null, `2026-09-01T11:30:00Z`)

describe(`runChain (EXP-974)`, () => {
  it(`returns just the session when nothing resumed it and it resumed nothing`, () => {
    expect(ids(runChain([a, x], `x`))).toEqual([`x`])
  })

  it(`walks resumedFromId backwards to the first row`, () => {
    expect(ids(runChain([c, x, a, b], `c`))).toEqual([`a`, `b`, `c`])
  })

  it(`walks forwards to the latest resume from a middle or first row`, () => {
    expect(ids(runChain([c, x, a, b], `a`))).toEqual([`a`, `b`, `c`])
    expect(ids(runChain([c, x, a, b], `b`))).toEqual([`a`, `b`, `c`])
  })

  it(`follows the newest successor at a fork`, () => {
    const c1 = row(`c1`, `b`, `2026-09-01T12:00:00Z`)
    const c2 = row(`c2`, `b`, `2026-09-01T13:00:00Z`)
    const d2 = row(`d2`, `c2`, `2026-09-01T14:00:00Z`)
    const rows = [a, b, c1, c2, d2]
    // From the root or the fork point: the newest branch, to its end.
    expect(ids(runChain(rows, `a`))).toEqual([`a`, `b`, `c2`, `d2`])
    expect(ids(runChain(rows, `b`))).toEqual([`a`, `b`, `c2`, `d2`])
    // From the older sibling: its own past plus itself — never the other
    // branch.
    expect(ids(runChain(rows, `c1`))).toEqual([`a`, `b`, `c1`])
    // Same stamp: the id breaks the tie, so every client picks the same row.
    const c3 = row(`c3`, `b`, `2026-09-01T13:00:00Z`)
    expect(ids(runChain([a, b, c2, c3], `b`))).toEqual([`a`, `b`, `c3`])
  })

  it(`yields [] for an unknown id`, () => {
    expect(runChain([a, b, c], `nope`)).toEqual([])
    expect(runChain([], `a`)).toEqual([])
  })

  it(`tolerates a predecessor the sweep deleted (dangling resumedFromId)`, () => {
    // `b` resumed `a`, but `a` is gone: the chain starts at `b`.
    expect(ids(runChain([b, c], `c`))).toEqual([`b`, `c`])
    expect(ids(runChain([b, c], `b`))).toEqual([`b`, `c`])
  })

  it(`never loops on a cyclic resumedFromId`, () => {
    const p = row(`p`, `q`, `2026-09-01T10:00:00Z`)
    const q = row(`q`, `p`, `2026-09-01T11:00:00Z`)
    expect(ids(runChain([p, q], `p`))).toEqual([`q`, `p`])
    expect(ids(runChain([p, q], `q`))).toEqual([`p`, `q`])
  })
})
