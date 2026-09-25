import { describe, expect, it } from "vitest"
import { workflowOpenQuestions } from "./open-questions"

// EXP-1082 §4 / EXP-1065 — the selector, mirrored ×4.
describe(`workflowOpenQuestions`, () => {
  it(`lists the open question of each live run of the workflow`, () => {
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

  it(`leaves out a planner run (no node) and a blank question, and orders by askedAt`, () => {
    expect(
      workflowOpenQuestions(
        [
          {
            id: `run-b`,
            workflowId: `wf-1`,
            workflowNodeId: `node-b`,
            pendingQuestion: { question: `Later?`, askedAt: `2026-09-25T11:00:00Z` },
            status: `in_review`,
          },
          {
            id: `run-a`,
            workflowId: `wf-1`,
            workflowNodeId: `node-a`,
            pendingQuestion: { question: `Earlier?`, askedAt: `2026-09-25T10:00:00Z` },
            status: `running`,
          },
          {
            id: `plan`,
            workflowId: `wf-1`,
            workflowNodeId: null,
            pendingQuestion: { question: `Runner device?`, askedAt: `2026-09-25T09:00:00Z` },
            status: `running`,
          },
          {
            id: `blank`,
            workflowId: `wf-1`,
            workflowNodeId: `node-c`,
            pendingQuestion: { question: `   `, askedAt: `2026-09-25T09:30:00Z` },
            status: `running`,
          },
        ],
        `wf-1`
      ).map((q) => q.sessionId)
    ).toEqual([`run-a`, `run-b`])
  })
})
