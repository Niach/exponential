// SLOP-4: pure text helpers for the reporter page — no DB import, shared by
// the /api/support/thread route and the page's own display path.
import reporterReplyFixture from "@exp/domain-contract/fixtures/reporter-reply.json"
import { replaceMentionTokens } from "@/lib/mention-refs"

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

// NEW-85 (reporter-reply.json `reporterView`): member identities never leave
// for the reporter. Every `@<email>` mention token in member-written text the
// reporter receives reads the fixture's `mentionText` ("@Support", beside the
// "Support" author label), except a mention of the reporter's own address.
// Applied server-side at every reporter egress: /api/support/thread, the
// reply email, the MCP reporter view.
export function reporterFacingText(
  text: string,
  reporterEmail: string | null | undefined
): string {
  const own = reporterEmail?.trim().toLowerCase() || null
  return replaceMentionTokens(text, (email) =>
    email === own ? null : reporterReplyFixture.reporterView.mentionText
  )
}

/** The author label of a team reply the reporter reads. */
export const REPORTER_SUPPORT_LABEL = reporterReplyFixture.reporterView.supportLabel
