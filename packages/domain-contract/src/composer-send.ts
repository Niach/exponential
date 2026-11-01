// EXP-1537: what a send leaves in THE composer. Pure and locked by
// `fixtures/composer-send.json` (`cases`), which every mirror replays: desktop
// `domain::composer_send`, iOS `ExpCore/Sources/ComposerSend.swift`, Android
// `domain/ComposerSend.kt`. The fixture's `$comment` holds the rules.

export interface ComposerContent {
  text: string
  /** The attachments' ids (a client's own key per picked file). */
  attachments: readonly string[]
}

export interface ComposerSendInput {
  outcome: `sent` | `failed`
  /** The field when Send was pressed (untrimmed) and what went out. */
  sent: ComposerContent
  /** The field when the send settled. */
  current: ComposerContent
}

export function composerAfterSend(input: ComposerSendInput): ComposerContent {
  const { outcome, sent, current } = input
  if (outcome === `failed`) return { text: current.text, attachments: [...current.attachments] }
  const gone = new Set(sent.attachments)
  return {
    text: current.text === sent.text ? `` : current.text,
    attachments: current.attachments.filter((id) => !gone.has(id)),
  }
}

/** The text half alone (a composer without attachments). */
export function composerTextAfterSend(sent: string, current: string): string {
  return current === sent ? `` : current
}
