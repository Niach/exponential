import { FeedText } from "@/components/feed-text"

// EXP-1293: a thread's text (an agent's reply, a person's message, a plan, a
// Guide section) = genUI's Markdown in the chat rhythm (#VAPP-258,
// `ReadMarkdown`): hard breaks, links into this instance routed in-app
// (EXP-1188), bare identifiers chipped the way agents write them (EXP-760).

export function ThreadMarkdown({
  text,
  ariaLabel = `Message`,
}: {
  text: string
  ariaLabel?: string
}) {
  return <FeedText text={text} ariaLabel={ariaLabel} hardBreaks />
}

/** The `renderText` slot every thread part takes. */
export function renderThreadText(text: string) {
  return <ThreadMarkdown text={text} />
}
