import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/blocked-start.json"
import {
  BLOCKED_BATCH_BODY,
  BLOCKED_BATCH_TITLE,
  BLOCKED_START_BODY_PREFIX,
  BLOCKED_START_BODY_SUFFIX,
  BLOCKED_START_BODY_SUFFIX_STACKABLE,
  BLOCKED_START_TITLE,
  STACK_DISABLED_REASONS,
  stackDisabledNote,
  STACKED_PR_LABEL,
  stackedStartPrompt,
  stackTarget,
  START_ANYWAY_LABEL,
  type StackBlocker,
  type StackDisabledReason,
} from "./blocked-start"

// SLOP-3: replayed off the ONE contract fixture ×4 (desktop
// `blocked_start.rs`, iOS `BlockedStartTests`, Android `BlockedStartTest`).

describe(`blocked start (contract fixture)`, () => {
  it(`locks the words`, () => {
    expect(BLOCKED_START_TITLE).toBe(fixture.copy.title)
    expect(BLOCKED_BATCH_TITLE).toBe(fixture.copy.batchTitle)
    expect(BLOCKED_BATCH_BODY).toBe(fixture.copy.batchBody)
    expect(BLOCKED_START_BODY_PREFIX).toBe(fixture.copy.bodyPrefix)
    expect(BLOCKED_START_BODY_SUFFIX).toBe(fixture.copy.bodySuffix)
    expect(BLOCKED_START_BODY_SUFFIX_STACKABLE).toBe(
      fixture.copy.bodySuffixStackable
    )
    expect(START_ANYWAY_LABEL).toBe(fixture.copy.startAnyway)
    expect(STACKED_PR_LABEL).toBe(fixture.copy.stackedPr)
  })

  it(`checks the reasons in the fixture's order`, () => {
    expect(STACK_DISABLED_REASONS).toEqual(fixture.reasons)
  })

  it(`locks every note to its template`, () => {
    for (const reason of fixture.reasons as StackDisabledReason[]) {
      const template = fixture.notes[reason]
      expect(stackDisabledNote(reason, `XYZ-9`)).toBe(
        template.replace(`{ident}`, `XYZ-9`)
      )
    }
  })

  for (const entry of fixture.targetCases) {
    it(entry.name, () => {
      const blockers = entry.blockers as StackBlocker[]
      const result = stackTarget({
        pickedCount: entry.pickedCount,
        subjectRepositoryId: entry.subjectRepositoryId,
        blockers,
      })
      expect(result.target?.identifier ?? null).toBe(entry.target)
      expect(result.reason).toBe(entry.reason)
      const note = result.reason
        ? stackDisabledNote(result.reason, blockers[0]?.identifier ?? ``)
        : null
      expect(note).toBe(entry.note)
    })
  }

  for (const entry of fixture.promptCases) {
    it(entry.name, () => {
      expect(stackedStartPrompt(entry.identifier, entry.branch, entry.text)).toBe(
        entry.prompt
      )
    })
  }

  it(`builds the prompt off the fixture's template`, () => {
    expect(stackedStartPrompt(`APP-1`, `exp/APP-1`, ``)).toBe(
      fixture.promptTemplate
        .replaceAll(`{ident}`, `APP-1`)
        .replaceAll(`{branch}`, `exp/APP-1`)
    )
  })
})
