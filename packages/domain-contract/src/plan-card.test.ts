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
})
