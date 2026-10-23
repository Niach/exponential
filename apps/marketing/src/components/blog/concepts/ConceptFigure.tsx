/* A drafted screen drawn in HTML and CSS with CSS-only motion (a few
   cursors move, steps advance, a surface streams in). The prerender ships
   the same static markup, the motion runs from the stylesheet alone, and
   `prefers-reduced-motion` holds every figure on one representative frame.
   `still` names the webp scripts/blog-stills.ts photographs FROM this figure
   (public/blog/<still>.webp): the cover, the index card, feed readers. */
import type { ReactNode } from "react"

export type CursorTone = `agent` | `agent-2` | `you`

/** A pointer arrow with its owner's name: the viewer's drawing of a seat,
 *  never an operating-system cursor. */
export function Cursor({ tone, label, className }: { tone: CursorTone; label: string; className?: string }) {
  return (
    <span className={`bc-cursor bc-cursor--${tone}${className ? ` ${className}` : ``}`} aria-hidden>
      <svg viewBox="0 0 18 22" width="16" height="20">
        <path d="M2.5 1.5 2.5 17.2 6.6 13.4 9.4 20.2 12.2 19 9.5 12.3 15.3 12.3Z" />
      </svg>
      <em>{label}</em>
    </span>
  )
}

/* The "concept" stamp leads the caption rather than floating over the
   frame: these screens fill their top edge (a title row, a panel). */
export function ConceptFigure({ id, still, caption, children }: { id: string; still: string; caption: ReactNode; children: ReactNode }) {
  return (
    <figure className="docs-shot blog-figure blog-concept-figure" data-concept={id} data-still={still}>
      <div className="blog-figure-frame blog-concept-frame">{children}</div>
      <figcaption className="docs-embed-caption">
        <span className="blog-concept blog-concept--inline">concept</span> {caption}
      </figcaption>
    </figure>
  )
}
