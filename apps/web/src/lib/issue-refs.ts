// Issue references are written as `#<IDENTIFIER>` (e.g. `#MET-115`) in issue
// descriptions and comment bodies — typeable by hand and inserted by the
// editor's #-autocomplete. Like `@email` mentions, the token is the single
// interchange form across all clients: it round-trips as plain GFM text (an
// inline `#` is never a heading — headings need `# ` at line start), so there
// is zero schema impact. Clients render a token as a clickable pill only when
// it resolves to an issue they can actually see; unresolved tokens stay plain
// text.
//
// This module is client-safe (no server imports) so the same regex drives the
// TipTap pill decorations, the composer autocomplete, and the server-side
// resolver in lib/integrations/mentions.ts.
//
// EXP-760: the STEERING feeds additionally chip BARE identifiers (`EXP-758`,
// no `#`) because agents narrate them that way. That is a display-only,
// opt-in mode (`{ bare: true }`): the bare form needs an UPPERCASE prefix
// (`utf-8` / `x86-64` never match), may follow `/` (`exp/EXP-758` chips), and
// is never fed to `extractIssueRefs`, so auto-relations and the stored-text
// contract keep the `#` form. iOS `IssueRefs.swift`, Android `IssueRefs.kt`
// and desktop `markdown/editor.rs` mirror BOTH sources byte for byte.

// `#` must not be glued to a word or another `#` (so `foo#MET-1` and `##MET-1`
// don't match), the identifier is `{PREFIX}-{number}` (prefixes are stored
// uppercase; matching is case-insensitive and normalized on extraction), and
// the match must end at a token boundary (so `#MET-115-2` / `#MET-115abc`
// don't half-match).
const HASH_REF_SOURCE = `(?<![\\w#])#([A-Za-z][A-Za-z0-9]*-\\d+)`
// Bare form: uppercase prefix, not glued to a word, `#` or `-` (so `foo-EXP-1`
// stays text but `exp/EXP-758` chips).
const BARE_REF_SOURCE = `(?<![\\w#-])([A-Z][A-Z0-9]*-\\d+)`
const REF_TAIL = `(?![\\w-])`

const ISSUE_REF_SOURCE = `${HASH_REF_SOURCE}${REF_TAIL}`
const ISSUE_REF_BARE_SOURCE = `(?:${HASH_REF_SOURCE}|${BARE_REF_SOURCE})${REF_TAIL}`

export interface IssueRefMatchOptions {
  /** Also match bare `EXP-758` tokens (steering feeds only, display-only). */
  bare?: boolean
}

export function createIssueRefRegExp(options?: IssueRefMatchOptions): RegExp {
  return new RegExp(options?.bare ? ISSUE_REF_BARE_SOURCE : ISSUE_REF_SOURCE, `g`)
}

/** The identifier a match captured, whichever alternative fired. */
export function matchedIdentifier(match: RegExpMatchArray): string {
  return match[1] ?? match[2]
}

/** Unique, uppercase-normalized identifiers referenced in `text`. */
export function extractIssueRefs(text: string): string[] {
  return [
    ...new Set(
      [...text.matchAll(createIssueRefRegExp())].map((m) =>
        matchedIdentifier(m).toUpperCase()
      ),
    ),
  ]
}

export interface IssueRefSegment {
  /** The verbatim slice; segments concatenate back to the input. */
  text: string
  /** Set on a reference segment: the identifier as written (not normalized). */
  identifier?: string
}

/**
 * Split `text` into plain and reference segments for renderers that chip
 * inline (the steering feed's plain-prose path). Joining the segments' `text`
 * reproduces the input exactly.
 */
export function splitIssueRefs(
  text: string,
  options?: IssueRefMatchOptions
): IssueRefSegment[] {
  const segments: IssueRefSegment[] = []
  let cursor = 0
  for (const match of text.matchAll(createIssueRefRegExp(options))) {
    const start = match.index ?? 0
    if (start > cursor) segments.push({ text: text.slice(cursor, start) })
    segments.push({ text: match[0], identifier: matchedIdentifier(match) })
    cursor = start + match[0].length
  }
  if (cursor < text.length) segments.push({ text: text.slice(cursor) })
  return segments
}
