import { Fragment, useEffect, useRef, useState } from "react"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { MentionPill } from "@exp/ui"
import { IssueChip } from "@/components/issue-chip"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { useMentions, type MentionContextValue } from "@/components/mention-provider"
import { useAppLinkClick } from "@/hooks/use-app-link-click"
import { looksLikeMarkdown } from "@/lib/agent-feed"
import { splitIssueRefs } from "@/lib/issue-refs"
import { linkSegments } from "@/lib/linkify"
import { createMentionRegExp } from "@/lib/mention-refs"

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
  // The placeholder already paints chips and mention pills (thread e4a04d22),
  // so a row never flashes a raw `@email` before its renderer mounts.
  return (
    <div ref={ref} className="whitespace-pre-wrap break-words" aria-label={ariaLabel}>
      <IssueRefText text={text} />
    </div>
  )
}

/** Linkified prose whose issue identifiers — `#EXP-758` AND the bare
 *  `EXP-758` agents actually write — render as chips (EXP-760), and whose
 *  `@<email>` mentions render as THE mention pill (thread e4a04d22, the
 *  comments' round pill). Only a RESOLVED, same-team issue chips and only a
 *  member the viewer can see pills; everything else stays prose and goes
 *  through the URL linkifier, exactly as `MarkerText` splits around its image
 *  markers. */
export function IssueRefText({ text }: { text: string }) {
  const issueRefs = useIssueRefs()
  const mentions = useMentions()
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
        return splitMentions(segment.text, mentions).map((piece, k) =>
          piece.member ? (
            <MentionPill key={`${i}-m${k}`} name={piece.member.name} email={piece.email} />
          ) : (
            <LinkedText key={`${i}-m${k}`} text={piece.text} />
          ),
        )
      })}
    </>
  )
}

type MentionPiece =
  | { text: string; email: string; member: { name: string } }
  | { text: string; email?: undefined; member?: undefined }

/** `text` cut around its RESOLVED `@<email>` mentions (an unresolved one
 *  stays in the surrounding text). No `@` = one piece, no regex run. */
export function splitMentions(
  text: string,
  mentions: Pick<MentionContextValue, `resolve`> | null,
): MentionPiece[] {
  if (!mentions || !text.includes(`@`)) return [{ text }]
  const out: MentionPiece[] = []
  let at = 0
  for (const match of text.matchAll(createMentionRegExp())) {
    const email = match[1]!
    const member = mentions.resolve(email)
    if (!member) continue
    const start = match.index!
    if (start > at) out.push({ text: text.slice(at, start) })
    out.push({ text: match[0], email, member })
    at = start + match[0].length
  }
  if (at < text.length) out.push({ text: text.slice(at) })
  return out.length > 0 ? out : [{ text }]
}

function LinkedText({ text }: { text: string }) {
  return (
    <>
      {linkSegments(text).map((part, j) =>
        part.href ? (
          // break-all: the EXP-430 sign-in URL has no break points.
          <a
            key={j}
            href={part.href}
            target="_blank"
            rel="noreferrer"
            className="break-all text-primary underline underline-offset-2 hover:opacity-80"
          >
            {part.text}
          </a>
        ) : (
          <Fragment key={j}>{part.text}</Fragment>
        ),
      )}
    </>
  )
}
