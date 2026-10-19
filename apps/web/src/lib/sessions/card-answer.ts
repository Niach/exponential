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
  // COMPAT (EXP-1280): delete once CLIENT_MIN_VERSION_IOS > 0.14.62 AND
  // CLIENT_MIN_VERSION_ANDROID > 0.14.64 AND CLIENT_MIN_VERSION_DESKTOP/_CLI
  // > 0.14.72 (`PLAN_ANSWERS_LEGACY_CLIENTS`, trpc/sessions.ts). Those
  // clients' Approve knows no questions: it sends a decision and no
  // `answers`. True = the caller is one of them, so an approval WITHOUT
  // `answers` on a plan with questions is a plain approval, not `missing`.
  legacyPlanApproval?: boolean
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
  // COMPAT (EXP-1280): an old native's Approve on a rich plan (see
  // `legacyPlanApproval`): the questions stay unanswered, the plan is
  // approved. A current client sending no `answers` is still refused.
  const legacyApproval =
    issue === `missing` &&
    card.kind === `plan` &&
    input.decision === `approved` &&
    input.answers === undefined &&
    input.legacyPlanApproval === true
  if (issue && !legacyApproval) return bad(PLAN_CARD_COPY.answerIssues[issue])
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
