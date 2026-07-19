// Mentions are written as `@<email>` in issue descriptions and comment bodies
// — typeable by hand and inserted by the editors' @-autocomplete. Like
// `#IDENTIFIER` issue references (lib/issue-refs.ts), the token is the single
// interchange form across all clients: it round-trips as plain GFM text, so
// there is zero schema impact. Clients render a token as a name pill only when
// the email resolves to a workspace member they can actually see; unresolved
// tokens stay plain text.
//
// This module is client-safe (no server imports) so the same regex drives the
// TipTap pill decorations, the editor autocomplete, and the server-side
// resolver in lib/integrations/mentions.ts.

// The captured group is the bare email after the leading `@`.
const MENTION_SOURCE = `@([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,})`

export function createMentionRegExp(): RegExp {
  return new RegExp(MENTION_SOURCE, `g`)
}

/** Unique, lowercase-normalized emails mentioned in `text`. */
export function extractMentionEmails(text: string): string[] {
  return [
    ...new Set(
      [...text.matchAll(createMentionRegExp())].map((m) => m[1].toLowerCase())
    ),
  ]
}

/**
 * Replace each `@email` mention token with whatever `replace` returns for the
 * lowercase-normalized email; a `null` return keeps the token verbatim. Used
 * to swap mentions of departed members for the anonymized
 * "Member XXXX" handle before the text leaves the server.
 */
export function replaceMentionTokens(
  text: string,
  replace: (email: string) => string | null
): string {
  return text.replace(createMentionRegExp(), (token, email: string) => {
    return replace(email.toLowerCase()) ?? token
  })
}
