import { describe, expect, it } from "vitest"
import { answeredCard } from "./card-answer"

// EXP-1316: `sessions.answer` and `sessions.resolveCard` stamp ONE shape.
describe(`answeredCard`, () => {
  it(`a question takes the trimmed answer, an option or free text`, () => {
    expect(
      answeredCard({ kind: `question`, question: `Where?`, options: [`A`, `B`] }, { answer: `  Only on staging ` })
    ).toEqual({
      ok: true,
      card: { kind: `question`, question: `Where?`, options: [`A`, `B`], answer: `Only on staging` },
    })
  })

  it(`a question without an answer is refused`, () => {
    expect(answeredCard({ kind: `question`, question: `Where?` }, { answer: `  ` })).toMatchObject({
      ok: false,
      message: `answer is required`,
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
    expect(answeredCard({ kind: `plan`, plan: `1. x` }, {})).toMatchObject({ ok: false })
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
