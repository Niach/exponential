import fixture from "@exp/domain-contract/fixtures/reporter-text.json"

// SLOP-4: widget-reporter text is UNTRUSTED plain text — the submission
// message and the reporter page's replies. The server escapes it ONCE into
// GFM that renders as the literal text on every client, so none of the four
// renderers needs a plain-text branch, and no mention, issue-ref or autolink
// token can fire from it (the server never runs the resolvers on these
// paths either). Locked by fixtures/reporter-text.json; the migration
// 0158's `pg_temp.slop4_escape` is the SQL twin of this function.
//
// Why every ASCII special and not only the structural ones: a reporter's
// words are rendered by TipTap, cmark-gfm, comrak and commonmark-java, which
// agree on backslash escapes (any ASCII punctuation) but not on every corner
// of inline parsing. Escaping broadly keeps the four outputs identical.

const SPECIALS = /([\\`*_[\]<>#~|@&])/g
const DOMAIN_DOT = /\.(?=[A-Za-z0-9])/g
// A tab counts as a full indent (CommonMark tab stops).
const INDENTED = /^(?:\t| {4,})[ \t]*/
const LIST_MARKER = /^([ \t]*)([-+])/
const ORDERED_MARKER = /^([ \t]*\d+)([.)])/
const SETEXT = /^([ \t]*)(=)/

export function escapeReporterText(text: string): string {
  const normalized = text.replace(/\r\n?/g, `\n`)
  return normalized
    .split(`\n`)
    .map((line) => {
      let out = line.replace(SPECIALS, `\\$1`).replace(DOMAIN_DOT, `\\.`)
      out = out.replace(INDENTED, ``)
      out = out.replace(LIST_MARKER, `$1\\$2`)
      out = out.replace(ORDERED_MARKER, `$1\\$2`)
      out = out.replace(SETEXT, `$1\\$2`)
      return out
    })
    .join(`\n`)
}

export const REPORTER_TITLE_MAX = 120
export const REPORTER_TITLE_FALLBACK: string = fixture.title.fallback

/** The issue title a reporter message yields: its first non-empty line,
 *  escaped, whitespace collapsed, cut with an ellipsis. */
export function titleFromReporterMessage(message: string): string {
  const firstLine =
    message
      .replace(/\r\n?/g, `\n`)
      .split(`\n`)
      .map((line) => line.trim().replace(/\s+/g, ` `))
      .find((line) => line.length > 0) ?? ``
  if (!firstLine) return REPORTER_TITLE_FALLBACK
  const escaped = escapeReporterText(firstLine)
  return escaped.length > REPORTER_TITLE_MAX
    ? `${escaped.slice(0, REPORTER_TITLE_MAX - 1).trimEnd()}…`
    : escaped
}
