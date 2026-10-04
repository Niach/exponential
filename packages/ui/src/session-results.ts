// EXP-879: a coding run's published RESULTS — the screenshots the agent filed
// with `exponential_sessions_results` while it worked, read off the synced
// `coding_sessions.results` jsonb.
//
// The blob is a FLAT, ORDERED list: `{ topic, label, attachmentId, width,
// height }`, and since EXP-933 a topic's report text `{ topic, text }`. Grouping is derived, never stored, so an agent that publishes
// `web` then `ios` under one topic and later a second topic keeps the order it
// chose. `(topic, label)` is the upsert key (the server replaces in place), so
// a client only ever renders what it reads.
//
// These are the PURE rules every client mirrors byte for byte: desktop
// `crates/ui/src/session_results.rs`, iOS `ExpCore/Domain/SessionResults.swift`,
// Android `domain/SessionResults.kt` — same names, same order, same test names.

/** The cap the server enforces on a single run's list. */
export const MAX_SESSION_RESULTS = 60

/** Every tile renders at ONE height; the probed aspect gives its width, so a
 *  row of an iOS, an Android and a web shot reads as one strip. */
export const SESSION_RESULT_TILE_HEIGHT = 320

/** EXP-1128: a picture whose probed width/height is UNDER this is TALL (a
 *  full-page capture; a phone shot at ~0.46 never is). A tall picture takes
 *  the 4:3 frame top-cropped with a Tall badge instead of rendering as a
 *  sliver, and opens fit-to-width in a vertical scroll. Strict: exactly 1:3
 *  is not tall. Fixture `session-results.json` `tiles` (×4). */
export const SESSION_RESULT_TALL_ASPECT = 1 / 3

/** EXP-1172: an `exponential_sessions_show` picture's tile in the run
 *  transcript, one base height lower than a Results tile so a shot sits in
 *  the conversation without swallowing it. Fixture `session-inline.json`. */
export const SESSION_INLINE_TILE_HEIGHT = 240

/** EXP-1172: the collapsed band a Results group folds its inline pictures
 *  under (`Earlier · 3`). */
export const SESSION_RESULTS_EARLIER_LABEL = `Earlier`

export interface SessionResultEntry {
  topic: string
  label: string
  attachmentId: string
  /** Probed at upload; null when the image could not be measured. */
  width: number | null
  height: number | null
  /** EXP-1172: filed by `exponential_sessions_show` while the run worked
   *  (true only for a JSON true). */
  inline: boolean
  /** EXP-1172: the show call's `text`, trimmed; null when blank. */
  caption: string | null
}

function text(value: unknown): string | null {
  if (typeof value !== `string`) return null
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

/** A probed dimension, or null: a zero, a negative, a NaN and an Infinity all
 *  mean "unknown" and fall back to the 4:3 default. */
function dimension(value: unknown): number | null {
  if (typeof value !== `number`) return null
  if (!Number.isFinite(value) || value <= 0) return null
  return value
}

/**
 * Tolerant reader for the jsonb blob. Takes the parsed array, a JSON string
 * (the natives hand their decoder the raw column), null or anything else, and
 * always returns a clean list: a malformed entry is DROPPED, never rendered as
 * a broken tile, and the list is capped like the writer caps it.
 */
function records(raw: unknown): Record<string, unknown>[] {
  let value = raw
  if (typeof value === `string`) {
    const trimmed = value.trim()
    if (trimmed.length === 0) return []
    try {
      value = JSON.parse(trimmed)
    } catch {
      return []
    }
  }
  if (!Array.isArray(value)) return []
  const out: Record<string, unknown>[] = []
  for (const row of value) {
    if (!row || typeof row !== `object` || Array.isArray(row)) continue
    out.push(row as Record<string, unknown>)
  }
  return out
}

function picture(record: Record<string, unknown>): SessionResultEntry | null {
  const topic = text(record.topic)
  const label = text(record.label)
  const attachmentId = text(record.attachmentId)
  if (!topic || !label || !attachmentId) return null
  return {
    topic,
    label,
    attachmentId,
    width: dimension(record.width),
    height: dimension(record.height),
    inline: record.inline === true,
    caption: text(record.caption),
  }
}

export function parseSessionResults(raw: unknown): SessionResultEntry[] {
  const entries: SessionResultEntry[] = []
  for (const record of records(raw)) {
    const entry = picture(record)
    if (!entry) continue
    entries.push(entry)
    if (entries.length >= MAX_SESSION_RESULTS) break
  }
  return entries
}

export interface SessionResultGroup {
  topic: string
  /** EXP-933: the topic's GFM report text (rendered ABOVE its pictures), null
   *  without one. */
  text: string | null
  entries: SessionResultEntry[]
  /** EXP-1172: the topic's INLINE pictures, folded under the `Earlier` band
   *  (publish order); empty when the topic has a single picture. */
  earlier: SessionResultEntry[]
}

/** EXP-1172: a topic with more than one picture moves its inline ones into
 *  `earlier`, so the final report leads; a topic's only picture stays. */
function foldInline(groups: SessionResultGroup[]): SessionResultGroup[] {
  for (const group of groups) {
    if (group.entries.length < 2) continue
    group.earlier = group.entries.filter((entry) => entry.inline)
    group.entries = group.entries.filter((entry) => !entry.inline)
  }
  return groups
}

/** Groups by topic in FIRST-SEEN order, keeping each group's entries in the
 *  order the agent published them. Pictures only: `text` stays null. */
export function groupSessionResults(
  entries: readonly SessionResultEntry[]
): SessionResultGroup[] {
  const groups: SessionResultGroup[] = []
  const byTopic = new Map<string, SessionResultGroup>()
  for (const entry of entries) {
    let group = byTopic.get(entry.topic)
    if (!group) {
      group = { topic: entry.topic, text: null, entries: [], earlier: [] }
      byTopic.set(entry.topic, group)
      groups.push(group)
    }
    group.entries.push(entry)
  }
  return foldInline(groups)
}

/**
 * EXP-933: the Results face as a REPORT — pictures AND each topic's text
 * (`{topic, label: null, attachmentId: null, text}`), grouped in FIRST-SEEN
 * topic order whichever kind opened the topic. A topic's text is its FIRST
 * non-blank text entry, trimmed; pictures keep the 60 cap. Fixture:
 * `packages/domain-contract/fixtures/session-results.json` (×4).
 */
export function parseSessionResultGroups(raw: unknown): SessionResultGroup[] {
  const groups: SessionResultGroup[] = []
  const byTopic = new Map<string, SessionResultGroup>()
  const open = (topic: string) => {
    let group = byTopic.get(topic)
    if (!group) {
      group = { topic, text: null, entries: [], earlier: [] }
      byTopic.set(topic, group)
      groups.push(group)
    }
    return group
  }
  let pictures = 0
  for (const record of records(raw)) {
    const entry = picture(record)
    if (entry) {
      if (pictures >= MAX_SESSION_RESULTS) continue
      pictures += 1
      open(entry.topic).entries.push(entry)
      continue
    }
    const topic = text(record.topic)
    const body = text(record.text)
    if (!topic || !body) continue
    const group = open(topic)
    if (group.text === null) group.text = body
  }
  return foldInline(groups)
}

/** True when the blob has anything for the Results face to show. */
export function hasSessionResults(raw: unknown): boolean {
  return parseSessionResultGroups(raw).length > 0
}

/** Every picture of a set of groups, in order — what the tile sizing reads
 *  (the folded `earlier` ones too: expanding the band never resizes). */
export function sessionResultPictures(
  groups: readonly SessionResultGroup[]
): SessionResultEntry[] {
  return groups.flatMap((group) => [...group.entries, ...(group.earlier ?? [])])
}

/** EXP-1172: the picture an `exponential_sessions_show` call filed, by the
 *  attachment id its answer carried (`preview.id`); null while the upload is
 *  still in flight, once it was removed, or for a blank id. */
export function sessionResultPicture(
  raw: unknown,
  attachmentId: string | null | undefined
): SessionResultEntry | null {
  const id = attachmentId?.trim()
  if (!id) return null
  return parseSessionResults(raw).find((entry) => entry.attachmentId === id) ?? null
}

/** EXP-1172: the line under a transcript tile — the show call's caption,
 *  else the picture's label. */
export function sessionResultTileCaption(
  entry: Pick<SessionResultEntry, `label` | `caption`>
): string {
  return entry.caption ?? entry.label
}

/** EXP-1128: true when the probed aspect is under `SESSION_RESULT_TALL_ASPECT`;
 *  an unmeasured picture is never tall. */
export function sessionResultIsTall(
  entry: Pick<SessionResultEntry, `width` | `height`>
): boolean {
  return (
    entry.width !== null &&
    entry.height !== null &&
    entry.width / entry.height < SESSION_RESULT_TALL_ASPECT
  )
}

/** The tile's width at a fixed height — the probed aspect, else 4:3 (a
 *  desktop screenshot's shape, and the least surprising placeholder). A TALL
 *  picture (EXP-1128) takes the 4:3 frame too: the tile shows its top, never
 *  a sliver. */
export function sessionResultTileWidth(
  entry: Pick<SessionResultEntry, `width` | `height`>,
  height: number = SESSION_RESULT_TILE_HEIGHT
): number {
  const aspect =
    entry.width !== null && entry.height !== null && !sessionResultIsTall(entry)
      ? entry.width / entry.height
      : 4 / 3
  return Math.round(height * aspect)
}

/**
 * The tile height that makes the page FIT: on a phone a landscape shot is
 * 480px wide at the 320px base and a 390px screen clips it, so the whole page
 * scales down by ONE factor — the widest tile's overflow — instead of letting
 * a row clip or each row pick its own size. One factor keeps every tile's
 * aspect (`sessionResultTileWidth(entry, thatHeight)`) AND the equal-height
 * strip, which is the point of a fixed height: an iOS, an Android and a web
 * shot of one screen still read as one row. Never scales UP: a wide page keeps
 * the base so shots never look blown out.
 *
 * `availableWidth` is the tiles container's content width; a zero or a
 * non-finite one means "not measured yet" and renders at the base.
 */
export function sessionResultTileHeightFitting(
  entries: readonly SessionResultEntry[],
  availableWidth: number,
  base: number = SESSION_RESULT_TILE_HEIGHT
): number {
  if (!Number.isFinite(availableWidth) || availableWidth <= 0) return base
  let widest = 0
  for (const entry of entries) {
    const width = sessionResultTileWidth(entry, base)
    if (width > widest) widest = width
  }
  if (widest <= availableWidth) return base
  return Math.max(1, Math.floor((base * availableWidth) / widest))
}
