// VAPP-159: the app host's store maps only the rows a change names.

import { describe, expect, it, vi } from "vitest"
import { incrementalTable, memoByVersions, type ChangeSource, type TableChange } from "./incremental-table"

interface Row {
  id: string
  teamId: string
  n: number
}

/** A change feed like TanStack DB's: the initial state as inserts, then batches. */
function feed(initial: Row[]) {
  let listener: ((changes: TableChange<Row>[]) => void) | null = null
  const options: unknown[] = []
  const source: ChangeSource<Row> = {
    subscribeChanges: (cb, o) => {
      options.push(o)
      listener = cb
      cb(initial.map((value) => ({ type: `insert`, key: value.id, value })))
      return { unsubscribe: () => (listener = null) }
    },
  }
  return { source, options, send: (changes: TableChange<Row>[]) => listener?.(changes), live: () => listener !== null }
}

const rows = (count: number) => Array.from({ length: count }, (_, i) => ({ id: `r${i}`, teamId: i % 2 ? `t1` : `t2`, n: i }))

describe(`incrementalTable`, () => {
  it(`maps every row once, then only the rows a change names`, () => {
    const f = feed(rows(1000))
    const map = vi.fn((r: Row) => (r.teamId === `t1` ? { id: r.id, n: r.n } : null))
    const onChange = vi.fn()
    const table = incrementalTable(f.source, map, onChange)
    expect(map).toHaveBeenCalledTimes(1000)
    expect(table.rows()).toHaveLength(500)

    map.mockClear()
    f.send([{ type: `update`, key: `r1`, value: { id: `r1`, teamId: `t1`, n: 42 } }])
    expect(map).toHaveBeenCalledTimes(1)
    expect(onChange).toHaveBeenCalledTimes(2)
    expect(table.get(`r1`)).toEqual({ id: `r1`, n: 42 })
    expect(table.rows()).toHaveLength(500)
  })

  it(`a delete or a row the map drops leaves; an insert joins`, () => {
    const f = feed(rows(4))
    const table = incrementalTable(f.source, (r: Row) => (r.teamId === `t1` ? r.id : null))
    expect([...table.rows()].sort()).toEqual([`r1`, `r3`])
    f.send([
      { type: `delete`, key: `r1`, value: { id: `r1`, teamId: `t1`, n: 1 } },
      { type: `update`, key: `r3`, value: { id: `r3`, teamId: `t2`, n: 3 } },
      { type: `insert`, key: `r9`, value: { id: `r9`, teamId: `t1`, n: 9 } },
    ])
    expect(table.rows()).toEqual([`r9`])
  })

  it(`keeps one array between changes and passes the subscription's where`, () => {
    const f = feed(rows(2))
    const where = () => true
    const table = incrementalTable(f.source, (r: Row) => r, undefined, where)
    expect(f.options[0]).toEqual({ includeInitialState: true, where })
    const first = table.rows()
    expect(table.rows()).toBe(first)
    const v = table.version()
    f.send([{ type: `insert`, key: `r5`, value: { id: `r5`, teamId: `t1`, n: 5 } }])
    expect(table.version()).toBe(v + 1)
    expect(table.rows()).not.toBe(first)
    table.unsubscribe()
    expect(f.live()).toBe(false)
  })
})

describe(`memoByVersions`, () => {
  it(`recomputes only when an input version moved`, () => {
    let a = 0
    const compute = vi.fn(() => a)
    const memo = memoByVersions(() => [a, 7], compute)
    memo()
    memo()
    expect(compute).toHaveBeenCalledTimes(1)
    a = 1
    expect(memo()).toBe(1)
    expect(compute).toHaveBeenCalledTimes(2)
  })
})

describe(`over an ON DEMAND TanStack DB collection (app records, VAPP-159)`, () => {
  it(`a narrowed table loads that app's subset and follows only its rows`, async () => {
    const { createCollection, eq } = await import(`@tanstack/react-db`)
    interface Rec {
      id: string
      appId: string
    }
    const all: Rec[] = [
      { id: `a1`, appId: `A` },
      { id: `a2`, appId: `A` },
      { id: `b1`, appId: `B` },
    ]
    const loads: unknown[] = []
    let sync!: { begin: () => void; write: (m: { type: `insert`; value: Rec }) => void; commit: () => void }
    const collection = createCollection<Rec, string>({
      id: `records-on-demand`,
      getKey: (r) => r.id,
      syncMode: `on-demand`,
      sync: {
        sync: (params) => {
          sync = params as never
          params.markReady()
          return {
            loadSubset: (options) => {
              loads.push(options.where)
              // The proxy's subset: only app A's rows (what `eq(appId, A)` asks).
              params.begin()
              for (const value of all.filter((r) => r.appId === `A`)) params.write({ type: `insert`, value })
              params.commit()
              return true
            },
          }
        },
      },
    })
    const onChange = vi.fn()
    const table = incrementalTable(
      collection as unknown as ChangeSource<Rec>,
      (r: Rec) => r.id,
      onChange,
      (r: { appId: unknown }) => eq(r.appId as never, `A`)
    )
    await vi.waitFor(() => expect([...table.rows()].sort()).toEqual([`a1`, `a2`]))
    expect(loads).toHaveLength(1)
    // A live insert of another app's record never reaches the table.
    sync.begin()
    sync.write({ type: `insert`, value: { id: `b2`, appId: `B` } })
    sync.write({ type: `insert`, value: { id: `a3`, appId: `A` } })
    sync.commit()
    await vi.waitFor(() => expect([...table.rows()].sort()).toEqual([`a1`, `a2`, `a3`]))
    table.unsubscribe()
  })
})
