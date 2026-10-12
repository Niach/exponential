import { useLayoutEffect, useRef, useState, type ReactNode } from "react"

import { Button } from "./button"
import { cn } from "./cn"
import { ExponentialLogo } from "./exponential-logo"
import { Spinner } from "./feedback"

// EXP-1241 (EXP-1259 D14): an `exponential_comments_create` call in a run's
// transcript renders the REAL comment, in the issue timeline's comment look:
// "Commented on <issue chip>" over a card holding the author row (the agent
// mark, the name, "via MCP · time") and the markdown body clamped to three
// lines behind "Show more". Clicking the card opens the issue scrolled to
// that comment (`?comment=<id>`). The host renders the chip, the mark and the
// body; this only lays them out.

export const COMMENT_CALL_COPY = {
  done: `Commented on`,
  running: `Commenting on`,
  showMore: `Show more`,
  showLess: `Show less`,
} as const

/** Three body lines at the chat markdown's 1.5 line height. */
const CLAMP_CLASS = `max-h-[4.5em] overflow-hidden`
// A clamp cuts mid-line when the body holds a heading or a list; the fade
// makes the cut read as "there is more" instead of a clipped line.
const CLAMP_FADE_CLASS = `[mask-image:linear-gradient(to_bottom,black_55%,transparent)]`

export function CommentCallCard({
  issue,
  author,
  mark,
  caption,
  body,
  running = false,
  onOpen,
  className,
}: {
  /** The issue chip the comment sits on (null = not resolvable here). */
  issue: ReactNode
  /** The comment's author name (the key owner the agent posted as). */
  author: string | null
  /** The author row's leading mark (the agent's run mark). */
  mark?: ReactNode
  /** The muted caption after the name: "via MCP · 21:02". */
  caption?: string | null
  /** The rendered markdown body; null while the call runs. */
  body: ReactNode
  running?: boolean
  /** Open the issue scrolled to the comment; absent = inert. */
  onOpen?: () => void
  className?: string
}) {
  const bodyRef = useRef<HTMLDivElement | null>(null)
  const [overflows, setOverflows] = useState(false)
  const [expanded, setExpanded] = useState(false)
  useLayoutEffect(() => {
    const node = bodyRef.current
    if (!node || expanded) return
    setOverflows(node.scrollHeight > node.clientHeight + 1)
  }, [body, expanded])

  return (
    <div
      data-testid="comment-call-card"
      className={cn(
        `min-w-0 overflow-clip rounded-lg border border-glass-stroke-card bg-glass-row p-3 text-xs`,
        className
      )}
    >
      <div className="flex min-w-0 items-center gap-2">
        <ExponentialLogo
          variant="light"
          size={14}
          className="size-3.5 shrink-0 text-muted-foreground"
        />
        <span className="shrink-0 font-medium">
          {running ? COMMENT_CALL_COPY.running : COMMENT_CALL_COPY.done}
        </span>
        {running && <Spinner size={12} label={COMMENT_CALL_COPY.running} />}
        {issue && <span className="min-w-0 truncate">{issue}</span>}
      </div>
      {body !== null && (
        <div
          role={onOpen ? `link` : undefined}
          tabIndex={onOpen ? 0 : undefined}
          data-slot="comment-call-body"
          onClick={onOpen}
          onKeyDown={
            onOpen
              ? (event) => {
                  if (event.key === `Enter`) onOpen()
                }
              : undefined
          }
          className={cn(
            `mt-2.5 min-w-0 rounded-md border border-glass-stroke bg-glass-row px-3 pt-2.5 pb-3`,
            onOpen && `cursor-pointer hover:bg-glass-active/50`
          )}
        >
          <div className="flex min-w-0 items-center gap-2">
            {mark}
            {author && (
              <span className="min-w-0 truncate text-sm font-medium text-foreground">
                {author}
              </span>
            )}
            {caption && (
              <span className="shrink-0 text-xs text-muted-foreground">
                {caption}
              </span>
            )}
          </div>
          <div
            ref={bodyRef}
            className={cn(
              `mt-1 min-w-0 text-sm text-foreground`,
              !expanded && CLAMP_CLASS,
              !expanded && overflows && CLAMP_FADE_CLASS
            )}
          >
            {body}
          </div>
          {(overflows || expanded) && (
            <Button
              type="button"
              variant="text"
              size="inline"
              className="mt-1 text-xs text-muted-foreground hover:text-foreground"
              onClick={(event) => {
                event.stopPropagation()
                setExpanded((value) => !value)
              }}
            >
              {expanded ? COMMENT_CALL_COPY.showLess : COMMENT_CALL_COPY.showMore}
            </Button>
          )}
        </div>
      )}
    </div>
  )
}
