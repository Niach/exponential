import { ReadMarkdown } from "@/components/read-markdown"

// The run view's and every chat's text (EXP-440, #EXP-1380 chats.json):
// genUI's Markdown through the app's host (#VAPP-258, `ReadMarkdown`): agent
// prose with its bare identifiers chipped (EXP-760), bare URLs linked and
// in-app links chipped (#EXP-1674), chat density.

export { useChippedProse } from "@/components/read-markdown"

export function FeedText({
  text,
  ariaLabel,
  hardBreaks,
  streaming,
}: {
  text: string
  ariaLabel: string
  /** Chat text is line-broken by hand — a single newline is a real break. */
  hardBreaks?: boolean
  /** The reply is still streaming in. */
  streaming?: boolean
}) {
  return <ReadMarkdown text={text} ariaLabel={ariaLabel} hardBreaks={hardBreaks} streaming={streaming} agentProse />
}
