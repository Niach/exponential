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
const PR_ISSUE_ID = `44444444-4444-4444-8444-444444444444`

// EXP-825: every input is a pick — the free-text kinds are gone (the start's
// `prompt` carries that). `icon` stands in as the lookup-free required pick.
const defs: ActionInputDef[] = [
  { key: `topic`, label: `Topic`, type: `icon`, required: true },
  { key: `notes`, label: `Notes`, type: `icon`, required: false },
  { key: `repo`, label: `Repository`, type: `repo`, required: false },
  { key: `board`, label: `Board`, type: `board`, required: false },
  { key: `pr`, label: `Pull request`, type: `pr`, required: false },
]

const lookups: ActionInputLookups = {
  repo: async (id, teamId) =>
    id === REPO_ID && teamId === `ws-1` ? { fullName: `acme/api` } : null,
  board: async (id, teamId) =>
    id === BOARD_ID && teamId === `ws-1` ? { name: `Roadmap` } : null,
  pr: async (issueId, teamId) =>
    issueId === PR_ISSUE_ID && teamId === `ws-1`
      ? { identifier: `EXP-42`, prNumber: 7 }
      : null,
}

describe(`missingRequiredInputs`, () => {
  it(`reports unfilled required labels; whitespace counts as empty`, () => {
    expect(missingRequiredInputs(defs, {})).toEqual([`Topic`])
    expect(missingRequiredInputs(defs, { topic: `   ` })).toEqual([`Topic`])
    expect(missingRequiredInputs(defs, { topic: `rocket` })).toEqual([])
    expect(missingRequiredInputs(null, {})).toEqual([])
  })
})

describe(`buildInputsPayload`, () => {
  it(`keeps filled keys, drops blanks and unknowns, undefined when empty`, () => {
    expect(
      buildInputsPayload(defs, { topic: `rocket`, notes: ` `, bogus: `x` })
    ).toEqual({ topic: `rocket` })
    expect(buildInputsPayload(defs, {})).toBeUndefined()
    expect(buildInputsPayload(undefined, { a: `b` })).toBeUndefined()
  })
})

describe(`resolveActionInputs`, () => {
  it(`resolves in definition order with display names`, async () => {
    const result = await resolveActionInputs(
      defs,
      { board: BOARD_ID, topic: `rocket`, repo: REPO_ID },
      `ws-1`,
      lookups
    )
    expect(result).toEqual({
      ok: true,
      inputs: [
        { key: `topic`, label: `Topic`, type: `icon`, value: `rocket`, display: `rocket` },
        {
          key: `repo`,
          label: `Repository`,
          type: `repo`,
          value: REPO_ID,
          display: `acme/api`,
        },
        {
          key: `board`,
          label: `Board`,
          type: `board`,
          value: BOARD_ID,
          display: `Roadmap`,
        },
      ],
    })
  })

  it(`rejects unknown keys even when the schema is empty`, async () => {
    const result = await resolveActionInputs([], { extra: `x` }, `ws-1`, lookups)
    expect(result).toMatchObject({ ok: false, message: `Unknown input "extra"` })
  })

  it(`rejects a missing/blank required value`, async () => {
    const result = await resolveActionInputs(defs, { topic: `  ` }, `ws-1`, lookups)
    expect(result).toMatchObject({
      ok: false,
      message: `Missing required input "topic"`,
    })
  })

  it(`skips blank optionals`, async () => {
    const result = await resolveActionInputs(
      defs,
      { topic: `rocket`, notes: `` },
      `ws-1`,
      lookups
    )
    expect(result.ok && result.inputs.map((i) => i.key)).toEqual([`topic`])
  })

  // EXP-825: a definition still carrying a retired free-text kind is not a
  // valid schema anymore — the domain zod refuses it at create/update.
  it(`refuses the retired text and textarea kinds at the schema`, async () => {
    const { actionInputDefSchema } = await import(`@exp/db-schema/domain`)
    for (const type of [`text`, `textarea`]) {
      expect(
        actionInputDefSchema.safeParse({ key: `topic`, label: `Topic`, type })
          .success
      ).toBe(false)
    }
  })

  it(`rejects non-uuid and unresolvable repo/board values`, async () => {
    let result = await resolveActionInputs(
      defs,
      { topic: `rocket`, repo: `not-a-uuid` },
      `ws-1`,
      lookups
    )
    expect(result).toMatchObject({ ok: false, message: `Input "repo" must be an id` })

    // Wrong team ⇒ lookup returns null ⇒ refused.
    result = await resolveActionInputs(
      defs,
      { topic: `rocket`, repo: REPO_ID },
      `ws-2`,
      lookups
    )
    expect(result).toMatchObject({ ok: false })

    result = await resolveActionInputs(
      defs,
      { topic: `rocket`, board: `33333333-3333-4333-8333-333333333333` },
      `ws-1`,
      lookups
    )
    expect(result).toMatchObject({ ok: false })
  })

  it(`resolves pr inputs to a "#N · IDENT" display (EXP-259)`, async () => {
    const result = await resolveActionInputs(
      defs,
      { topic: `rocket`, pr: PR_ISSUE_ID },
      `ws-1`,
      lookups
    )
    expect(result.ok && result.inputs.find((i) => i.key === `pr`)).toEqual({
      key: `pr`,
      label: `Pull request`,
      type: `pr`,
      value: PR_ISSUE_ID,
      display: `#7 · EXP-42`,
    })
  })

  it(`rejects an unresolvable pr value (wrong team / no open PR)`, async () => {
    const result = await resolveActionInputs(
      defs,
      { topic: `rocket`, pr: PR_ISSUE_ID },
      `ws-2`,
      lookups
    )
    expect(result).toMatchObject({
      ok: false,
      message: `Input "pr": pick an open pull request of the team`,
    })
  })

  // EXP-273: `icon` is the only picked value that is NOT an id — it validates
  // against the curated contract set instead of a team-scoped lookup, so it
  // must bypass the uuid gate the repo/board/pr branches share.
  const iconDefs: ActionInputDef[] = [
    { key: `glyph`, label: `Icon`, type: `icon`, required: false },
  ]

  it(`resolves a curated icon name without hitting a lookup`, async () => {
    const result = await resolveActionInputs(
      iconDefs,
      { glyph: `rocket` },
      `ws-1`,
      lookups
    )
    expect(result).toEqual({
      ok: true,
      inputs: [
        {
          key: `glyph`,
          label: `Icon`,
          type: `icon`,
          value: `rocket`,
          display: `rocket`,
        },
      ],
    })
  })

  it(`rejects an icon name outside the curated set`, async () => {
    const result = await resolveActionInputs(
      iconDefs,
      { glyph: `not-a-real-icon` },
      `ws-1`,
      lookups
    )
    expect(result).toMatchObject({
      ok: false,
      message: `Input "glyph": pick an icon from the curated set`,
    })
  })

  it(`rejects a uuid as an icon value`, async () => {
    // The uuid gate is skipped for icons, so the curated-set check is the only
    // thing standing between a stray id and the prompt.
    const result = await resolveActionInputs(
      iconDefs,
      { glyph: BOARD_ID },
      `ws-1`,
      lookups
    )
    expect(result).toMatchObject({ ok: false })
  })
})
