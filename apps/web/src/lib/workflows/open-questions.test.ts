import { describe, expect, it } from "vitest"
import { workflowOpenQuestions } from "./open-questions"

// EXP-1082 §4 — declared; EXP-1065 implements the selector and un-skips.
describe(`workflowOpenQuestions`, () => {
  it.skip(`lists the open question of each live run of the workflow`, () => {
    const askedAt = `2026-09-25T10:00:00Z`
    expect(
      workflowOpenQuestions(
        [
          {
            id: `run-1`,
            workflowId: `wf-1`,
            workflowNodeId: `node-1`,
            pendingQuestion: { question: `Which schema?`, askedAt },
            status: `running`,
          },
          // Another workflow's run, an ended run and a run with no question
          // contribute nothing.
          {
            id: `run-2`,
            workflowId: `wf-2`,
            workflowNodeId: `node-9`,
            pendingQuestion: { question: `Elsewhere?`, askedAt },
            status: `running`,
          },
          {
            id: `run-3`,
            workflowId: `wf-1`,
            workflowNodeId: `node-2`,
            pendingQuestion: { question: `Too late?`, askedAt },
            status: `ended`,
          },
          {
            id: `run-4`,
            workflowId: `wf-1`,
            workflowNodeId: `node-3`,
            pendingQuestion: null,
            status: `running`,
          },
        ],
        `wf-1`
      )
    ).toEqual([{ nodeId: `node-1`, sessionId: `run-1`, question: `Which schema?`, askedAt }])
  })
})
