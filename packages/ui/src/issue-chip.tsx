import type { MouseEvent, ReactElement, ReactNode } from "react"
import { conceptIcon } from "./icons.generated"
import { StatusGlyph, type StatusGlyphProps } from "./status-glyph"
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

const UiCloseIcon = conceptIcon(`ui-close`)

/** 0.875em of the chip's own 0.75rem — the exact glyph box the editor
 *  decoration paints with its ::before mask. */
const CHIP_GLYPH_CLASS = `!h-[0.875em] !w-[0.875em] shrink-0`

/** The open affordance's box — the same whether it is a button or a link. */
const CHIP_OPEN_CLASS = `flex min-w-0 cursor-pointer items-center gap-1 rounded-[4px] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`

export function ChipRemoveButton({
  label,
  disabled,
  testId,
  onRemove,
}: {
  /** The ✕'s accessible name — also its hover title. */
  label: string
  disabled?: boolean
  testId?: string
  onRemove: () => void
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      data-testid={testId}
      disabled={disabled}
      onClick={(event: MouseEvent) => {
        event.stopPropagation()
        onRemove()
      }}
      className="flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-[4px] text-muted-foreground outline-none hover:bg-glass-active hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50"
    >
      <UiCloseIcon className="size-3" />
    </button>
  )
}

/** What the chip hands a caller-supplied link element (EXP-887): the body's
 *  layout classes, the accessible name and the three parts as children. */
export interface IssueChipLinkProps {
  className: string
  "aria-label": string
  children: ReactNode
}

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
}): ReactNode {
  const tooltip = `${identifier} · ${title}`
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
    <span
      data-slot="issue-chip"
      data-testid={testId}
      data-removable={onRemove ? `true` : undefined}
      title={tooltip}
      // `issue-chip` is the shared box (styles.css); everything here is
      // LAYOUT. `align-middle`: the chip flows inline in prose, and an
      // inline-flex box would otherwise sit on the text baseline.
      className={cn(
        `issue-chip inline-flex max-w-[18rem] items-center gap-1 align-middle text-xs`,
        className
      )}
    >
      {link ? (
        link({
          className: CHIP_OPEN_CLASS,
          "aria-label": `Open ${identifier}`,
          children: body,
        })
      ) : onClick ? (
        <button
          type="button"
          onClick={onClick}
          aria-label={`Open ${identifier}`}
          className={CHIP_OPEN_CLASS}
        >
          {body}
        </button>
      ) : (
        body
      )}
      {onRemove && (
        <ChipRemoveButton
          label={removeLabel ?? `Remove ${identifier}`}
          disabled={removeDisabled}
          testId={removeTestId}
          onRemove={onRemove}
        />
      )}
    </span>
  )
}
