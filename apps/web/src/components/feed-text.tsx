import { Fragment, useEffect, useRef, useState } from "react"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { IssueChip } from "@/components/issue-chip"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { useAppLinkClick } from "@/hooks/use-app-link-click"
import { looksLikeMarkdown } from "@/lib/agent-feed"
import { splitIssueRefs } from "@/lib/issue-refs"
import { linkSegments } from "@/lib/linkify"

// The run view's cheap text path (EXP-440), shared with every chat
// (#EXP-1380 chats.json PERF): plain prose stays plain linkified text with
// issue chips; only text that carries markdown mounts the read-only TipTap
// renderer.

const noop = () => {}

/** The shared read-only markdown renderer — the same TipTap pipeline the issue
 *  and comment bodies use, in its compact `chat` presentation, so a feed bubble
 *  draws headings, lists, code and IMAGES exactly like the rest of the product
 *  (EXP-440). `linkify` is on because agent text carries bare URLs. */
export function FeedMarkdown({
  text,
  ariaLabel,
  hardBreaks,
}: {
  text: string
  ariaLabel: string
  /** Chat text is line-broken by hand — a single newline is a real break. */
  hardBreaks?: boolean
}) {
  const onLinkClick = useAppLinkClick()
  return (
    // EXP-1188: links to this instance route in-app, sources open a tab.
    <div onClickCapture={onLinkClick}>
      <MarkdownEditor
        markdown={text}
        editable={false}
        onChange={noop}
        appearance="chat"
        linkify
        hardBreaks={hardBreaks}
        ariaLabel={ariaLabel}
        // EXP-760: agents narrate BARE identifiers ("landed EXP-758"), so the
        // steering feed — and only it — chips those too. Descriptions and
        // comments keep the `#IDENT` contract.
        bareIssueRefs
      />
    </div>
  )
}

/** Feed text: markdown when it carries any (EXP-440), else the plain
 *  linkified rendering — most narration is one prose line, and spinning up a
 *  TipTap instance per line would be pure overhead. */
export function FeedText({
  text,
  ariaLabel,
  hardBreaks,
  deferred,
}: {
  text: string
  ariaLabel: string
  hardBreaks?: boolean
  /** #EXP-1380: mount the markdown renderer only near the viewport. */
  deferred?: boolean
}) {
  const onLinkClick = useAppLinkClick()
  if (looksLikeMarkdown(text)) {
    return deferred ? (
      <DeferredFeedMarkdown text={text} ariaLabel={ariaLabel} hardBreaks={hardBreaks} />
    ) : (
      <FeedMarkdown text={text} ariaLabel={ariaLabel} hardBreaks={hardBreaks} />
    )
  }
  return (
    <div className="whitespace-pre-wrap break-words" onClickCapture={onLinkClick}>
      <IssueRefText text={text} />
    </div>
  )
}

/** How far outside the viewport a deferred row already renders. */
const DEFER_MARGIN = `1200px 0px`

/** #EXP-1380 (chats.json PERF): a chat's markdown rows mount the renderer
 *  once they come within `DEFER_MARGIN` of the viewport (a long chat opens
 *  at its tail: a handful of editors, not a window's worth); until then the
 *  text paints as plain prose. Once mounted it stays. */
function DeferredFeedMarkdown({
  text,
  ariaLabel,
  hardBreaks,
}: {
  text: string
  ariaLabel: string
  hardBreaks?: boolean
}) {
  const ref = useRef<HTMLDivElement | null>(null)
  const [near, setNear] = useState(() => typeof IntersectionObserver === `undefined`)
  useEffect(() => {
    if (near) return
    const node = ref.current
    if (!node) return
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) setNear(true)
      },
      { rootMargin: DEFER_MARGIN }
    )
    observer.observe(node)
    return () => observer.disconnect()
  }, [near])
  if (near) return <FeedMarkdown text={text} ariaLabel={ariaLabel} hardBreaks={hardBreaks} />
  return (
    <div ref={ref} className="whitespace-pre-wrap break-words" aria-label={ariaLabel}>
      {text}
    </div>
  )
}

/** Linkified prose whose issue identifiers — `#EXP-758` AND the bare
 *  `EXP-758` agents actually write — render as chips (EXP-760). Only a
 *  RESOLVED, same-team issue chips; everything else stays prose and goes
 *  through the URL linkifier, exactly as `MarkerText` splits around its image
 *  markers. */
function IssueRefText({ text }: { text: string }) {
  const issueRefs = useIssueRefs()
  return (
    <>
      {splitIssueRefs(text, { bare: true }).map((segment, i) => {
        const resolved = segment.identifier
          ? (issueRefs?.resolve(segment.identifier) ?? null)
          : null
        if (resolved)
          return (
            <IssueChip
              key={i}
              issue={resolved}
              onClick={() => issueRefs?.open(resolved.identifier)}
            />
          )
        return linkSegments(segment.text).map((part, j) =>
          part.href ? (
            // break-all: the EXP-430 sign-in URL has no break points.
            <a
              key={`${i}-${j}`}
              href={part.href}
              target="_blank"
              rel="noreferrer"
              className="break-all text-primary underline underline-offset-2 hover:opacity-80"
            >
              {part.text}
            </a>
          ) : (
            <Fragment key={`${i}-${j}`}>{part.text}</Fragment>
          ),
        )
      })}
    </>
  )
}
