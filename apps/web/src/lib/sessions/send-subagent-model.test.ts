import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { sessionsSendInputSchema } from "@exp/db-schema/domain"
import { assertRunnerVocabulary } from "./send"

// EXP-1583: a new session's subagent model rides `sessions.send` (claude
// only, one of the contract coding models; composer-menu.json `subagents`).
describe(`sessions.send subagentModel`, () => {
  it(`parses on a new session`, () => {
    const parsed = sessionsSendInputSchema.safeParse({
      teamId: `00000000-0000-4000-8000-000000000000`,
      deviceId: `dev-1`,
      cli: `claude`,
      subagentModel: `sonnet`,
      text: `go`,
    })
    expect(parsed.success).toBe(true)
  })

  it(`takes a claude model on claude`, () => {
    expect(() => assertRunnerVocabulary({ cli: `claude`, subagentModel: `opus` })).not.toThrow()
  })

  it(`refuses an unknown model and any agent but claude`, () => {
    expect(() => assertRunnerVocabulary({ cli: `claude`, subagentModel: `gpt-5` })).toThrow(/subagent model/)
    expect(() => assertRunnerVocabulary({ cli: `codex`, subagentModel: `opus` })).toThrow(/subagent model/)
  })
})
