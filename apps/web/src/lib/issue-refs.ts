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

// `#` must not be glued to a word or another `#` (so `foo#MET-1` and `##MET-1`
// don't match), the identifier is `{PREFIX}-{number}` (prefixes are stored
// uppercase; matching is case-insensitive and normalized on extraction), and
// the match must end at a token boundary (so `#MET-115-2` / `#MET-115abc`
// don't half-match).
const ISSUE_REF_SOURCE = `(?<![\\w#])#([A-Za-z][A-Za-z0-9]*-\\d+)(?![\\w-])`

export function createIssueRefRegExp(): RegExp {
  return new RegExp(ISSUE_REF_SOURCE, `g`)
}

/** Unique, uppercase-normalized identifiers referenced in `text`. */
export function extractIssueRefs(text: string): string[] {
  return [
    ...new Set(
      [...text.matchAll(createIssueRefRegExp())].map((m) => m[1].toUpperCase())
    ),
  ]
}
