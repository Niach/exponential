import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/composer-draft.json" with { type: "json" }
import {
  COMPOSER_DRAFT_DEBOUNCE_MS,
  composerDraftField,
  composerDraftKey,
  composerDraftWrite,
  parseComposerDraftKey,
  type ComposerDraftFieldInput,
  type ComposerDraftTarget,
} from "./composer-draft"

describe(`composer drafts`, () => {
  test(`the debounce is the fixture's`, () => {
    expect(COMPOSER_DRAFT_DEBOUNCE_MS).toBe(fixture.debounceMs)
  })

  for (const entry of fixture.keys) {
    test(`key: ${entry.name}`, () => {
      const target = entry.target as ComposerDraftTarget
      expect(composerDraftKey(target)).toBe(entry.key)
      expect(parseComposerDraftKey(entry.key)).toEqual(target)
    })
  }

  for (const key of fixture.unparsable) {
    test(`unparsable: ${JSON.stringify(key)}`, () => {
      expect(parseComposerDraftKey(key)).toBeNull()
    })
  }

  for (const entry of fixture.writes) {
    test(`write: ${entry.name}`, () => {
      expect(composerDraftWrite(entry.text)).toEqual(entry.expected as never)
    })
  }

  for (const entry of fixture.fields) {
    test(`field: ${entry.name}`, () => {
      expect(composerDraftField(entry.input as ComposerDraftFieldInput)).toBe(entry.expected)
    })
  }
})
