// EXP-1283: THE Guide picture viewer ×4. Pure and byte-locked by
// fixtures/session-results.json `viewer.cases`; mirrors: desktop
// `domain::session_results`, iOS ExpCore `SessionResults.swift`, Android
// `domain/SessionResults.kt`. The fixture's `viewer.$comment` holds the rules.

export interface GuideViewerEntry {
  topic?: unknown
  label?: unknown
  attachmentId?: unknown
  text?: unknown
}

export interface GuideViewerState {
  /** 1-based place of the picture among every Guide picture. */
  index: number
  count: number
  topic: string
  label: string
  /** The picture's topic text (its first non-blank text entry), trimmed. */
  text: string | null
  prevId: string | null
  nextId: string | null
}

const str = (value: unknown): string | null =>
  typeof value === `string` && value.trim().length > 0 ? value : null

const isSummary = (topic: string) => topic.trim().toLowerCase() === `summary`

/** The viewer over `attachmentId`: every picture of the Guide in the
 *  Guide's order (the first Summary topic leads, then topics first-seen,
 *  pictures in publish order), no wrap at the ends; null when the
 *  attachment is not a Guide picture. */
export function guideViewer(
  guide: readonly GuideViewerEntry[] | null | undefined,
  attachmentId: string
): GuideViewerState | null {
  const topics: string[] = []
  const texts = new Map<string, string>()
  const pictures = new Map<string, { label: string; attachmentId: string }[]>()
  for (const entry of guide ?? []) {
    if (!entry || typeof entry !== `object`) continue
    const topic = str(entry.topic)
    if (!topic) continue
    const id = str(entry.attachmentId)
    const label = str(entry.label)
    const text = str(entry.text)
    if (id !== null && label !== null) {
      if (!topics.includes(topic)) topics.push(topic)
      const list = pictures.get(topic) ?? []
      list.push({ label, attachmentId: id })
      pictures.set(topic, list)
    } else if (text !== null) {
      if (!topics.includes(topic)) topics.push(topic)
      if (!texts.has(topic)) texts.set(topic, text.trim())
    }
  }
  const lead = topics.find(isSummary)
  const ordered = lead ? [lead, ...topics.filter((topic) => topic !== lead)] : topics
  const flat = ordered.flatMap((topic) =>
    (pictures.get(topic) ?? []).map((picture) => ({ topic, ...picture }))
  )
  const at = flat.findIndex((picture) => picture.attachmentId === attachmentId)
  if (at < 0) return null
  const picture = flat[at]
  return {
    index: at + 1,
    count: flat.length,
    topic: picture.topic,
    label: picture.label,
    text: texts.get(picture.topic) ?? null,
    prevId: at > 0 ? flat[at - 1].attachmentId : null,
    nextId: at < flat.length - 1 ? flat[at + 1].attachmentId : null,
  }
}
