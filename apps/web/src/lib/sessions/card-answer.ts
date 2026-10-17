import { PLAN_CARD_SURFACE_MAX_BYTES, type MessageCard } from "@exp/db-schema/domain"
import {
  answerIssue,
  joinAnswers,
  PLAN_CARD_COPY,
  type AskCardLike,
} from "@exp/domain-contract/plan-card"

// EXP-1316: ONE answered-card shape for both write paths: the thread card
// (`sessions.answer`, a person) and the device's write-back after a session
// view answered over the relay (`sessions.resolveCard`). Pure.
// EXP-1280: the rich card's answer (`answers`, `surfaceData`) is checked by
// the contract's `answerIssue` (fixtures/plan-card.json) and stamped IN it.

export interface CardAnswerInput {
  answer?: string
  /** A plan: question id → picked labels; a question: the picked labels. */
  answers?: Record<string, string[]> | string[]
  decision?: `approved` | `rejected`
  /** An approval that starts in a fresh context. */
  fresh?: boolean
  feedback?: string
  /** The inline surface's data model at answer time. */
  surfaceData?: Record<string, unknown>
}

export type CardAnswerResult =
  | { ok: true; card: MessageCard }
  | { ok: false; code: `BAD_REQUEST`; message: string }

const picks = (labels: readonly string[]) => labels.map((label) => label.trim()).filter(Boolean)

const bad = (message: string): CardAnswerResult => ({ ok: false, code: `BAD_REQUEST`, message })

/** The card with its answer (question) or decision + answers + feedback
 *  (plan) in it. */
export function answeredCard(card: MessageCard | null, input: CardAnswerInput): CardAnswerResult {
  if (!card || (card.kind !== `question` && card.kind !== `plan`)) {
    return bad(`This message asks nothing`)
  }
  const issue = answerIssue(card as AskCardLike, input)
  if (issue) return bad(PLAN_CARD_COPY.answerIssues[issue])
  const surfaceData = input.surfaceData ?? null
  if (surfaceData && new TextEncoder().encode(JSON.stringify(surfaceData)).byteLength > PLAN_CARD_SURFACE_MAX_BYTES) {
    return bad(`The form data is larger than ${PLAN_CARD_SURFACE_MAX_BYTES} bytes`)
  }
  const withSurface = surfaceData ? { surfaceData } : {}

  if (card.kind === `question`) {
    const labels = Array.isArray(input.answers) ? picks(input.answers) : []
    // Free text (or a legacy single answer): `answer` as given.
    const answers = labels.length > 0 ? labels : [input.answer!.trim()]
    return {
      ok: true,
      card: { ...card, answer: labels.length > 0 ? joinAnswers(labels) : answers[0]!, answers, ...withSurface },
    }
  }

  let answers: Record<string, string[]> | null = null
  if (input.answers && !Array.isArray(input.answers)) {
    answers = {}
    for (const [id, labels] of Object.entries(input.answers)) {
      const picked = picks(labels)
      if (picked.length > 0) answers[id] = picked
    }
  }
  return {
    ok: true,
    card: {
      ...card,
      decision: input.decision!,
      fresh: input.decision === `approved` && input.fresh === true,
      feedback: input.feedback?.trim() || null,
      ...(answers ? { answers } : {}),
      ...withSurface,
    },
  }
}
