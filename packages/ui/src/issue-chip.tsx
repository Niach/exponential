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

/** SLOP-15: `md` = glyph · identifier · title; `sm` = the SHORT chip, glyph ·
 *  identifier alone (the title rides in the tooltip), for the surfaces that
 *  stand for several issues at once — the hover graph's nodes, the phone
 *  header's badge — where a title per chip is what made them sprawl. */
export type IssueChipSize = `md` | `sm`

export function IssueChip({
  identifier,
  title,
  status,
  size = `md`,
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
  /** `sm` drops the title (SLOP-15); the box and the glyph stay the same. */
  size?: IssueChipSize
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
      {size === `md` && (
        <span className="min-w-0 truncate text-[0.8125rem] font-medium text-foreground">
          {title}
        </span>
      )}
    </>
  )

  return (
    <ChipBox
      slot="issue-chip"
      data-size={size}
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

// EXP-1033 — a BATCHED issue as one chip. The workflow graph draws a compound
// node (a parent run with its sub-issues on one branch) with it, and every
// other surface that stands for several issues at once (the work header's
// stack, a batch run's subject) takes the same picture: the front chip with
// two ghosts of its box peeking out behind it, offset top-right in small
// steps. It wraps a chip rather than being one, so the front chip keeps its
// own link, its ✕ and its hover preview.
//
// SLOP-15: the ghosts are OUTLINES (`issue-chip-ghost`, styles.css) — no
// fill, the far one fainter, 2px steps instead of 3. Filled boxes stepped 6px
// out read as three chips jostling in the header; a faint edge or two behind
// the chip reads as one chip with depth, which is all the stack has to say.

/** How far each ghost sits behind the front chip, farthest first. */
const STACK_OFFSETS = [4, 2] as const

export function IssueChipStack({
  children,
  count,
  className,
  testId,
}: {
  /** The front chip — an `IssueChip`, or anything chip-shaped. */
  children: ReactNode
  /** How many issues ride behind the front one. Omitted = no number (the
   *  workflow graph's chip already reads `EXP-14 +3`). */
  count?: number
  className?: string
  testId?: string
}): ReactNode {
  // The ghosts are absolutely positioned, so they wrap the CHIP alone: were
  // they sized off the whole stack they would reach under the `+N` and paint
  // over it (EXP-1097). The count sits BESIDE the stack, clear of the 6px the
  // back ghost peeks out on the right.
  return (
    <span
      data-slot="issue-chip-stack"
      data-testid={testId}
      className={cn(`inline-flex min-w-0 max-w-full items-center`, className)}
    >
      <span className="relative inline-flex min-w-0 flex-1 self-stretch">
        {STACK_OFFSETS.map((offset, index) => (
          <span
            key={offset}
            aria-hidden
            data-slot="issue-chip-stack-ghost"
            data-depth={STACK_OFFSETS.length - index}
            className="issue-chip-ghost pointer-events-none absolute"
            style={{ top: -offset, right: -offset, left: offset, bottom: offset }}
          />
        ))}
        <span className="relative flex min-w-0 flex-1">{children}</span>
      </span>
      {count !== undefined && count > 0 && (
        <span
          data-slot="issue-chip-stack-count"
          className="relative shrink-0 pl-1.5 font-mono text-xs text-muted-foreground"
        >
          {`+${count}`}
        </span>
      )}
    </span>
  )
}
