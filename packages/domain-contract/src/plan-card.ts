// EXP-1280 custom plan mode: THE plan / question card's behaviour ×4 (the
// render rules live in fixtures/plan-card.json's $comment). TS reference;
// desktop `domain::plan_card`, iOS ExpCore `PlanCard.swift`, Android
// `domain/PlanCard.kt` replay the fixture's cases.

import fixture from "../fixtures/plan-card.json" with { type: "json" }
import contractJson from "../contract.json" with { type: "json" }

export const PLAN_CARD_COPY = fixture.copy
export const PLAN_CARD_TEST_IDS = fixture.testIds
export const PLAN_CARD_TILES = fixture.tiles
export const PLAN_CARD_ANSWER_TEXT = fixture.answerText
export const PLAN_CARD_UPLOAD_SCHEME = fixture.uploadScheme
export const PLAN_CARD_CAPS = contractJson.planCard

export interface PlanCardPicture {
  label: string
  attachmentId: string | null
  width: number | null
  height: number | null
  caption?: string | null
}

export interface PlanCardOption {
  label: string
  description?: string | null
  /** The picture's LABEL in `pictures` (resolved through `pictureFor`). */
  picture?: string | null
}

export interface PlanCardQuestion {
  id: string
  prompt: string
  multi?: boolean | null
  /** Free text allowed (default true). */
  custom?: boolean | null
  options: PlanCardOption[]
}

/** The surface an agent authored: an Exponential UI core NestedNode + data. */
export interface CardSurface {
  root: unknown
  data?: Record<string, unknown> | null
}

export type PlanDecision = `approved` | `rejected`

/** The `plan` card as stored (additive on the EXP-1291 shape). */
export interface PlanCardLike {
  kind: `plan`
  plan: string
  pictures?: PlanCardPicture[] | null
  questions?: PlanCardQuestion[] | null
  surface?: CardSurface | null
  decision?: PlanDecision | null
  fresh?: boolean | null
  feedback?: string | null
  answers?: Record<string, string[]> | null
  surfaceData?: Record<string, unknown> | null
}

/** The `question` card as stored (additive on the EXP-1322 shape). */
export interface QuestionCardLike {
  kind: `question`
  question: string
  options?: string[] | null
  optionDescriptions?: (string | null)[] | null
  /** Picture LABELS parallel to `options` (null = none). */
  optionPictures?: (string | null)[] | null
  pictures?: PlanCardPicture[] | null
  multi?: boolean | null
  custom?: boolean | null
  surface?: CardSurface | null
  /** Compat: the labels joined by `answerText.joiner`, or free text. */
  answer?: string | null
  answers?: string[] | null
  surfaceData?: Record<string, unknown> | null
}

export type AskCardLike = PlanCardLike | QuestionCardLike

export interface CardAnswerInput {
  answer?: string | null
  answers?: Record<string, string[]> | string[] | null
  decision?: PlanDecision | null
  fresh?: boolean | null
  feedback?: string | null
  surfaceData?: Record<string, unknown> | null
}

export type AnswerIssue = keyof typeof fixture.copy.answerIssues

/** Why an answer cannot be sent (null = fine). A plan needs a decision;
 *  approving needs every question answered; a single-choice question takes
 *  one answer; without `custom` every answer must be an option's label. A
 *  question card needs one answer (`answers`, else `answer`). */
export function answerIssue(card: AskCardLike, input: CardAnswerInput): AnswerIssue | null {
  if (card.kind === `plan`) {
    if (input.decision !== `approved` && input.decision !== `rejected`) return `decision`
    const answers = isRecord(input.answers) ? input.answers : {}
    const questions = card.questions ?? []
    for (const id of Object.keys(answers)) {
      if (!questions.some((question) => question.id === id)) return `unknown`
    }
    for (const question of questions) {
      const picked = (answers[question.id] ?? []).map((label) => label.trim()).filter(Boolean)
      if (picked.length === 0) {
        if (input.decision === `approved`) return `missing`
        continue
      }
      const issue = optionIssue(question.options.map((option) => option.label), question.multi === true, question.custom !== false, picked)
      if (issue) return issue
    }
    return null
  }
  const picked = Array.isArray(input.answers)
    ? input.answers.map((label) => label.trim()).filter(Boolean)
    : input.answer?.trim()
      ? [input.answer.trim()]
      : []
  if (picked.length === 0) return `missing`
  return optionIssue(card.options ?? [], card.multi === true, card.custom !== false, picked)
}

function optionIssue(labels: string[], multi: boolean, custom: boolean, picked: string[]): AnswerIssue | null {
  if (!multi && picked.length > 1) return `multi`
  if (!custom && picked.some((label) => !labels.some((option) => option.trim() === label))) return `unknown`
  return null
}

function isRecord(value: unknown): value is Record<string, string[]> {
  return typeof value === `object` && value !== null && !Array.isArray(value)
}

/** The text the agent reads as its next message once the card is answered:
 *  the decision line (a fresh approval carries the plan), the answers block
 *  (answered questions only, card order), the surface's data model as
 *  compact JSON with sorted keys, the feedback. Paragraphs joined by a blank
 *  line. */
export function answerText(card: AskCardLike): string {
  const t = fixture.answerText
  const parts: string[] = []
  if (card.kind === `plan`) {
    if (card.decision === `approved`) {
      parts.push(card.fresh === true ? t.approvedFresh.replace(`{plan}`, card.plan) : t.approved)
    } else {
      parts.push(t.rejected)
    }
    const answers = card.answers ?? {}
    const lines = (card.questions ?? [])
      .filter((question) => (answers[question.id] ?? []).length > 0)
      .map((question) =>
        t.answerLine.replace(`{prompt}`, question.prompt).replace(`{answer}`, (answers[question.id] ?? []).join(t.joiner))
      )
    if (lines.length > 0) parts.push([t.answers, ...lines].join(`\n`))
  } else {
    const picked = card.answers && card.answers.length > 0 ? card.answers.join(t.joiner) : (card.answer ?? ``)
    parts.push(t.question.replace(`{question}`, card.question).replace(`{answer}`, picked))
  }
  if (card.surfaceData && typeof card.surfaceData === `object`) {
    parts.push(t.surfaceData.replace(`{json}`, stableJson(card.surfaceData)))
  }
  if (card.kind === `plan` && card.feedback?.trim()) {
    parts.push(t.feedback.replace(`{feedback}`, card.feedback.trim()))
  }
  return parts.join(`\n\n`)
}

/** JSON with object keys sorted at every depth (one byte sequence ×4). */
export function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(`,`)}]`
  if (value && typeof value === `object`) {
    const entries = Object.entries(value as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    return `{${entries.map(([key, inner]) => `${JSON.stringify(key)}:${stableJson(inner)}`).join(`,`)}}`
  }
  return JSON.stringify(value) ?? `null`
}

export interface ResolvedUpload {
  label: string
  attachmentId: string
  width: number | null
  height: number | null
}

/** The card once `label`'s upload landed: its `pictures` entry filled and,
 *  on a plan, every `](upload:<label>)` embed pointed at the attachment.
 *  An unknown label changes nothing. */
export function resolveUpload<T extends AskCardLike>(card: T, upload: ResolvedUpload): T {
  const pictures = card.pictures ?? []
  if (!pictures.some((picture) => picture.label === upload.label)) return card
  const next: AskCardLike = {
    ...card,
    pictures: pictures.map((picture) =>
      picture.label === upload.label
        ? { ...picture, attachmentId: upload.attachmentId, width: upload.width, height: upload.height }
        : picture
    ),
  }
  if (next.kind === `plan`) {
    next.plan = next.plan.split(`](${fixture.uploadScheme}${upload.label})`).join(`](/api/attachments/${upload.attachmentId})`)
  }
  return next as T
}

/** The attachment a picture label resolved to (null = none / not yet). */
export function pictureFor(pictures: PlanCardPicture[] | null | undefined, label: string | null | undefined): string | null {
  if (!label || !pictures) return null
  return pictures.find((picture) => picture.label === label)?.attachmentId ?? null
}

export type OptionRows = `list` | `tiles`

/** Text-only options = THE option list; any option with a picture = tiles. */
export function optionRows(options: readonly { attachmentId?: string | null }[]): OptionRows {
  return options.some((option) => option.attachmentId) ? `tiles` : `list`
}

/** The question card's compat `answer` string for a list of picks. */
export function joinAnswers(answers: readonly string[]): string {
  return answers.join(fixture.answerText.joiner)
}
