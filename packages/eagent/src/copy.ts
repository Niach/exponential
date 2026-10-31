// The hub's own lines, rendered from fixtures/rules.json `copy`.

import rules from "../fixtures/rules.json" with { type: "json" }
import type { Blocked, PlanCard, QuestionCard } from "./model"

export { rules }

const copy = rules.copy

export function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) => (key in values ? String(values[key]) : whole))
}

export function blockedText(blocked: Pick<Blocked, `window` | `resetsAt`>): string {
  const window = blocked.window ? fill(copy.blockedDetail.window, { window: blocked.window }) : ``
  const resets = blocked.resetsAt
    ? `${window ? `, ` : ` (`}${fill(copy.blockedDetail.resets, { resetsAt: blocked.resetsAt })}`
    : window
      ? `)`
      : ``
  return fill(copy.blocked, { detail: `${window}${resets}` })
}

export const unclaimedText = () => fill(copy.unclaimed, { minutes: rules.watchdog.unclaimedMinutes })
export const stuckText = (minutes: number) => fill(copy.stuck, { minutes })
export const unresumedText = () => fill(copy.unresumed, { minutes: rules.watchdog.resumeMinutes })
export const prOpenedText = (number: number, url: string) => fill(copy.prOpened, { number, url })
export const privateText = (id: string, lead: string) => fill(copy.private, { lead, id })

/** Whole minutes since `alive`, never fewer than the stuck threshold (a skewed clock). */
export function silentMinutes(alive: number, now: number): number {
  const minutes = Number.isFinite(alive) ? Math.floor((now - alive) / 60_000) : 0
  return Math.max(rules.watchdog.stuckMinutes, minutes)
}

function clip(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1).trimEnd()}…` : text
}

/** What a listener's agent reads for a child's open question or plan. */
export function askText(id: string, card: QuestionCard | PlanCard): string {
  const ask = copy.askQuestion
  if (card.kind === `question`) {
    const options = (card.options ?? []).filter((option) => typeof option === `string`)
    const flags = [card.multi ? ask.several : null, card.custom !== false ? ask.freeText : null].filter(Boolean)
    return [
      `${copy.askQuestionLead}${card.question}`,
      options.length ? fill(ask.options, { options: options.join(` | `), flags: flags.length ? ` (${flags.join(`, `)})` : `` }) : null,
      fill(ask.answer, { id }),
    ]
      .filter(Boolean)
      .join(`\n`)
  }
  const questions = (card.questions ?? []).map(
    (question) => `- ${question.id}: ${question.prompt} (${question.options.map((option) => option.label).join(` | `)})`
  )
  return [
    copy.askPlanLead,
    clip(card.plan, rules.ask.planMax),
    questions.length ? `${ask.questions}\n${questions.join(`\n`)}` : null,
    fill(ask.decide, { id }),
  ]
    .filter(Boolean)
    .join(`\n\n`)
}

/** The neutral ask line for a listener whose readers may not see the child. */
export function askHiddenText(id: string, card: QuestionCard | PlanCard): string {
  return fill(copy.askHidden, {
    lead: card.kind === `plan` ? copy.askPrivatePlanLead : copy.askPrivateQuestionLead,
    id,
  })
}

export function scheduleText(cron: string, tz: string | null, caughtUp: boolean): string {
  return fill(copy.schedule, {
    cron,
    tz: tz ? fill(copy.scheduleTz, { tz }) : ``,
    caughtUp: caughtUp ? copy.scheduleCaughtUp : ``,
  })
}

export function eventText(name: string, emittedBy: `session` | `user` | `server`, payload: unknown): string {
  return fill(copy.event, {
    name,
    from: copy.eventFrom[emittedBy] ?? copy.eventFrom.server,
    payload: JSON.stringify(payload, null, 2),
  })
}
