import type { MessageCard } from "@exp/db-schema/domain"

// EXP-1316: ONE answered-card shape for both write paths: the thread card
// (`sessions.answer`, a person) and the device's write-back after a session
// view answered over the relay (`sessions.resolveCard`). Pure.

export interface CardAnswerInput {
  answer?: string
  decision?: `approved` | `rejected`
  /** An approval that starts in a fresh context. */
  fresh?: boolean
  feedback?: string
}

export type CardAnswerResult =
  | { ok: true; card: MessageCard }
  | { ok: false; code: `BAD_REQUEST`; message: string }

/** The card with its answer (question) or decision + feedback (plan) in it. */
export function answeredCard(card: MessageCard | null, input: CardAnswerInput): CardAnswerResult {
  if (!card || (card.kind !== `question` && card.kind !== `plan`)) {
    return { ok: false, code: `BAD_REQUEST`, message: `This message asks nothing` }
  }
  if (card.kind === `question`) {
    const answer = input.answer?.trim()
    if (!answer) return { ok: false, code: `BAD_REQUEST`, message: `answer is required` }
    return { ok: true, card: { ...card, answer } }
  }
  if (!input.decision) return { ok: false, code: `BAD_REQUEST`, message: `decision is required` }
  return {
    ok: true,
    card: {
      ...card,
      decision: input.decision,
      fresh: input.decision === `approved` && input.fresh === true,
      feedback: input.feedback?.trim() || null,
    },
  }
}
