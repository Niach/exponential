import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/app-intents.json" with { type: "json" }
import { intentBoard, intentIssueTitle, matchIntentAgent } from "./app-intents"

describe(`intentIssueTitle`, () => {
  for (const entry of fixture.titleCases) {
    test(entry.name, () => expect(intentIssueTitle(entry.input)).toBe(entry.expected))
  }
})

describe(`intentBoard`, () => {
  for (const entry of fixture.boardCases) {
    test(entry.name, () => expect(intentBoard(entry.input)).toBe(entry.expected))
  }
})

describe(`matchIntentAgent`, () => {
  for (const entry of fixture.agentCases) {
    test(entry.name, () => expect(matchIntentAgent(entry.input)).toBe(entry.expected))
  }
})
