// SLOP-4: pure text helpers for the reporter page — no DB import, shared by
// the /api/support/thread route and the page's own display path.

// The stored widget description = the reporter's escaped text + the image
// embeds buildWidgetDescription appended (`![Screenshot](/api/attachments/…)`
// paragraphs). The reporter page gets the pictures as a strip (loaded
// through the token-gated attachment route), so the embeds come out of the
// text before it ships.
const IMAGE_EMBED_PARAGRAPH = /^!\[[^\]\n]*\]\([^)\n]*\)[ \t]*$/

export function stripImageEmbeds(description: string): string {
  return description
    .replace(/\r\n?/g, `\n`)
    .split(`\n`)
    .filter((line) => !IMAGE_EMBED_PARAGRAPH.test(line))
    .join(`\n`)
    .replace(/\n{3,}/g, `\n\n`)
    .trim()
}

// The reporter wrote plain text; the server stored it as backslash-escaped
// GFM (lib/reporter-text.ts) so members' clients render it literally. The
// reporter's own page is plain text again: drop the escapes.
export function unescapeReporterText(text: string): string {
  return text.replace(/\\([\\`*_[\]<>#~|@&.+\-=)])/g, `$1`)
}
