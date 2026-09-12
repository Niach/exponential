// EXP-850 §7: the fixture is the contract. Every mirror (desktop
// `steer::workflow::tests`, web, iOS, Android) replays this same file, so a
// rule that moves here moves everywhere or the suites go red.

import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/workflow-caption.json" with { type: "json" }
import { workflowCaption, type WorkflowCaptionInput } from "./workflow-caption"

interface Case {
  name: string
  workflow: WorkflowCaptionInput
  expected: string
}

describe(`workflowCaption`, () => {
  test(`the fixture covers every status and both phase paths`, () => {
    const cases = fixture as unknown as Case[]
    expect(cases.length).toBeGreaterThanOrEqual(8)
    const statuses = new Set(cases.map((entry) => entry.workflow.status))
    expect([...statuses].sort()).toEqual([
      `completed`,
      `failed`,
      `running`,
      `stopped`,
    ])
  })

  for (const entry of fixture as unknown as Case[]) {
    test(entry.name, () => {
      expect(workflowCaption(entry.workflow)).toBe(entry.expected)
    })
  }
})
