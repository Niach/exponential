import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/home-widgets.json" with { type: "json" }
import { boardWidget, myIssuesWidget, needsYouWidget } from "./home-widgets"

describe(`boardWidget`, () => {
  for (const entry of fixture.boardCases) test(entry.name, () => expect(boardWidget(entry.input)).toEqual(entry.expected))
})

describe(`myIssuesWidget`, () => {
  for (const entry of fixture.myIssuesCases) test(entry.name, () => expect(myIssuesWidget(entry.input)).toEqual(entry.expected))
})

describe(`needsYouWidget`, () => {
  for (const entry of fixture.needsYouCases) test(entry.name, () => expect(needsYouWidget(entry.input)).toEqual(entry.expected))
})
