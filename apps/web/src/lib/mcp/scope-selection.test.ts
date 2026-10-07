import { describe, expect, it } from "vitest"
import {
  assertScopeSelectionNonEmpty,
  clampScopeSelection,
  scopeSelectionInput,
} from "@/lib/mcp/scope-selection"
import { createFakeDb } from "@/lib/mcp-oauth/test-db"

// FEED-76: ONE clamp behind the consent grant and the scoped API key mint —
// a selection never exceeds membership, trashed boards drop, nothing-left
// rejects.
const T1 = `11111111-1111-4111-8111-111111111111`
const T2 = `22222222-2222-4222-8222-222222222222`
const B1 = `33333333-3333-4333-8333-333333333333`
const B_OTHER = `44444444-4444-4444-8444-444444444444`

const db = createFakeDb({
  boards: [
    { id: B1, teamId: T1, deletedAt: null, archivedAt: null },
    { id: B_OTHER, teamId: `99999999-9999-4999-8999-999999999999` },
  ],
})
const members = new Set([T1, T2])
const input = (v: Record<string, unknown>) => scopeSelectionInput.parse(v)

describe(`clampScopeSelection`, () => {
  it(`rejects an empty selection before reading anything`, async () => {
    expect(() => assertScopeSelectionNonEmpty(input({}))).toThrow(/Select at least one/)
    await expect(clampScopeSelection(db, members, input({}))).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
  })

  it(`allTeams carries no ids`, async () => {
    expect(
      await clampScopeSelection(db, members, input({ allTeams: true, teamIds: [T1] }))
    ).toEqual({ allTeams: true, teamIds: [], boardIds: [] })
  })

  it(`clamps teams and boards to membership`, async () => {
    expect(
      await clampScopeSelection(
        db,
        members,
        input({
          teamIds: [T2, `55555555-5555-4555-8555-555555555555`],
          boardIds: [B1, B_OTHER],
        })
      )
    ).toEqual({ allTeams: false, teamIds: [T2], boardIds: [B1] })
  })

  it(`rejects a selection that reaches nothing accessible`, async () => {
    await expect(
      clampScopeSelection(db, members, input({ boardIds: [B_OTHER] }))
    ).rejects.toMatchObject({ code: `BAD_REQUEST`, message: /None of the selected/ })
  })
})
