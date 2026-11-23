import { describe, expect, test } from "bun:test"

import contract from "../contract.json" with { type: "json" }
import fixture from "../fixtures/transcript-grouping.json" with { type: "json" }
import { TRANSCRIPT_LAYOUT, checkInLabel, crossSessionInput, feedInput, feedRows, memberInput, isBlankText, markersShown, traceNotice, transcriptLayout } from "./transcript-grouping"

describe(`transcript grouping (EXP-1736)`, () => {
  test(`layout comes from the fixture`, () => {
    expect(TRANSCRIPT_LAYOUT).toEqual(fixture.layout)
  })
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(transcriptLayout(entry.input as never)).toEqual(entry.expected as never)
    })
  }
  for (const entry of fixture.inputCases) {
    test(`input: ${entry.name}`, () => {
      expect(crossSessionInput(entry.text)).toEqual(entry.expected as never)
    })
  }
  for (const entry of fixture.memberInputCases) {
    test(`member input: ${entry.name}`, () => {
      expect(memberInput(entry.input.name, entry.input.userId, entry.input.body)).toBe(entry.expected)
      expect(feedInput(entry.expected)).toEqual({ kind: `member`, ...entry.input })
    })
  }
  for (const entry of fixture.feedInputCases) {
    test(`feed input: ${entry.name}`, () => {
      expect(feedInput(entry.text)).toEqual(entry.expected as never)
    })
  }
  for (const entry of fixture.markerCases) {
    test(`markers: ${entry.name}`, () => {
      expect(markersShown(entry.kinds as never)).toEqual(entry.expected)
    })
  }
  for (const entry of fixture.checkInCases) {
    test(`check-ins: ${entry.name}`, () => {
      expect(feedRows(entry.kinds as never)).toEqual(entry.expected)
    })
  }
  for (const entry of fixture.checkInLabelCases) {
    test(`check-in label: ${entry.name}`, () => expect(checkInLabel(entry.count, entry.time)).toBe(entry.expected))
  }
  for (const entry of fixture.blankCases) {
    test(`blank: ${entry.name}`, () => expect(isBlankText(entry.text)).toBe(entry.expected))
  }
  for (const entry of fixture.noticeCases) {
    test(`notice: ${entry.name}`, () => expect(traceNotice(entry.toolName)).toBe(entry.expected))
  }
  test(`a notice reads its contract row's done caption`, () => {
    expect(contract.expToolDisplay.prefix).toBe(`exponential_`)
    for (const [row, label] of Object.entries(fixture.notices)) {
      expect(contract.expToolDisplay.tools.find((tool) => tool.name === row)?.done).toBe(label)
    }
  })
})
