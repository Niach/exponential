import type { ComponentProps, ReactNode, Ref } from "react"
import { cn } from "./cn"
import { stripTabClass } from "./entity-chip"
import { conceptIcon } from "./icons.generated"

// EXP-1334: THE strip chip — ONE tab component for every strip of chips: the
// thread header's issue + session chips (`ThreadTabs`) and the work tabs
// strip above the content card (`work-tabs-strip.tsx`). The box is THE strip
// tab box (`stripTabClass`: h-8, text-sm, rounded-md, a hairline while it is
// a chip at rest, the glass wash while active); inside: the lead glyph · the
// mono identifier · the label (truncating) · `› <open one>` (a folded chip
// whose nested item is open) · the chevron-down (a chip that opens its
// subtree on a click), then an optional trailing control (the work tab's ×)
// OUTSIDE the open button.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)

/** The box's own props: DOM handlers + `data-*` (a `menuProps` target). */
export type StripChipBoxProps = Omit<ComponentProps<`div`>, `children` | `className`> & {
  [key: `data-${string}`]: string | undefined
}

export interface StripChipProps
  extends Omit<ComponentProps<`button`>, `children` | `title` | `ref`> {
  ref?: Ref<HTMLButtonElement>
  /** The lead glyph (status glyph, agent mark, icon), in a 14px box. */
  lead?: ReactNode
  /** A mono identifier (`EXP-12`) before the label. */
  identifier?: string | null
  /** The label (the issue title, the session's name); absent = the
   *  identifier alone (a short chip). */
  label?: ReactNode
  /** The open nested item (`<chip> › <open one>`). */
  path?: string | null
  /** The chip opens a dropdown (its subtree) instead of itself. */
  chevron?: boolean
  active?: boolean
  /** A chip keeps its hairline at rest (`stripTabClass` bordered). */
  bordered?: boolean
  /** After the open button, inside the box (a close ×). */
  trailing?: ReactNode
  /** The box's own classes (width caps). */
  boxClassName?: string
  /** Props for the BOX (a context-menu target's `menuProps`, middle-click
   *  handlers). The rest go on the open button. */
  boxProps?: StripChipBoxProps
  /** The tooltip (`title`) on the open button. */
  tooltip?: string
}

export function StripChip({
  ref,
  lead,
  identifier,
  label,
  path,
  chevron = false,
  active = false,
  bordered = true,
  trailing,
  boxClassName,
  boxProps,
  tooltip,
  className,
  type = `button`,
  ...button
}: StripChipProps) {
  return (
    <div
      {...boxProps}
      data-slot="strip-chip"
      data-active={active || undefined}
      className={cn(
        stripTabClass(active, bordered),
        `max-w-[15rem] gap-0 overflow-hidden p-0`,
        boxClassName
      )}
    >
      <button
        ref={ref}
        type={type}
        title={tooltip}
        className={cn(
          `flex h-full min-w-0 flex-1 cursor-pointer items-center gap-1.5 px-2.5 outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`,
          trailing ? `pr-1` : undefined,
          className
        )}
        {...button}
      >
        {lead !== undefined && lead !== null && (
          <span className="flex size-3.5 shrink-0 items-center justify-center">{lead}</span>
        )}
        {identifier && (
          <span data-slot="strip-chip-identifier" className="shrink-0 font-mono text-xs text-muted-foreground">
            {identifier}
          </span>
        )}
        {label !== undefined && label !== null && label !== `` && (
          <span data-slot="strip-chip-label" className="min-w-0 truncate">
            {label}
          </span>
        )}
        {path && (
          <span data-slot="strip-chip-path" className="min-w-0 truncate text-muted-foreground">
            {`› ${path}`}
          </span>
        )}
        {chevron && (
          <ChevronDownIcon
            aria-hidden
            data-slot="strip-chip-chevron"
            className="size-3.5 shrink-0 text-muted-foreground"
          />
        )}
      </button>
      {trailing}
    </div>
  )
}

/** The "+N" of a folded zone (THE Menu's trigger): the strip chip box at
 *  rest, the count alone. */
export function StripOverflowButton({
  ref,
  count,
  className,
  type = `button`,
  ...props
}: Omit<ComponentProps<`button`>, `children` | `ref`> & {
  ref?: Ref<HTMLButtonElement>
  count: number
}) {
  return (
    <button
      ref={ref}
      type={type}
      data-slot="strip-overflow"
      className={cn(
        stripTabClass(false, true),
        `shrink-0 cursor-pointer px-2 tabular-nums outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`,
        className
      )}
      {...props}
    >
      +{count}
    </button>
  )
}
