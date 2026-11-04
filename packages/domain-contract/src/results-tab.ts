// #FEED-191 RESULTS TAB ×4 (fixtures/results-tab.json: its `$comment` is the
// spec): a chat tree's results, the session screen's last tab. TS reference;
// desktop `domain::results_tab`, iOS ExpCore `ResultsTab.swift`, Android
// `domain/ResultsTab.kt` replay `cases`.

import fixture from "../fixtures/results-tab.json" with { type: "json" }

export const RESULTS_TAB = fixture.tab

export interface ResultsSessionInput {
  id: string
  rootId: string
  /** The session's display name (session-title). */
  name: string
}

export interface ResultsMessageInput {
  id: string
  sessionId: string
  authorSessionId: string | null
  /** Epoch ms. */
  createdAt: number
  /** The raw `messages.card`. */
  card: unknown
}

export interface ResultPicture {
  label: string
  attachmentId: string
  width: number | null
  height: number | null
  caption: string | null
}

export interface Result {
  id: string
  sessionId: string
  sessionName: string
  createdAt: number
  /** `<m>m` / `<h>h` / `<d>d`, `now` under a minute. */
  age: string
  title: string
  description: string | null
  pictures: ResultPicture[]
  surface: boolean
}

type Card = Record<string, unknown>

const str = (value: unknown): string | null => (typeof value === `string` ? value : null)
const num = (value: unknown): number | null => (typeof value === `number` ? value : null)

function resolvedPictures(card: Card): ResultPicture[] {
  const list = Array.isArray(card.pictures) ? card.pictures : []
  const out: ResultPicture[] = []
  for (const raw of list) {
    if (!raw || typeof raw !== `object`) continue
    const picture = raw as Card
    const attachmentId = str(picture.attachmentId)
    if (!attachmentId) continue
    out.push({
      label: str(picture.label) ?? ``,
      attachmentId,
      width: num(picture.width),
      height: num(picture.height),
      caption: str(picture.caption),
    })
  }
  return out
}

/** The card's flag: `result`, else a card from before the rename's `story`. */
function explicitFlag(card: Card): boolean | null {
  if (typeof card.result === `boolean`) return card.result
  return typeof card.story === `boolean` ? card.story : null
}

/** A surface card that goes on the Results tab: the flag, else any resolved
 *  picture. Plans and questions never do. */
export function isResultCard(card: unknown): boolean {
  if (!card || typeof card !== `object`) return false
  const c = card as Card
  if (c.kind !== `surface` || str(c.questionId)) return false
  return explicitFlag(c) ?? resolvedPictures(c).length > 0
}

/** A card whose pictures are PR pictures (TS only: the server's PR body and
 *  `pr open` guard): a result, a plan or a question, unless flagged off. */
export function isPrPictureCard(card: unknown): boolean {
  if (!card || typeof card !== `object`) return false
  const c = card as Card
  if (c.kind !== `plan` && c.kind !== `question` && c.kind !== `surface`) return false
  if (str(c.questionId)) return false
  return explicitFlag(c) ?? (c.kind !== `surface` || resolvedPictures(c).length > 0)
}

/** `<m>m` / `<h>h` / `<d>d`, floored; under a minute = `copy.now`. */
export function resultAge(createdAt: number, now: number): string {
  const minutes = Math.floor(Math.max(0, now - createdAt) / 60_000)
  if (minutes < 1) return fixture.copy.now
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h`
  return `${Math.floor(hours / 24)}d`
}

/** THE fold: every result the root's tree posted, newest first. */
export function resultsTab(input: {
  now: number
  rootId: string
  sessions: readonly ResultsSessionInput[]
  messages: readonly ResultsMessageInput[]
}): Result[] {
  const sessions = new Map(input.sessions.map((session) => [session.id, session]))
  const out: Result[] = []
  for (const message of input.messages) {
    if (!message.authorSessionId || message.authorSessionId !== message.sessionId) continue
    if (!isResultCard(message.card)) continue
    const session = sessions.get(message.sessionId)
    if (!session || session.rootId !== input.rootId) continue
    const card = message.card as Card
    out.push({
      id: message.id,
      sessionId: session.id,
      sessionName: session.name,
      createdAt: message.createdAt,
      age: resultAge(message.createdAt, input.now),
      title: str(card.title)?.trim() || fixture.copy.untitled,
      description: str(card.description)?.trim() || null,
      pictures: resolvedPictures(card),
      surface: card.surface != null && typeof card.surface === `object`,
    })
  }
  return out.sort((a, b) => b.createdAt - a.createdAt || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0))
}

/** THE styleguide specimen ×4 (`tab.specimen`). */
export const RESULTS_TAB_SPECIMEN = fixture.cases[fixture.tab.specimen.case]!.expected as unknown as Result[]
