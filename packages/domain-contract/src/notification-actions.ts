// #EXP-1641: answering from the notification. Pure and locked by
// `fixtures/notification-actions.json`, replayed by iOS ExpPush
// `NotificationActions.swift` and Android `data/push/NotificationActions.kt`.
// The fixture's `$comment` holds the rules; this is the reference.

import fixture from "../fixtures/notification-actions.json" with { type: "json" }

export const NOTIFICATION_ACTIONS = fixture

export interface NotificationAction {
  /** `approve` | `request-changes` | `answer` | `reply` | `open` | `option:<i>` */
  id: string
  kind: `button` | `text`
  title: string
}

const FLAGS = /\s\(((?:several allowed|free text allowed)(?:, (?:several allowed|free text allowed))*)\)$/
const OPTIONS_LEAD = `Options: `
const PLAN_QUESTIONS = `\n\nQuestions:\n- `

/** A question's options and free-text flag, read from its bus ask text
 *  (apps/web lib/sessions/bus.ts `askText`). */
export function askTextOptions(askText: string): { options: string[]; custom: boolean } {
  const line = askText
    .split(`\n`)
    .slice(1)
    .find((row) => row.startsWith(OPTIONS_LEAD))
  if (!line) return { options: [], custom: true }
  let rest = line.slice(OPTIONS_LEAD.length)
  const flags = FLAGS.exec(rest)
  if (flags) rest = rest.slice(0, flags.index)
  const options = rest.split(` | `).filter((option) => option.length > 0)
  return { options, custom: flags ? flags[1]!.includes(`free text allowed`) : false }
}

const button = (id: string, title: string): NotificationAction => ({ id, kind: `button`, title })
const text = (id: string, title: string): NotificationAction => ({ id, kind: `text`, title })

function parseOptions(raw: unknown): string[] {
  if (typeof raw !== `string`) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((option): option is string => typeof option === `string`) : []
  } catch {
    return []
  }
}

/** The actions a delivered push offers, in order (`[]` = none). */
export function notificationActions(input: {
  data: Record<string, string | undefined>
  /** The phone holds the session tree's key. */
  keyHeld: boolean
  /** A sealed card's ask text, opened on the phone (null = not opened). */
  openedAskText: string | null
}): NotificationAction[] {
  const { data } = input
  const keys = fixture.dataKeys
  const sealed = data[keys.sealed] === `1`
  const kind = data[keys.cardKind]
  const open = button(`open`, fixture.copy.open)
  if (kind === `plan` || kind === `question`) {
    if (!data[keys.messageId]) return [open]
    const opened = sealed ? (input.keyHeld ? input.openedAskText : null) : null
    if (sealed && opened === null) return [open]
    if (kind === `plan`) {
      const questions = opened !== null ? opened.includes(PLAN_QUESTIONS) : Number(data[keys.planQuestions] ?? `0`) > 0
      const changes = text(`request-changes`, fixture.copy.requestChanges)
      return questions ? [changes, open] : [button(`approve`, fixture.copy.approve), changes, open]
    }
    const { options, custom } =
      opened !== null
        ? askTextOptions(opened)
        : { options: parseOptions(data[keys.options]), custom: data[keys.custom] !== `0` }
    const room = custom ? fixture.maxOptionButtons - 1 : fixture.maxOptionButtons
    const buttons = options.length <= room ? options.map((option, i) => button(`option:${i}`, option)) : []
    const actions = custom ? [...buttons, text(`answer`, fixture.copy.answer)] : buttons
    return actions.length ? actions : [open]
  }
  if (data.sessionId) return sealed && !input.keyHeld ? [] : [text(`reply`, fixture.copy.reply)]
  return []
}
