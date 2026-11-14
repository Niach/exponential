import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/composer-attachments.json" with { type: "json" }
import {
  dropAttachment,
  insertImageMarker,
  renumberImageMarkers,
  stageAttachments,
  stageToasts,
  type DropInput,
  type StageInput,
} from "./composer-attachments"

describe(`composer-attachments.json`, () => {
  test(`every list has cases`, () => {
    for (const list of [fixture.insert, fixture.renumber, fixture.stage, fixture.drop]) {
      expect(list.length).toBeGreaterThan(0)
    }
  })
  for (const entry of fixture.insert) {
    test(`insert: ${entry.name}`, () => {
      expect(insertImageMarker(entry.input.text, entry.input.caret, entry.input.index)).toEqual(entry.expected)
    })
  }
  for (const entry of fixture.renumber) {
    test(`renumber: ${entry.name}`, () => {
      expect(renumberImageMarkers(entry.input.text, entry.input.removed)).toBe(entry.expected)
    })
  }
  for (const entry of fixture.stage) {
    test(`stage: ${entry.name}`, () => {
      const result = stageAttachments(entry.input as StageInput)
      expect({ ...result, toasts: stageToasts(result) }).toEqual(entry.expected)
    })
  }
  for (const entry of fixture.drop) {
    test(`drop: ${entry.name}`, () => {
      expect(dropAttachment(entry.input as DropInput)).toEqual(entry.expected)
    })
  }
})
