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
export const undeliveredText = () => fill(copy.undelivered, { minutes: rules.watchdog.undeliveredMinutes })
export const stuckText = (minutes: number) => fill(copy.stuck, { minutes })
export const unresumedText = () => fill(copy.unresumed, { minutes: rules.watchdog.resumeMinutes })
export const offlineText = () => fill(copy.offline, { minutes: rules.watchdog.offlineMinutes })
export const prOpenedText = (number: number, url: string) => fill(copy.prOpened, { number, url })
export const privateText = (id: string, lead: string) => fill(copy.private, { lead, id })

/** Whole minutes since `alive`, never fewer than the stuck threshold (a skewed clock). */
export function silentMinutes(alive: number, now: number): number {
  const minutes = Number.isFinite(alive) ? Math.floor((now - alive) / 60_000) : 0
  return Math.max(rules.watchdog.stuckMinutes, minutes)
}

/** `text.slice(0, end)` in UTF-16 code units; a surrogate pair cut in half becomes U+FFFD (the Rust core
 *  cannot hold a lone surrogate, so the spec says U+FFFD for both). */
export function sliceUnits(text: string, end: number): string {
  const head = text.slice(0, end)
  const last = head.charCodeAt(head.length - 1)
  return last >= 0xd800 && last <= 0xdbff ? `${head.slice(0, -1)}\uFFFD` : head
}

function clip(text: string, max: number): string {
  return text.length > max ? `${sliceUnits(text, max - 1).trimEnd()}…` : text
}

const STAMP =
  /^(?:([+-])(\d{6})|(\d{4}))-(\d{2})-(\d{2})(?:[Tt](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?(?:([Zz])|([+-])(\d{2}):(\d{2})))?$/

/** Days since 1970-01-01 of a proleptic Gregorian date (out-of-range days roll over like `MakeDay`). */
function daysFromCivil(year: number, month: number, day: number): number {
  const y = month <= 2 ? year - 1 : year
  const era = Math.floor(y / 400)
  const yoe = y - era * 400
  const mp = (month + 9) % 12
  const doy = Math.floor((153 * mp + 2) / 5) + day - 1
  const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy
  return era * 146_097 + doe - 719_468
}

/** A usage wall's `resetsAt` in unix ms: an ISO date (`YYYY-MM-DD`, UTC) or an RFC 3339 date-time WITH its
 *  offset (`Z` or `±HH:MM`, optional fraction). Anything else, a timezone-less date-time included (local time
 *  in `Date.parse`, so it would expire at a different instant on every host), is NaN: the wall never expires. */
export function stampMs(stamp: string): number {
  const m = STAMP.exec(stamp)
  if (!m) return Number.NaN
  const [, sign, wide, plain, mo, d, h, mi, sec, frac, zulu, offSign, offH, offM] = m
  if (sign === `-` && Number(wide) === 0) return Number.NaN
  const year = plain !== undefined ? Number(plain) : sign === `-` ? -Number(wide) : Number(wide)
  const month = Number(mo)
  const day = Number(d)
  if (month < 1 || month > 12 || day < 1 || day > 31) return Number.NaN
  const date = daysFromCivil(year, month, day) * 86_400_000
  if (h === undefined) return date
  if (zulu === undefined && offSign === undefined) return Number.NaN
  const hour = Number(h)
  const minute = Number(mi)
  const second = sec === undefined ? 0 : Number(sec)
  const millis = frac === undefined ? 0 : Number(frac.slice(0, 3).padEnd(3, `0`))
  const time = (hour < 24 && minute < 60 && second < 60) || (hour === 24 && minute === 0 && second === 0 && millis === 0)
  if (!time) return Number.NaN
  let offset = 0
  if (offSign !== undefined) {
    if (Number(offH) > 23 || Number(offM) > 59) return Number.NaN
    offset = (Number(offH) * 60 + Number(offM)) * (offSign === `-` ? -1 : 1)
  }
  return date + ((hour * 60 + minute - offset) * 60 + second) * 1000 + millis
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
