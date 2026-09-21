import { useState, type ReactElement, type ReactNode } from "react"
import { Button } from "./button"
import { cn } from "./cn"

// EXP-920 — the entity PREVIEW card, presentational half: what a tool-row
// chip's hover card (desktop pointer) or tap sheet (phones) shows for a
// board, an action, a comment, a run, a label… One chrome for every kind,
// the issue preview's (`w-80 p-3` inside the hover surface, EXP-760):
//
//   glyph · EYEBROW (the kind noun, or an identifier)
//   Title
//   subtitle
//   body (multi-line, clamped, "Show more" unfolds it)
//   facts (a wrapping row of pills)
//   rows (a list: a board's open issues, a list chip's members) + "+N more"
//   footer
//
// Everything is handed IN, already resolved; the app's
// `components/entity-preview/` reads the synced rows and picks per kind.
// Desktop `entity_preview.rs`, iOS `EntityPreviewCard.swift` and Android
// `EntityPreviewCard.kt` draw the same ladder.

/** What a row hands a caller-supplied link element: the row's layout classes
 *  and its parts as children. */
export interface EntityPreviewRowLinkProps {
  className: string
  children: ReactNode
}

export interface EntityPreviewRow {
  key: string
  /** The row's glyph — a status glyph for an issue, a kind concept else. */
  icon: ReactNode
  /** A mono muted identifier leading the text (`EXP-42`). */
  identifier?: string | null
  primary: string
  /** Muted text after the primary (an email, a device). */
  secondary?: string | null
  onClick?: () => void
  /** A real link around the row (wins over `onClick`). */
  link?: (props: EntityPreviewRowLinkProps) => ReactElement
}

const ROW_CLASS = `flex min-w-0 items-center gap-2 rounded-md px-1.5 py-1 text-xs`
const ROW_TARGET_CLASS = `cursor-pointer text-left outline-none hover:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50`

function rowBody(row: EntityPreviewRow): ReactNode {
  return (
    <>
      <span className="flex size-3.5 shrink-0 items-center justify-center [&>svg]:size-3.5">
        {row.icon}
      </span>
      {row.identifier && (
        <span className="shrink-0 font-mono text-muted-foreground">
          {row.identifier}
        </span>
      )}
      <span className="min-w-0 truncate text-foreground">{row.primary}</span>
      {row.secondary && (
        <span className="min-w-0 truncate text-muted-foreground">
          {row.secondary}
        </span>
      )}
    </>
  )
}

/** The card's list: each row a target when it has one, and the muted
 *  "+N more" line under it when the answer carried more than the card
 *  lists. Exported alone for the surfaces that list without the header. */
export function EntityPreviewRows({
  rows,
  more,
  className,
}: {
  rows: readonly EntityPreviewRow[]
  more?: string | null
  className?: string
}) {
  return (
    <div
      data-slot="entity-preview-rows"
      className={cn(`-mx-1.5 flex flex-col`, className)}
    >
      {rows.map((row) =>
        row.link ? (
          <span key={row.key} data-slot="entity-preview-row" className="flex min-w-0">
            {row.link({
              className: cn(ROW_CLASS, ROW_TARGET_CLASS, `w-full`),
              children: rowBody(row),
            })}
          </span>
        ) : row.onClick ? (
          <button
            key={row.key}
            type="button"
            data-slot="entity-preview-row"
            onClick={row.onClick}
            className={cn(ROW_CLASS, ROW_TARGET_CLASS, `w-full`)}
          >
            {rowBody(row)}
          </button>
        ) : (
          <span
            key={row.key}
            data-slot="entity-preview-row"
            className={ROW_CLASS}
          >
            {rowBody(row)}
          </span>
        )
      )}
      {more && (
        <span className="px-1.5 py-1 text-xs text-muted-foreground">{more}</span>
      )}
    </div>
  )
}

/** A body longer than this folds behind "Show more" (matches the transcript's
 *  own fold, `line-clamp-4`). */
const BODY_CLAMP_CHARS = 280
const BODY_CLAMP_LINES = 4

export function EntityPreviewCard({
  icon,
  eyebrow,
  title,
  subtitle,
  body,
  facts,
  rows,
  more,
  footer,
  className,
  testId,
}: {
  /** The header glyph — the kind's concept, an avatar, a board glyph. */
  icon: ReactNode
  /** The kind noun (`Board`, `Comment`) or an identifier (`EXP-42`). */
  eyebrow: string
  title: string
  subtitle?: string | null
  /** Multi-line text (a comment's body, an action's description), clamped
   *  with a "Show more" toggle when long. */
  body?: string | null
  /** A wrapping row of pills (status · repo · N inputs). */
  facts?: ReactNode
  rows?: readonly EntityPreviewRow[]
  /** The line under the rows (`+3 more`). */
  more?: string | null
  footer?: ReactNode
  className?: string
  testId?: string
}) {
  const [expanded, setExpanded] = useState(false)
  const text = body?.trim() ?? ``
  const clampable =
    text.length > BODY_CLAMP_CHARS || text.split(`\n`).length > BODY_CLAMP_LINES

  return (
    <div
      data-slot="entity-preview-card"
      data-testid={testId}
      className={cn(`flex min-w-0 flex-col gap-2`, className)}
    >
      <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
        <span className="flex size-3.5 shrink-0 items-center justify-center [&>svg]:size-3.5">
          {icon}
        </span>
        <span className="min-w-0 truncate">{eyebrow}</span>
      </div>

      <p className="line-clamp-2 text-sm font-medium text-foreground">{title}</p>

      {subtitle && (
        <p className="line-clamp-2 text-xs text-muted-foreground">{subtitle}</p>
      )}

      {text && (
        <div className="flex min-w-0 flex-col items-start">
          <p
            className={cn(
              `whitespace-pre-wrap break-words text-xs text-foreground/85`,
              clampable && !expanded && `line-clamp-4`
            )}
          >
            {text}
          </p>
          {clampable && (
            <Button
              variant="text"
              size="inline"
              className="mt-1 font-medium"
              onClick={() => setExpanded((value) => !value)}
            >
              {expanded ? `Show less` : `Show more`}
            </Button>
          )}
        </div>
      )}

      {facts && (
        <div className="flex flex-wrap items-center gap-1.5">{facts}</div>
      )}

      {rows && rows.length > 0 ? (
        <EntityPreviewRows rows={rows} more={more} />
      ) : (
        more && <span className="text-xs text-muted-foreground">{more}</span>
      )}

      {footer}
    </div>
  )
}
