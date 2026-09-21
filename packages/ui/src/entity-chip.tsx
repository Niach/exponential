import type { ComponentProps, MouseEvent, ReactElement, ReactNode } from "react"
import {
  entityChipDetail,
  entityChipLabel,
  entityRefIcon,
  type EntityRefLike,
} from "@exp/domain-contract/entity-preview"
import type { IconConcept } from "@exp/icons"
import { conceptIcon } from "./icons.generated"
import { cn } from "./cn"

// EXP-920 — THE entity chip, presentational half. An Exponential MCP tool's
// settled transcript row names what its answer touched (`preview.refs`,
// contract `entityRefKind`), and every client draws ONE chip per ref: a glyph
// and a short label in the very box the issue chip owns (`issue-chip` in this
// package's styles.css — a small rounded RECT, hairline over `--accent`).
// `IssueChip` (./issue-chip.tsx) renders THROUGH `ChipBox` below with its
// three parts, so an issue chip and a board chip on the same row cannot
// disagree by a pixel.
//
// The glyph is handed IN: an issue passes its resolved `StatusGlyph`, every
// other kind the concept `entityRefIcon` names (`entityChipGlyph`); a chip
// stays free of live queries either way. `muted` is the unsynced row — the
// same box, greyed, with no target (the app's `EntityRefChip` decides).
// The label and its optional detail are the contract's (`entityChipLabel`,
// `entityChipDetail`), fixture-locked ×4.

const UiCloseIcon = conceptIcon(`ui-close`)

/** 0.875em of the chip's own 0.75rem — the exact glyph box the editor
 *  decoration paints with its ::before mask. Every glyph a chip takes wears
 *  this class, whatever kind it draws. */
export const CHIP_GLYPH_CLASS = `!h-[0.875em] !w-[0.875em] shrink-0`

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

/** What a chip hands a caller-supplied link element (EXP-887): the body's
 *  layout classes, the accessible name and the parts as children. */
export interface EntityChipLinkProps {
  className: string
  "aria-label": string
  children: ReactNode
}

/** What a Radix `asChild` trigger (a hover card, a sheet) merges onto the
 *  chip's root at runtime — the pointer handlers, `ref`, aria state. A chip
 *  forwards them so it can BE the trigger without a wrapper element. */
export type ChipHostProps = Omit<
  ComponentProps<`span`>,
  `children` | `className` | `title` | `onClick`
>

/** The shared box: `issue-chip` (styles.css) is the paint, everything here is
 *  LAYOUT. `align-middle`: the chip flows inline in prose, and an inline-flex
 *  box would otherwise sit on the text baseline. The body becomes a link
 *  (`link` wins), a button (`onClick`) or stays inert. */
export function ChipBox({
  slot,
  body,
  openLabel,
  tooltip,
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
  /** The `data-slot` a test or a stylesheet keys on. */
  slot: string
  body: ReactNode
  /** The open affordance's accessible name (`Open EXP-42`). */
  openLabel: string
  tooltip?: string
  onClick?: () => void
  link?: (props: EntityChipLinkProps) => ReactElement
  onRemove?: () => void
  removeLabel?: string
  removeDisabled?: boolean
  className?: string
  testId?: string
  removeTestId?: string
} & ChipHostProps): ReactNode {
  return (
    <span
      {...host}
      data-slot={slot}
      data-testid={testId}
      data-removable={onRemove ? `true` : undefined}
      title={tooltip}
      className={cn(
        `issue-chip inline-flex max-w-[18rem] items-center gap-1 align-middle text-xs`,
        className
      )}
    >
      {link ? (
        link({
          className: CHIP_OPEN_CLASS,
          "aria-label": openLabel,
          children: body,
        })
      ) : onClick ? (
        <button
          type="button"
          onClick={onClick}
          aria-label={openLabel}
          className={CHIP_OPEN_CLASS}
        >
          {body}
        </button>
      ) : (
        body
      )}
      {onRemove && (
        <ChipRemoveButton
          label={removeLabel ?? `Remove`}
          disabled={removeDisabled}
          testId={removeTestId}
          onRemove={onRemove}
        />
      )}
    </span>
  )
}

/** The concept glyph a non-issue ref's chip and card header draw, already in
 *  the chip's glyph box. (An issue passes its resolved `StatusGlyph` instead
 *  once its row is synced; unsynced, this is its `ui-issue` mark.) */
export function entityChipGlyph(
  ref: EntityRefLike,
  className?: string
): ReactElement {
  const Icon = conceptIcon(entityRefIcon(ref) as IconConcept)
  return <Icon className={cn(CHIP_GLYPH_CLASS, className)} />
}

export function EntityChip({
  icon,
  label,
  detail,
  muted = false,
  onClick,
  link,
  className,
  testId,
  ...host
}: {
  /** The glyph, sized by the caller with `CHIP_GLYPH_CLASS` (a `StatusGlyph`,
   *  or `entityChipGlyph(ref)`). */
  icon: ReactNode
  /** The chip's text (`entityChipLabel`). With a `detail` it is the mono
   *  muted identifier; alone it is the foreground name. */
  label: string
  /** An issue's title beside its identifier (`entityChipDetail`). */
  detail?: string | null
  /** An unsynced row: greyed, and the caller passes no target. */
  muted?: boolean
  /** Open the entity. Omitted = an inert chip (no target, no pointer). */
  onClick?: () => void
  /** Open the entity as a real LINK (⌘-click, middle-click, copy address,
   *  router preloading): render the caller's anchor around the body. Wins
   *  over `onClick`. */
  link?: (props: EntityChipLinkProps) => ReactElement
  className?: string
  testId?: string
} & ChipHostProps): ReactNode {
  const body = (
    <>
      {icon}
      {detail ? (
        <>
          <span className="shrink-0 font-mono text-muted-foreground">
            {label}
          </span>
          <span
            className={cn(
              `min-w-0 truncate text-[0.8125rem] font-medium`,
              muted ? `text-muted-foreground` : `text-foreground`
            )}
          >
            {detail}
          </span>
        </>
      ) : (
        <span
          className={cn(
            `min-w-0 truncate text-[0.8125rem] font-medium`,
            muted ? `text-muted-foreground` : `text-foreground`
          )}
        >
          {label}
        </span>
      )}
    </>
  )
  return (
    <ChipBox
      slot="entity-chip"
      body={body}
      openLabel={`Open ${label}`}
      tooltip={detail ? `${label} · ${detail}` : label}
      onClick={onClick}
      link={link}
      className={cn(muted && `text-muted-foreground`, className)}
      testId={testId}
      {...host}
    />
  )
}

/** A ref straight to a chip: the contract's label/detail and the concept
 *  glyph. The app wraps this with a resolved status glyph, a hover card and
 *  a target; a styleguide island or an unsynced row draws it as is. */
export function EntityRefChipView({
  entityRef,
  icon,
  muted,
  onClick,
  link,
  className,
  testId,
  ...host
}: {
  entityRef: EntityRefLike
  /** Override the glyph (an issue's resolved status). */
  icon?: ReactNode
  muted?: boolean
  onClick?: () => void
  link?: (props: EntityChipLinkProps) => ReactElement
  className?: string
  testId?: string
} & ChipHostProps): ReactNode {
  return (
    <EntityChip
      icon={icon ?? entityChipGlyph(entityRef)}
      label={entityChipLabel(entityRef)}
      detail={entityChipDetail(entityRef)}
      muted={muted}
      onClick={onClick}
      link={link}
      className={className}
      testId={testId}
      {...host}
    />
  )
}
