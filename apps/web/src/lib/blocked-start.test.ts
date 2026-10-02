import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/blocked-start.json"
import {
  BLOCKED_BATCH_BODY,
  BLOCKED_BATCH_TITLE,
  BLOCKED_START_BODY_PREFIX,
  BLOCKED_START_BODY_SUFFIX,
  BLOCKED_START_BODY_SUFFIX_STACKABLE,
  BLOCKED_START_TITLE,
  STACK_BASE_TEMPLATE,
  STACK_DISABLED_NOTES,
  STACK_DISABLED_REASONS,
  STACK_LINE_TEMPLATE,
  STACK_MAX_RUN,
  STACK_PLAN_NOTE_TEMPLATE,
  STACK_TEXT_TEMPLATE,
  stackDisabledNote,
  STACKED_PR_LABEL,
  stackedStartPrompt,
  stackLine,
  stackPlan,
  stackPlanNote,
  START_ANYWAY_LABEL,
  type StackDisabledReason,
  type StackLineMember,
  type StackPlan,
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

  it(`locks the cap, the reasons' order and the templates`, () => {
    expect(STACK_MAX_RUN).toBe(fixture.maxRun)
    expect(STACK_DISABLED_REASONS).toEqual(fixture.reasons)
    expect(STACK_DISABLED_NOTES).toEqual(fixture.notes)
    expect(STACK_PLAN_NOTE_TEMPLATE).toBe(fixture.planNoteTemplate)
    expect(STACK_BASE_TEMPLATE).toBe(fixture.baseTemplate)
    expect(STACK_LINE_TEMPLATE).toBe(fixture.lineTemplate)
    expect(STACK_TEXT_TEMPLATE).toBe(fixture.textTemplate)
  })

  it(`locks every note to its template`, () => {
    for (const reason of fixture.reasons as StackDisabledReason[]) {
      const template = fixture.notes[reason]
      expect(stackDisabledNote(reason, `XYZ-9`)).toBe(
        template.replace(`{ident}`, `XYZ-9`)
      )
    }
  })

  describe(`stackLine`, () => {
    for (const entry of fixture.lineCases) {
      it(entry.name, () => {
        const result = stackLine(entry.subject, entry.relations, entry.issues)
        expect(result.line.map((issue) => issue.identifier)).toEqual(entry.line)
        expect(result.fork).toBe(entry.fork)
        expect(result.cycle).toBe(entry.cycle)
      })
    }
  })

  describe(`stackPlan`, () => {
    for (const entry of fixture.planCases) {
      it(entry.name, () => {
        const result = stackPlan({
          pickedCount: entry.pickedCount,
          subject: entry.subject,
          line: entry.line as StackLineMember[],
          fork: entry.fork,
          cycle: entry.cycle,
        })
        expect(result.plan).toEqual(entry.plan)
        expect(result.reason).toBe(entry.reason)
        const note = result.reason
          ? stackDisabledNote(result.reason, result.ident ?? ``)
          : null
        expect(note).toBe(entry.note)
        expect(result.plan ? stackPlanNote(result.plan.run) : null).toBe(
          entry.planNote
        )
      })
    }
  })

  describe(`stackedStartPrompt`, () => {
    for (const entry of fixture.promptCases) {
      it(entry.name, () => {
        const plan: StackPlan = { base: entry.base, run: entry.run }
        expect(stackedStartPrompt(plan, entry.text)).toBe(entry.prompt)
      })
    }
  })
})
