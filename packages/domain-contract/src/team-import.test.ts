import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/team-import.json" with { type: "json" }
import { chunkRows, mapUserId, matchStatuses, remapRow, rewriteLocalMentions, sessionArrival, validateChunk, TEAM_IMPORT_TABLES, type ImportRow } from "./team-import"

describe(`team import (EXP-1713)`, () => {
  test(`the tables are the server's, in dependency order`, () => {
    expect(TEAM_IMPORT_TABLES.indexOf(`boards`)).toBeLessThan(TEAM_IMPORT_TABLES.indexOf(`issues`))
    expect(TEAM_IMPORT_TABLES.indexOf(`sessions`)).toBeLessThan(TEAM_IMPORT_TABLES.indexOf(`messages`))
    expect(TEAM_IMPORT_TABLES.indexOf(`messages`)).toBeLessThan(TEAM_IMPORT_TABLES.indexOf(`attachments`))
  })
  test.each(fixture.validateChunk.cases)(`validateChunk: $name`, (c) => {
    const repeat = (c as { rowsRepeat?: { table: string; times: number; bodyBytes?: number } }).rowsRepeat
    const rows: ImportRow[] = repeat
      ? Array.from({ length: repeat.times }, (_, i) => ({ table: repeat.table, id: `r${i}`, row: { id: `r${i}`, ...(repeat.bodyBytes ? { body: `x`.repeat(repeat.bodyBytes) } : {}) } }))
      : ((c as { rows?: ImportRow[] }).rows ?? [])
    expect(validateChunk(rows)).toBe(c.error)
  })
  test.each(fixture.identity.cases)(`mapUserId: $name`, (c) => {
    expect(mapUserId(c.value, c.localUserId, c.callerId, c.required)).toEqual(c.expected)
  })
  test.each(fixture.mentions.cases)(`rewriteLocalMentions: $name`, (c) => {
    expect(rewriteLocalMentions(c.text, c.localEmail, c.callerEmail)).toBe(c.expected)
  })
  test.each(fixture.statusMatch.cases)(`matchStatuses: $name`, (c) => {
    expect(matchStatuses(c.local, fixture.statusMatch.team)).toEqual({ map: c.map, create: c.create })
  })
  test.each(fixture.remap.cases)(`remapRow: $name`, (c) => {
    const fresh = ((c as { fresh?: Record<string, string> }).fresh ?? {}) as Record<string, string>
    const lookup = (table: string, id: string) => (c.existing.includes(`${table}/${id}`) ? fresh[`${table}/${id}`]! : id)
    expect(remapRow(c.row, c.foreignKeys, lookup)).toEqual({ row: c.expected, remapped: c.remapped })
  })
  test.each(fixture.chunking.cases)(`chunkRows: $name`, (c) => {
    const got = chunkRows(c.rows, { maxRows: c.maxRows, maxBytes: c.maxBytes }).map((chunk) => chunk.map((r) => `${r.table}/${r.id}`))
    expect(got).toEqual(c.expected)
  })
  test(`a session arrives ended and private`, () => {
    const row = sessionArrival({ id: `s1`, status: `running`, shared: true, busy: true, needs_input: true, blocked: `x`, ended_at: null }, `2026-11-06T00:00:00.000Z`)
    expect(row).toEqual({ id: `s1`, status: `ended`, shared: false, busy: false, needs_input: false, blocked: null, ended_at: `2026-11-06T00:00:00.000Z` })
    expect(sessionArrival({ ended_at: `2026-01-01T00:00:00.000Z` }, `now`).ended_at).toBe(`2026-01-01T00:00:00.000Z`)
  })
})
