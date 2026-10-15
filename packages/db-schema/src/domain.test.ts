import { describe, expect, test } from "bun:test"

import {
  messageCardSchema,
  questionCardOptionDescriptionsSchema,
  questionCardOptionSchema,
  questionCardSchema,
} from "./domain"

// EXP-1322 sidecar wire contract. COMPAT: old iOS 0.14.59 / Android 0.14.61 /
// desktop 0.14.69 only read string options, so `options` stays `string[]` and
// the descriptions ride the parallel `optionDescriptions`.
describe(`question card options (EXP-1322)`, () => {
  test(`an option is a label string or, tolerated on read, a {label, description} object`, () => {
    expect(questionCardOptionSchema.safeParse(`Yes`).success).toBe(true)
    expect(questionCardOptionSchema.safeParse({ label: `Yes`, description: `Ship it` }).success).toBe(true)
    expect(questionCardOptionSchema.safeParse({ label: `Yes` }).success).toBe(true)
    expect(questionCardOptionSchema.safeParse({ label: `Yes`, description: null }).success).toBe(true)
    expect(questionCardOptionSchema.safeParse({ description: `x` }).success).toBe(false)
    expect(questionCardOptionSchema.safeParse(3).success).toBe(false)
  })

  test(`the sidecar is a list of nullable strings`, () => {
    expect(questionCardOptionDescriptionsSchema.safeParse([`Ship it`, null, ``]).success).toBe(true)
    expect(questionCardOptionDescriptionsSchema.safeParse([]).success).toBe(true)
    expect(questionCardOptionDescriptionsSchema.safeParse([1]).success).toBe(false)
    expect(questionCardOptionDescriptionsSchema.safeParse(`Ship it`).success).toBe(false)
    expect(questionCardOptionDescriptionsSchema.safeParse([{ label: `x` }]).success).toBe(false)
  })

  test(`a question card carries string options and an optional parallel sidecar`, () => {
    const sidecar = {
      kind: `question`,
      question: `Ship?`,
      options: [`Now`, `Later`],
      optionDescriptions: [`Behind a flag`, null],
      questionId: `q-1`,
    }
    const parsed = questionCardSchema.safeParse(sidecar)
    expect(parsed.success).toBe(true)
    expect(parsed.data).toEqual(sidecar)
    expect(questionCardSchema.safeParse({ kind: `question`, question: `Ship?`, options: [`Now`] }).success).toBe(true)
    expect(questionCardSchema.safeParse({ kind: `question`, question: `Ship?` }).success).toBe(true)
    // Object options still read (cards posted before the sidecar).
    expect(
      questionCardSchema.safeParse({ kind: `question`, question: `Ship?`, options: [{ label: `Now`, description: `x` }, `Later`] })
        .success,
    ).toBe(true)
    // The sidecar must be parallel to the options.
    expect(
      questionCardSchema.safeParse({ kind: `question`, question: `Ship?`, options: [`Now`], optionDescriptions: [`a`, `b`] }).success,
    ).toBe(false)
    expect(questionCardSchema.safeParse({ kind: `plan`, plan: `1.` }).success).toBe(false)
  })

  test(`the tolerant read schema passes the sidecar through`, () => {
    const card = { kind: `question`, question: `Ship?`, options: [`Now`], optionDescriptions: [`x`], questionId: `q` }
    expect(messageCardSchema.parse(card)).toEqual(card)
    expect(messageCardSchema.safeParse({ kind: `someday` }).success).toBe(true)
  })
})
