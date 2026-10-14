import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { useAppLinkClick } from "@/hooks/use-app-link-click"

// EXP-1293: a thread's text (an agent's reply, a person's message, a plan, a
// Guide section) = the comment renderer: read-only GFM at feed size, links
// into this instance routed in-app (EXP-1188), bare identifiers chipped the
// way agents write them (EXP-760).

const noop = () => {}

export function ThreadMarkdown({
  text,
  ariaLabel = `Message`,
}: {
  text: string
  ariaLabel?: string
}) {
  const onLinkClick = useAppLinkClick()
  return (
    <div onClickCapture={onLinkClick}>
      <MarkdownEditor
        markdown={text}
        editable={false}
        onChange={noop}
        appearance="chat"
        linkify
        hardBreaks
        ariaLabel={ariaLabel}
        bareIssueRefs
      />
    </div>
  )
}

/** The `renderText` slot every thread part takes. */
export function renderThreadText(text: string) {
  return <ThreadMarkdown text={text} />
}
