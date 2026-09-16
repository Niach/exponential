// EXP-879: a coding run's published RESULTS — the screenshots the agent filed
// with `exponential_sessions_results` while it worked, read off the synced
// `coding_sessions.results` jsonb.
//
// The blob is a FLAT, ORDERED list: `{ topic, label, attachmentId, width,
// height }`. Grouping is derived, never stored, so an agent that publishes
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

export interface SessionResultEntry {
  topic: string
  label: string
  attachmentId: string
  /** Probed at upload; null when the image could not be measured. */
  width: number | null
  height: number | null
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
export function parseSessionResults(raw: unknown): SessionResultEntry[] {
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
  const entries: SessionResultEntry[] = []
  for (const row of value) {
    if (!row || typeof row !== `object` || Array.isArray(row)) continue
    const record = row as Record<string, unknown>
    const topic = text(record.topic)
    const label = text(record.label)
    const attachmentId = text(record.attachmentId)
    if (!topic || !label || !attachmentId) continue
    entries.push({
      topic,
      label,
      attachmentId,
      width: dimension(record.width),
      height: dimension(record.height),
    })
    if (entries.length >= MAX_SESSION_RESULTS) break
  }
  return entries
}

export interface SessionResultGroup {
  topic: string
  entries: SessionResultEntry[]
}

/** Groups by topic in FIRST-SEEN order, keeping each group's entries in the
 *  order the agent published them. */
export function groupSessionResults(
  entries: readonly SessionResultEntry[]
): SessionResultGroup[] {
  const groups: SessionResultGroup[] = []
  const byTopic = new Map<string, SessionResultGroup>()
  for (const entry of entries) {
    let group = byTopic.get(entry.topic)
    if (!group) {
      group = { topic: entry.topic, entries: [] }
      byTopic.set(entry.topic, group)
      groups.push(group)
    }
    group.entries.push(entry)
  }
  return groups
}

/** The tile's width at a fixed height — the probed aspect, else 4:3 (a
 *  desktop screenshot's shape, and the least surprising placeholder). */
export function sessionResultTileWidth(
  entry: Pick<SessionResultEntry, `width` | `height`>,
  height: number = SESSION_RESULT_TILE_HEIGHT
): number {
  const aspect =
    entry.width !== null && entry.height !== null
      ? entry.width / entry.height
      : 4 / 3
  return Math.round(height * aspect)
}
