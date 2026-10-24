import { FeedText } from "@/components/feed-text"

// EXP-1293: a thread's text (an agent's reply, a person's message, a plan, a
// Guide section) = the comment renderer: read-only GFM at feed size, links
// into this instance routed in-app (EXP-1188), bare identifiers chipped the
// way agents write them (EXP-760). #EXP-1380 (chats.json PERF): through the
// run view's cheap path (`FeedText`): plain text never mounts an editor,
// markdown mounts it near the viewport only (`deferred`).

export function ThreadMarkdown({
  text,
  ariaLabel = `Message`,
}: {
  text: string
  ariaLabel?: string
}) {
  return <FeedText text={text} ariaLabel={ariaLabel} hardBreaks deferred />
}

/** The `renderText` slot every thread part takes. */
export function renderThreadText(text: string) {
  return <ThreadMarkdown text={text} />
}
