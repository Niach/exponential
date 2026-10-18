import { describe, expect, it } from "vitest"
import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"
import type { MessageCard } from "@exp/db-schema/domain"
import { answeredCard, type CardAnswerInput } from "./card-answer"

// EXP-1316: `sessions.answer` and `sessions.resolveCard` stamp ONE shape.
describe(`answeredCard`, () => {
  it(`a question takes the trimmed answer, an option or free text`, () => {
    expect(
      answeredCard({ kind: `question`, question: `Where?`, options: [`A`, `B`] }, { answer: `  Only on staging ` })
    ).toEqual({
      ok: true,
      card: {
        kind: `question`,
        question: `Where?`,
        options: [`A`, `B`],
        answer: `Only on staging`,
        answers: [`Only on staging`],
      },
    })
  })

  it(`a question without an answer is refused`, () => {
    expect(answeredCard({ kind: `question`, question: `Where?` }, { answer: `  ` })).toMatchObject({
      ok: false,
      code: `BAD_REQUEST`,
      message: PLAN_CARD_COPY.answerIssues.missing,
    })
  })

  it(`a plan takes the decision and the trimmed feedback, blank = null`, () => {
    expect(answeredCard({ kind: `plan`, plan: `1. x` }, { decision: `rejected`, feedback: ` not yet ` })).toEqual({
      ok: true,
      card: { kind: `plan`, plan: `1. x`, decision: `rejected`, fresh: false, feedback: `not yet` },
    })
    expect(answeredCard({ kind: `plan`, plan: `1. x` }, { decision: `approved`, feedback: `` })).toEqual({
      ok: true,
      card: { kind: `plan`, plan: `1. x`, decision: `approved`, fresh: false, feedback: null },
    })
    expect(answeredCard({ kind: `plan`, plan: `1. x` }, {})).toMatchObject({
      ok: false,
      message: PLAN_CARD_COPY.answerIssues.decision,
    })
  })

  it(`any other card asks nothing`, () => {
    expect(answeredCard({ kind: `session`, sessionId: `s` }, { answer: `x` })).toMatchObject({
      ok: false,
      message: `This message asks nothing`,
    })
    expect(answeredCard(null, { answer: `x` })).toMatchObject({ ok: false })
  })

  it(`a fresh-context approval stamps fresh; a rejection never does`, () => {
    expect(answeredCard({ kind: `plan`, plan: `1. x` }, { decision: `approved`, fresh: true })).toEqual({
      ok: true,
      card: { kind: `plan`, plan: `1. x`, decision: `approved`, fresh: true, feedback: null },
    })
    expect(answeredCard({ kind: `plan`, plan: `1. x` }, { decision: `rejected`, fresh: true })).toEqual({
      ok: true,
      card: { kind: `plan`, plan: `1. x`, decision: `rejected`, fresh: false, feedback: null },
    })
  })
})

// EXP-1280: the rich card's answers + surface data, checked by the
// contract's `answerIssue` (fixtures/plan-card.json).
describe(`answeredCard — EXP-1280 rich cards`, () => {
  const plan: MessageCard = {
    kind: `plan`,
    plan: `1. Build it`,
    questions: [
      { id: `layout`, prompt: `Layout?`, multi: false, custom: false, options: [{ label: `Grid` }, { label: `List` }] },
      { id: `extras`, prompt: `Extras?`, multi: true, custom: true, options: [{ label: `Dark` }, { label: `RTL` }] },
    ],
    surface: { root: { id: `r`, component: `Text` }, data: null },
  }

  it(`an approval stamps the trimmed answers, the surface data and leaves empty lists out`, () => {
    expect(
      answeredCard(plan, {
        decision: `approved`,
        answers: { layout: [` Grid `], extras: [`Dark`, `Sepia`] },
        surfaceData: { name: `x` },
        feedback: ` ok `,
      })
    ).toEqual({
      ok: true,
      card: {
        ...plan,
        decision: `approved`,
        fresh: false,
        feedback: `ok`,
        answers: { layout: [`Grid`], extras: [`Dark`, `Sepia`] },
        surfaceData: { name: `x` },
      },
    })
    expect(
      answeredCard(plan, { decision: `rejected`, answers: { layout: [`List`], extras: [] } })
    ).toMatchObject({ ok: true, card: { decision: `rejected`, answers: { layout: [`List`] } } })
  })

  it.each<[keyof typeof PLAN_CARD_COPY.answerIssues, CardAnswerInput]>([
    [`missing`, { decision: `approved`, answers: { layout: [`Grid`] } }],
    [`multi`, { decision: `approved`, answers: { layout: [`Grid`, `List`], extras: [`Dark`] } }],
    [`unknown`, { decision: `approved`, answers: { layout: [`Cards`], extras: [`Dark`] } }],
    [`unknown`, { decision: `rejected`, answers: { nope: [`x`] } }],
    [`decision`, { answers: { layout: [`Grid`] } }],
  ])(`refuses %s with the contract copy`, (issue, input) => {
    expect(answeredCard(plan, input)).toEqual({
      ok: false,
      code: `BAD_REQUEST`,
      message: PLAN_CARD_COPY.answerIssues[issue],
    })
  })

  it(`a rejection never needs answers`, () => {
    expect(answeredCard(plan, { decision: `rejected` })).toMatchObject({ ok: true })
  })

  it(`a question takes picked labels (answer = joined) or free text; multi and custom are enforced`, () => {
    const question: MessageCard = {
      kind: `question`,
      question: `Which?`,
      options: [`A`, `B`],
      multi: true,
      custom: false,
    }
    expect(answeredCard(question, { answers: [`A`, ` B `], surfaceData: { k: 1 } })).toEqual({
      ok: true,
      card: { ...question, answer: `A, B`, answers: [`A`, `B`], surfaceData: { k: 1 } },
    })
    expect(answeredCard(question, { answer: `C` })).toMatchObject({
      ok: false,
      message: PLAN_CARD_COPY.answerIssues.unknown,
    })
    expect(answeredCard({ ...question, multi: false }, { answers: [`A`, `B`] })).toMatchObject({
      ok: false,
      message: PLAN_CARD_COPY.answerIssues.multi,
    })
    expect(answeredCard(question, { answers: [] })).toMatchObject({
      ok: false,
      message: PLAN_CARD_COPY.answerIssues.missing,
    })
  })

  it(`a legacy question (no multi/custom) still takes free text`, () => {
    expect(
      answeredCard({ kind: `question`, question: `Where?`, options: [`A`], questionId: `q1` } as MessageCard, {
        answer: `Somewhere else`,
      })
    ).toMatchObject({ ok: true, card: { answer: `Somewhere else`, answers: [`Somewhere else`], questionId: `q1` } })
  })

  it(`refuses surface data past the byte cap`, () => {
    expect(
      answeredCard(plan, { decision: `rejected`, surfaceData: { blob: `x`.repeat(70_000) } })
    ).toMatchObject({ ok: false, code: `BAD_REQUEST` })
  })
})
