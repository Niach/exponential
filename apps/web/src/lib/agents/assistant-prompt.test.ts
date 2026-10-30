// @vitest-environment node
// #EXP-1439 (agent-settings.json `assistant.prompt`): the Assistant's prompt
// is built in and versioned; the migration that dropped the instructions
// field posts the member's old text under `copy.legacyInstructions`.
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"
import { ASSISTANT, ASSISTANT_COPY } from "@exp/domain-contract/agent-settings"

const sql = readFileSync(new URL(`../../db/out/0216_exp1439_assistant_builtin_prompt.sql`, import.meta.url), `utf8`)

describe(`the Assistant's built-in prompt`, () => {
  it(`is versioned text and the page has no instructions section`, () => {
    expect(ASSISTANT.prompt.version).toBeGreaterThanOrEqual(2)
    expect(ASSISTANT.prompt.text).toContain(`status card`)
    expect(ASSISTANT.sections).not.toContain(`instructions`)
  })

  it(`the migration posts the old text under the contract header`, () => {
    expect(sql).toContain(ASSISTANT_COPY.legacyInstructions.replaceAll(`'`, `''`))
  })
})
