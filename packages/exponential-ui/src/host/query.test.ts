import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/records-query.json" with { type: "json" }
import host from "../../catalog/host.json" with { type: "json" }
import {
  RECORDS_FILTER_OPS,
  RECORDS_OP_ALIASES,
  RECORDS_AGGREGATE_OPS,
  RECORDS_RUNNING_OPS,
  RECORDS_AGGREGATE_RESERVED,
  RECORDS_MONTHS,
  RECORDS_QUERY,
  RECORDS_RESERVED_PARAMS,
  RECORDS_ROW_KEYS,
  RECORDS_WEEKDAYS,
  RECORD_RESERVED_KEYS,
  MemoryRecordStore,
  parseRecordsQuery,
  parseRecordsJson,
  parseRecordsSource,
  expandQuerySource,
  parseSource,
  recordsValue,
  shiftMonth,
} from "."
import type { RecordRow, RecordsQuery } from "."
import { DATE_BUCKETS, dateBucket, type DateBucket } from "../dates"

// VAPP-189: the one records evaluator against fixtures/records-query.json
// (the Rust core, the Swift and the Compose SDK replay the same cases).

describe(`records query (VAPP-189)`, () => {
  test(`the constants are the fixture's and the catalog's`, () => {
    expect({
      ...RECORDS_QUERY,
      reserved: [...RECORDS_RESERVED_PARAMS].sort(),
      operators: [...RECORDS_FILTER_OPS],
      aliases: RECORDS_OP_ALIASES,
      aggregates: [...RECORDS_AGGREGATE_OPS],
      running: [...RECORDS_RUNNING_OPS],
      buckets: [...DATE_BUCKETS],
      aggregateReserved: [...RECORDS_AGGREGATE_RESERVED],
    } as unknown).toEqual({ ...fixture.query, reserved: [...fixture.query.reserved].sort() })
    expect([...RECORDS_RESERVED_PARAMS]).toEqual(host.package.records.query.reserved)
    expect([...RECORDS_ROW_KEYS]).toEqual(fixture.rowKeys)
    for (const key of RECORD_RESERVED_KEYS) expect(fixture.rowKeys).toContain(key)
    expect([...RECORDS_WEEKDAYS]).toEqual(fixture.weekdays)
    expect([...RECORDS_MONTHS]).toEqual(fixture.months)
  })
  test.each(fixture.parseCases)(`parse: $name`, (c) => {
    expect(parseRecordsQuery(c.params as unknown as Record<string, string>, (c as { kind?: string }).kind)).toEqual(c.expected as never)
  })
  test.each(fixture.sourceCases)(`source: $name`, (c) => {
    const p = parseSource(c.source)
    expect(p ? parseRecordsSource(p) : null).toEqual(c.expected as never)
  })
  test.each(fixture.jsonCases)(`json: $name`, (c) => {
    expect(parseRecordsJson(c.query, (c as { kind?: string }).kind)).toEqual(c.expected as never)
  })
  test.each(fixture.bucketCases)(`bucket: $date by $bucket`, (c) => {
    expect(dateBucket(c.date, c.bucket as DateBucket)).toBe(c.expected)
  })
  test.each(fixture.expandCases)(`expand: $name`, (c) => {
    expect(expandQuerySource(c.source, (c as { queries?: Record<string, unknown> }).queries)).toBe(c.expected)
  })
  test.each(fixture.valueCases)(`value: $name`, (c) => {
    const json = (c as { query?: unknown }).query
    const kind = (c as { kind?: string }).kind
    const query = json !== undefined ? parseRecordsJson(json, kind).query : parseRecordsQuery((c as unknown as { params: Record<string, string> }).params, kind)
    const computed = (c as { computed?: Record<string, string> }).computed
    expect(recordsValue(c.rows as RecordRow[], query, { me: c.me, now: c.now, today: (c as { today?: string }).today, names: c.names as Record<string, string>, computed })).toEqual(c.expected as never)
  })
  test(`shiftMonth crosses years both ways`, () => {
    expect(shiftMonth(`2026-01`, -1)).toBe(`2025-12`)
    expect(shiftMonth(`2026-12`, 1)).toBe(`2027-01`)
    expect(shiftMonth(`2026-03`, -15)).toBe(`2024-12`)
  })
  test(`several sort keys apply in order (the structured form)`, () => {
    const rows: RecordRow[] = [
      { id: `a`, kind: `t`, data: { p: 1, n: `b` }, createdBy: null, createdAt: ``, updatedAt: `` },
      { id: `b`, kind: `t`, data: { p: 2, n: `a` }, createdBy: null, createdAt: ``, updatedAt: `` },
      { id: `c`, kind: `t`, data: { p: 1, n: `a` }, createdBy: null, createdAt: ``, updatedAt: `` },
    ]
    const query: RecordsQuery = { ...parseRecordsQuery({}, `t`), sort: [{ field: `p`, order: `asc` }, { field: `n`, order: `desc` }] }
    expect(recordsValue(rows, query, { me: null })!.rows.map((r) => r.id)).toEqual([`a`, `c`, `b`])
  })
  test(`the evaluator reads any store's rows`, () => {
    const store = new MemoryRecordStore({ rows: fixture.valueCases[1]!.rows as RecordRow[] })
    const c = fixture.valueCases[1]!
    let seen: unknown = undefined
    store.subscribe((rows) => (seen = recordsValue(rows, parseRecordsQuery(c.params as Record<string, string>, (c as { kind?: string }).kind), { me: c.me, now: c.now, names: c.names as Record<string, string> })))
    expect(seen).toEqual(c.expected as never)
  })
})
