import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/plan-card.json" with { type: "json" }
import { answerIssue, answerText, optionRows, pictureFor, resolveUpload, type AskCardLike, type CardAnswerInput } from "./plan-card"

describe(`plan-card.json`, () => {
  test.each(fixture.answerTextCases)(`answerText: $name`, (c) => {
    expect(answerText(c.card as AskCardLike)).toBe(c.expected)
  })

  test.each(fixture.answerIssueCases)(`answerIssue: $name`, (c) => {
    expect(answerIssue(c.card as AskCardLike, c.input as CardAnswerInput)).toBe(c.expected as never)
  })

  test.each(fixture.resolveUploadCases)(`resolveUpload: $name`, (c) => {
    expect(resolveUpload(c.card as AskCardLike, c.upload)).toEqual(c.expected)
  })

  test.each(fixture.pictureForCases)(`pictureFor: $name`, (c) => {
    expect(pictureFor(c.pictures as never, c.label)).toBe(c.expected)
  })

  test.each(fixture.optionRowsCases)(`optionRows: $name`, (c) => {
    expect(optionRows(c.options)).toBe(c.expected)
  })

  test(`the copy names every answer issue`, () => {
    expect(Object.keys(fixture.copy.answerIssues).sort()).toEqual([`decision`, `missing`, `multi`, `unknown`])
  })

  test(`the copy names the specimen's three cards and the card's states`, () => {
    expect(Object.keys(fixture.copy.specimen).sort()).toEqual([`answered`, `approved`, `pending`])
    expect(fixture.copy.feedbackLine).toContain(`{feedback}`)
    expect(fixture.copy.answered).not.toBe(fixture.copy.answering)
  })

  test(`the one look ×4: every tile and card number is positive`, () => {
    for (const [key, value] of [...Object.entries(fixture.tiles), ...Object.entries(fixture.card)]) {
      expect(typeof value, key).toBe(`number`)
      expect(value, key).toBeGreaterThan(0)
    }
    expect(fixture.tiles.inertOpacity).toBe(fixture.card.inertOpacity)
    expect(fixture.tiles.pictureRadius).toBeLessThan(fixture.tiles.radius)
  })
})
