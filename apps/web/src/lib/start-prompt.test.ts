import { describe, expect, it } from "vitest"
import { MAX_START_PROMPT } from "@exp/db-schema/domain"
import {
  normalizeStartPrompt,
  resolveStartPrompt,
  type StartPromptAttachment,
} from "@/lib/start-prompt"

// EXP-825: the start's free text — trimmed away when blank, capped, and its
// image embeds checked against the caller's own pending uploads.

const TEAM = `11111111-1111-4111-8111-111111111111`
const IMG_A = `aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa`
const IMG_B = `bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb`

function lookups(rows: StartPromptAttachment[]) {
  const seen: string[][] = []
  return {
    seen,
    attachments: async (ids: string[]) => {
      seen.push(ids)
      return rows.filter((row) => ids.includes(row.id))
    },
  }
}

const mine = (id: string, over: Partial<StartPromptAttachment> = {}) => ({
  id,
  teamId: TEAM,
  uploaderId: `me`,
  sessionUserId: null,
  ...over,
})

describe(`normalizeStartPrompt`, () => {
  it(`treats blank text as absent and keeps everything else verbatim`, () => {
    expect(normalizeStartPrompt(undefined)).toBeUndefined()
    expect(normalizeStartPrompt(``)).toBeUndefined()
    expect(normalizeStartPrompt(`  \n\t`)).toBeUndefined()
    expect(normalizeStartPrompt(` keep me \n`)).toBe(` keep me \n`)
  })
})

describe(`resolveStartPrompt`, () => {
  it(`forwards plain text byte-identical without any lookup`, async () => {
    const l = lookups([])
    await expect(
      resolveStartPrompt(`Use the v2 endpoint.\n`, TEAM, `me`, l)
    ).resolves.toEqual({ ok: true, prompt: `Use the v2 endpoint.\n` })
    expect(l.seen).toEqual([])
  })

  it(`drops a blank prompt`, async () => {
    await expect(
      resolveStartPrompt(`   `, TEAM, `me`, lookups([]))
    ).resolves.toEqual({ ok: true, prompt: undefined })
  })

  it(`caps the whole string`, async () => {
    const result = await resolveStartPrompt(
      `x`.repeat(MAX_START_PROMPT + 1),
      TEAM,
      `me`,
      lookups([])
    )
    expect(result).toMatchObject({ ok: false, code: `BAD_REQUEST` })
  })

  it(`accepts the caller's own pending uploads for the team`, async () => {
    const prompt = `crop [Image #1]\n\n![image](/api/attachments/${IMG_A})\n![image](/api/attachments/${IMG_B})`
    const l = lookups([mine(IMG_A), mine(IMG_B)])
    await expect(resolveStartPrompt(prompt, TEAM, `me`, l)).resolves.toEqual({
      ok: true,
      prompt,
    })
    expect(l.seen).toEqual([[IMG_A, IMG_B]])
  })

  it(`accepts an image already bound to the caller's own session`, async () => {
    const prompt = `![image](/api/attachments/${IMG_A})`
    const l = lookups([mine(IMG_A, { sessionUserId: `me` })])
    await expect(resolveStartPrompt(prompt, TEAM, `me`, l)).resolves.toEqual({
      ok: true,
      prompt,
    })
  })

  it.each([
    [`unknown`, []],
    [`another team`, [mine(IMG_A, { teamId: `other` })]],
    [`another uploader`, [mine(IMG_A, { uploaderId: `them` })]],
    [`bound to someone else's session`, [mine(IMG_A, { sessionUserId: `them` })]],
  ])(`refuses an embed that is %s`, async (_label, rows) => {
    const result = await resolveStartPrompt(
      `![image](/api/attachments/${IMG_A})`,
      TEAM,
      `me`,
      lookups(rows)
    )
    expect(result).toMatchObject({ ok: false, code: `PRECONDITION_FAILED` })
  })

  it(`refuses more than four images and duplicates`, async () => {
    const five = [1, 2, 3, 4, 5]
      .map((n) => `![image](/api/attachments/${IMG_A.slice(0, -1)}${n})`)
      .join(`\n`)
    expect(
      await resolveStartPrompt(five, TEAM, `me`, lookups([]))
    ).toMatchObject({ ok: false, code: `BAD_REQUEST` })
    const twice = `![image](/api/attachments/${IMG_A})\n![image](/api/attachments/${IMG_A})`
    expect(
      await resolveStartPrompt(twice, TEAM, `me`, lookups([mine(IMG_A)]))
    ).toMatchObject({ ok: false, code: `BAD_REQUEST` })
  })
})
