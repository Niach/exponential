import type { ReactElement, ReactNode } from "react"
import { StatusGlyph, type StatusGlyphProps } from "./status-glyph"
import {
  CHIP_GLYPH_CLASS,
  ChipBox,
  type ChipHostProps,
  type EntityChipLinkProps,
} from "./entity-chip"
import { cn } from "./cn"

// EXP-885 — THE issue chip, presentational half. ONE box for every surface
// that names an issue as a chip: the steering feed's prose (`IssueRefText`),
// the MCP tool result preview, the timeline's relation events and the launch
// composer's subject chips. Inside a markdown editor the same chip is a
// ProseMirror DECORATION instead (the app's `lib/issue-ref-extension.ts`),
// because the document text has to stay the bare `#IDENT` token — the two
// share their box through the `issue-chip` class in this package's styles.css,
// and styles.test.ts locks that pair.
//
// The style, byte-for-byte across all four clients: a small rounded RECT
// (6px, never a capsule), `--border` hairline over `--accent`, then status
// glyph · mono muted identifier · sans/500 foreground title, truncated.
//
// A chip renders only for an issue the caller already resolved; an unresolved
// token stays prose. `onClick` opens it, `onRemove` adds the trailing ✕ that
// composers need — the ✕ is the ONLY part of a chip that removes it. The
// status is passed IN (already resolved against the team's rows) so this file
// stays free of live queries; the app's `components/issue-chip.tsx` is the
// binding that resolves it and adds the hover preview.
//
// EXP-920: the box, the open affordance and the ✕ live in `ChipBox`
// (./entity-chip.tsx), shared with the entity chip every other kind draws in
// a tool row — so an issue chip and a board chip cannot drift apart.

/** What the chip hands a caller-supplied link element (EXP-887): the body's
 *  layout classes, the accessible name and the three parts as children. */
export type IssueChipLinkProps = EntityChipLinkProps

export function IssueChip({
  identifier,
  title,
  status,
  onClick,
  link,
  onRemove,
  removeLabel,
  removeDisabled,
  className,
  testId,
  removeTestId,
  ...host
}: {
  identifier: string
  title: string
  /** The already-resolved status glyph (icon + its token class or hex). */
  status: StatusGlyphProps
  /** Open the issue. Omitted = an inert chip (no target, no pointer). */
  onClick?: () => void
  /** Open the issue as a real LINK (⌘-click, middle-click, copy address,
   *  router preloading): render the caller's anchor element around the body.
   *  Wins over `onClick`. */
  link?: (props: IssueChipLinkProps) => ReactElement
  /** Composers only: render the trailing ✕. */
  onRemove?: () => void
  /** The ✕'s accessible name; defaults to `Remove <IDENT>`. */
  removeLabel?: string
  /** Keep the ✕ in place but inert (a submitting composer). */
  removeDisabled?: boolean
  className?: string
  testId?: string
  removeTestId?: string
} & ChipHostProps): ReactNode {
  const body = (
    <>
      <StatusGlyph
        {...status}
        className={cn(CHIP_GLYPH_CLASS, status.className)}
      />
      <span className="shrink-0 font-mono text-muted-foreground">
        {identifier}
      </span>
      <span className="min-w-0 truncate text-[0.8125rem] font-medium text-foreground">
        {title}
      </span>
    </>
  )

  return (
    <ChipBox
      slot="issue-chip"
      body={body}
      openLabel={`Open ${identifier}`}
      tooltip={`${identifier} · ${title}`}
      onClick={onClick}
      link={link}
      onRemove={onRemove}
      removeLabel={removeLabel ?? `Remove ${identifier}`}
      removeDisabled={removeDisabled}
      className={className}
      testId={testId}
      removeTestId={removeTestId}
      {...host}
    />
  )
}
