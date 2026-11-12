// #FEED-219: every composer's synced DRAFT. Pure and locked by
// `fixtures/composer-draft.json` (`keys`, `unparsable`, `writes`, `fields`),
// which every client replays; the fixture's `$comment` holds the rules.

export const COMPOSER_DRAFT_KINDS = [
  `comment`,
  `reply`,
  `session`,
  `agent`,
  `app`,
  `newChat`,
] as const

export type ComposerDraftKind = (typeof COMPOSER_DRAFT_KINDS)[number]

/** The target a composer writes to. */
export interface ComposerDraftTarget {
  kind: ComposerDraftKind
  id: string
}

/** The fixture's debounce after the last keystroke. */
export const COMPOSER_DRAFT_DEBOUNCE_MS = 800

/** The longest key a row may carry (`kind:` + a uuid fits easily). */
export const COMPOSER_DRAFT_KEY_MAX = 80

export function composerDraftKey(target: ComposerDraftTarget): string {
  return `${target.kind}:${target.id}`
}

export function parseComposerDraftKey(key: string): ComposerDraftTarget | null {
  const at = key.indexOf(`:`)
  if (at <= 0) return null
  const kind = key.slice(0, at)
  const id = key.slice(at + 1)
  if (id.length === 0) return null
  if (!(COMPOSER_DRAFT_KINDS as readonly string[]).includes(kind)) return null
  return { kind: kind as ComposerDraftKind, id }
}

export type ComposerDraftWrite = { op: `upsert`; text: string } | { op: `delete` }

/** What writing the field's text does to the row. */
export function composerDraftWrite(text: string): ComposerDraftWrite {
  return text.trim().length === 0 ? { op: `delete` } : { op: `upsert`, text }
}

export interface ComposerDraftFieldInput {
  /** The field's text right now. */
  field: string
  /** The person edited the field since the composer opened. */
  edited: boolean
  /** This client's own write, not yet echoed back by the shape. */
  local: string | null
  /** The synced row's text (null = no row). */
  synced: string | null
}

/** What the field shows. */
export function composerDraftField(input: ComposerDraftFieldInput): string {
  if (input.edited) return input.field
  return input.local ?? input.synced ?? ``
}
