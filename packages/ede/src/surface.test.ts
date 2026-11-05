import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/surface.json" with { type: "json" }
import { documentIssues, normalizeSurface } from "./surface"

describe(`normalizeSurface`, () => {
  for (const entry of fixture.headerCases) {
    test(entry.name, () => expect(normalizeSurface(entry.input)).toEqual(entry.expected as ReturnType<typeof normalizeSurface>))
  }
})

describe(`documentIssues`, () => {
  for (const entry of fixture.documentCases) {
    test(entry.name, () => expect(documentIssues(entry.document)).toEqual(entry.expected.issues))
  }
})
