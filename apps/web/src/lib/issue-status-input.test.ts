import { describe, expect, it } from "vitest"
import {
  RETIRED_ISSUE_STATUS_ALIASES,
  issueStatusInputSchema,
  issueStatusSchema,
  issueStatusValues,
  normalizeIssueStatus,
} from "@/lib/domain"

// EXP-685 retired the builtin `todo` status. The wire vocabulary shrank to
// six values, but old clients and agent scripts keep sending the retired
// token — every tRPC mutation input therefore parses through
// `issueStatusInputSchema`, which accepts it and normalizes it to `backlog`
// BEFORE any write. The strict `issueStatusSchema` (row shapes, MCP enums,
// the Electric column) still rejects it, so nothing ever persists it.
describe(`retired issue status tokens`, () => {
  it(`drops todo from the live enum`, () => {
    expect([...issueStatusValues]).not.toContain(`todo`)
    expect(issueStatusSchema.safeParse(`todo`).success).toBe(false)
    expect(issueStatusSchema.safeParse(`backlog`).success).toBe(true)
  })

  it(`accepts the retired token on mutation inputs and normalizes it`, () => {
    expect(issueStatusInputSchema.parse(`todo`)).toBe(`backlog`)
    expect(normalizeIssueStatus(`todo`)).toBe(`backlog`)
    expect(RETIRED_ISSUE_STATUS_ALIASES.todo).toBe(`backlog`)
  })

  it(`still passes live values through untouched, and rejects nonsense`, () => {
    for (const value of issueStatusValues) {
      expect(issueStatusInputSchema.parse(value)).toBe(value)
    }
    expect(issueStatusInputSchema.safeParse(`nonsense`).success).toBe(false)
  })
})
