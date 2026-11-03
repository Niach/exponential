// eApps R6 (VAPP-222): fixtures/record-transitions.json replayed through the TS reference.

import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/record-transitions.json" with { type: "json" }
import { kindTransitions, transitionRefusal, transitionsFrom, validateRecordTransitions, type RecordTransitions, type TransitionViewer } from "./record-transitions"
import type { RecordOwnership } from "./record-policy"

type Case = Record<string, unknown> & { name: string }
const machine = (c: Case): RecordTransitions | null => (`transitions` in c ? kindTransitions({ transitions: c.transitions }) : kindTransitions(fixture.kind))

describe(`record-transitions.json`, () => {
  for (const c of fixture.validateCases as Case[])
    test(`validate: ${c.name}`, () => expect(validateRecordTransitions(c.kind, `/records/${c.kindName}`, c.kindName as string)).toEqual(c.expected as never))
  for (const c of fixture.refusalCases as Case[])
    test(`refusal: ${c.name}`, () =>
      expect(transitionRefusal(machine(c), c.before as Record<string, unknown> | null, c.after as Record<string, unknown>, c.viewer as TransitionViewer, c.row as RecordOwnership | undefined)).toEqual(c.expected as never))
  for (const c of fixture.fromCases as Case[])
    test(`from: ${c.name}`, () => expect(transitionsFrom(machine(c), c.record as Record<string, unknown> | null, c.viewer as TransitionViewer | undefined, c.row as RecordOwnership | undefined)).toEqual(c.expected as never))
})
