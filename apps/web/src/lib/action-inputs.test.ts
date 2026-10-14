import { describe, expect, it } from "vitest"
import type { ActionInputDef } from "@exp/db-schema/domain"
import {
  buildInputsPayload,
  missingRequiredInputs,
  resolveActionInputs,
  type ActionInputLookups,
} from "@/lib/action-inputs"

const REPO_ID = `11111111-1111-4111-8111-111111111111`
const BOARD_ID = `22222222-2222-4222-8222-222222222222`
const PR_ID = `44444444-4444-4444-8444-444444444444`
const ISSUE_ID = `55555555-5555-4555-8555-555555555555`

const defs: ActionInputDef[] = [
  { key: `issue`, label: `Issue`, type: `issue`, required: true },
  { key: `repo`, label: `Repository`, type: `repo`, required: false },
  { key: `board`, label: `Board`, type: `board`, required: false },
  { key: `pr`, label: `Pull request`, type: `pr`, required: false },
]

const lookups: ActionInputLookups = {
  repo: async (id, teamId) =>
    id === REPO_ID && teamId === `ws-1` ? { fullName: `acme/api` } : null,
  board: async (id, teamId) =>
    id === BOARD_ID && teamId === `ws-1` ? { name: `Roadmap` } : null,
  pr: async (id, teamId) =>
    id === PR_ID && teamId === `ws-1` ? { number: 7, title: `Fix login` } : null,
  issue: async (id, teamId) =>
    id === ISSUE_ID && teamId === `ws-1` ? { identifier: `EXP-42`, title: `Login` } : null,
}

describe(`missingRequiredInputs`, () => {
  it(`reports unfilled required labels; whitespace counts as empty`, () => {
    expect(missingRequiredInputs(defs, {})).toEqual([`Issue`])
    expect(missingRequiredInputs(defs, { issue: `   ` })).toEqual([`Issue`])
    expect(missingRequiredInputs(defs, { issue: ISSUE_ID })).toEqual([])
    expect(missingRequiredInputs(null, {})).toEqual([])
  })
})

describe(`buildInputsPayload`, () => {
  it(`keeps filled keys, drops blanks and unknowns, undefined when empty`, () => {
    expect(buildInputsPayload(defs, { issue: ISSUE_ID, repo: ` `, bogus: `x` })).toEqual({
      issue: ISSUE_ID,
    })
    expect(buildInputsPayload(defs, {})).toBeUndefined()
    expect(buildInputsPayload(undefined, { a: `b` })).toBeUndefined()
  })
})

describe(`resolveActionInputs`, () => {
  it(`resolves every pick kind in definition order with its display`, async () => {
    const result = await resolveActionInputs(
      defs,
      { pr: PR_ID, board: BOARD_ID, issue: ISSUE_ID, repo: REPO_ID },
      `ws-1`,
      lookups
    )
    expect(result).toEqual({
      ok: true,
      inputs: [
        { key: `issue`, label: `Issue`, type: `issue`, value: ISSUE_ID, display: `EXP-42 Login` },
        { key: `repo`, label: `Repository`, type: `repo`, value: REPO_ID, display: `acme/api` },
        { key: `board`, label: `Board`, type: `board`, value: BOARD_ID, display: `Roadmap` },
        { key: `pr`, label: `Pull request`, type: `pr`, value: PR_ID, display: `#7 · Fix login` },
      ],
    })
  })

  it(`rejects unknown keys even when the schema is empty`, async () => {
    const result = await resolveActionInputs([], { extra: `x` }, `ws-1`, lookups)
    expect(result).toMatchObject({ ok: false, message: `Unknown input "extra"` })
  })

  it(`rejects a missing/blank required value and skips blank optionals`, async () => {
    expect(await resolveActionInputs(defs, { issue: `  ` }, `ws-1`, lookups)).toMatchObject({
      ok: false,
      message: `Missing required input "issue"`,
    })
    const result = await resolveActionInputs(defs, { issue: ISSUE_ID, repo: `` }, `ws-1`, lookups)
    expect(result.ok && result.inputs.map((i) => i.key)).toEqual([`issue`])
  })

  it(`refuses the retired kinds at the schema`, async () => {
    const { actionInputDefSchema } = await import(`@exp/db-schema/domain`)
    for (const type of [`text`, `textarea`, `icon`]) {
      expect(actionInputDefSchema.safeParse({ key: `topic`, label: `Topic`, type }).success).toBe(
        false
      )
    }
  })

  it(`rejects non-uuid and other-team values`, async () => {
    expect(
      await resolveActionInputs(defs, { issue: ISSUE_ID, repo: `not-a-uuid` }, `ws-1`, lookups)
    ).toMatchObject({ ok: false, message: `Input "repo" must be an id` })
    expect(await resolveActionInputs(defs, { issue: ISSUE_ID }, `ws-2`, lookups)).toMatchObject({
      ok: false,
      message: `Input "issue": pick an issue of the team`,
    })
    expect(
      await resolveActionInputs(defs, { issue: ISSUE_ID, pr: REPO_ID }, `ws-1`, lookups)
    ).toMatchObject({ ok: false, message: `Input "pr": pick an open pull request of the team` })
  })
})
