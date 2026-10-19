import { describe, expect, it } from "vitest"
import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"
import type { MessageCard } from "@exp/db-schema/domain"
import { answeredCard } from "./card-answer"

// COMPAT (EXP-1280): a native shipped before the rich plan card's questions
// (iOS ≤ 0.14.62, Android ≤ 0.14.64, desktop/CLI ≤ 0.14.72) approves a plan
// with a decision and no `answers`. On a plan WITH questions that is a plain
// approval for them, the `missing` refusal for everyone else. Delete with
// `legacyPlanApproval` once those floors pass.

const plan: MessageCard = {
  kind: `plan`,
  plan: `1. Build it`,
  questions: [
    {
      id: `layout`,
      prompt: `Layout?`,
      multi: false,
      custom: false,
      options: [{ label: `Grid` }, { label: `List` }],
    },
  ],
}

describe(`answeredCard — legacy plan approval`, () => {
  it(`an old client's Approve without answers is a plain approval`, () => {
    expect(
      answeredCard(plan, { decision: `approved`, legacyPlanApproval: true })
    ).toEqual({
      ok: true,
      card: { ...plan, decision: `approved`, fresh: false, feedback: null },
    })
    expect(
      answeredCard(plan, {
        decision: `approved`,
        fresh: true,
        feedback: ` go `,
        legacyPlanApproval: true,
      })
    ).toEqual({
      ok: true,
      card: { ...plan, decision: `approved`, fresh: true, feedback: `go` },
    })
  })

  it(`a current client leaving a question blank is still refused`, () => {
    expect(answeredCard(plan, { decision: `approved` })).toMatchObject({
      ok: false,
      code: `BAD_REQUEST`,
      message: PLAN_CARD_COPY.answerIssues.missing,
    })
    expect(
      answeredCard(plan, { decision: `approved`, legacyPlanApproval: false })
    ).toMatchObject({
      ok: false,
      message: PLAN_CARD_COPY.answerIssues.missing,
    })
    // Answers sent (even empty) = a client that knows the questions.
    expect(
      answeredCard(plan, {
        decision: `approved`,
        answers: {},
        legacyPlanApproval: true,
      })
    ).toMatchObject({
      ok: false,
      message: PLAN_CARD_COPY.answerIssues.missing,
    })
  })

  it(`the shim never excuses any other issue`, () => {
    expect(answeredCard(plan, { legacyPlanApproval: true })).toMatchObject({
      ok: false,
      message: PLAN_CARD_COPY.answerIssues.decision,
    })
    expect(
      answeredCard(plan, {
        decision: `approved`,
        answers: { layout: [`Grid`, `List`] },
        legacyPlanApproval: true,
      })
    ).toMatchObject({ ok: false, message: PLAN_CARD_COPY.answerIssues.multi })
    expect(
      answeredCard(
        { kind: `question`, question: `Where?` },
        { answer: ``, legacyPlanApproval: true }
      )
    ).toMatchObject({ ok: false, message: PLAN_CARD_COPY.answerIssues.missing })
  })

  it(`a rejection never needed answers`, () => {
    expect(
      answeredCard(plan, { decision: `rejected`, legacyPlanApproval: true })
    ).toMatchObject({
      ok: true,
      card: { decision: `rejected` },
    })
  })
})
