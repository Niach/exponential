import type { ComponentProps, ReactNode } from "react"
import { Button } from "./button"
import { CheckboxMark } from "./checkbox"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { Separator } from "./separator"

// THE bulk action bar's capsule (EXP-698 r5, #EXP-1388): the opaque glass
// card the issue list's selection bar and the Chats selection bar both
// paint, so the two cannot drift. Only the LOOK lives here: close, the
// count, the actions as children. Placement (md+ the host's floating strip,
// phones the tab bar's slot with `--bulkbar-h`) stays with each web host
// (apps/web bulk-action-bar.tsx, chat-selection-bar.tsx).
//
//   BulkBarCapsule   the card + close + count + separator + children
//   BulkBarAction    one icon button (its label beside it on md+)
//   SelectionCheck   a row's leading selection glyph while selecting
//   SELECTABLE_ROW   a selectable row's hover + selected fill (the issue row's)

const CloseIcon = conceptIcon(`ui-close`)

/** An action button's classes: the phone (and `iconOnly`) runs every action
 *  as a bare 32px glyph cell. */
export function bulkBarActionClass({
  iconOnly = false,
  destructive = false,
}: { iconOnly?: boolean; destructive?: boolean } = {}): string {
  return cn(
    `shrink-0`,
    destructive ? `text-destructive hover:text-destructive` : `text-muted-foreground`,
    iconOnly ? `w-8 px-0!` : `max-md:w-8 max-md:px-0!`
  )
}

export function BulkBarCapsule({
  count,
  onClear,
  iconOnly = false,
  wrap = false,
  children,
  className,
  ...props
}: Omit<ComponentProps<`div`>, `children`> & {
  /** The count cell: the number (the issue bar and chats alike). */
  count: ReactNode
  onClear: () => void
  iconOnly?: boolean
  /** EXP-1048: a column too narrow for one line folds instead of clipping. */
  wrap?: boolean
  children?: ReactNode
}) {
  return (
    <div
      // EXP-698 r5: the opaque glass card of the tab bar and the natives'
      // selection bar — 24px radius over the strong hairline, 10×8 padding.
      // EXP-523: enter-only. Phone budget: the separators go, the gaps halve;
      // `overflow-x-auto` is the safety net, not the plan.
      className={cn(
        `flex items-center gap-1 rounded-3xl border border-glass-stroke-strong bg-glass-card-opaque px-2.5 py-2 motion-enter-pop slide-in-from-bottom-1 max-md:h-[52px] max-md:max-w-[calc(100vw-2rem)] max-md:gap-0.5 max-md:overflow-x-auto shadow-lg shadow-black/40`,
        wrap && `max-w-full flex-wrap justify-center max-md:h-auto`,
        className
      )}
      data-testid="bulk-action-bar"
      {...props}
    >
      <Button
        variant="ghost"
        size="icon"
        className="size-8 shrink-0 text-muted-foreground"
        aria-label="Clear selection"
        onClick={onClear}
      >
        <CloseIcon className="size-4" />
      </Button>
      <span className="shrink-0 px-1 text-sm font-semibold whitespace-nowrap">{count}</span>
      {!iconOnly && <Separator orientation="vertical" className="mx-1 h-4! max-md:hidden" />}
      {children}
    </div>
  )
}

/** The divider before the bar's destructive end. */
export function BulkBarSeparator({ iconOnly = false }: { iconOnly?: boolean }) {
  if (iconOnly) return null
  return <Separator orientation="vertical" className="mx-1 h-4! max-md:hidden" />
}

export function BulkBarAction({
  icon,
  label,
  iconOnly = false,
  destructive = false,
  className,
  ...props
}: Omit<ComponentProps<typeof Button>, `children`> & {
  icon: ReactNode
  label: string
  iconOnly?: boolean
  destructive?: boolean
}) {
  return (
    <Button
      variant="ghost"
      size="sm"
      aria-label={label}
      className={cn(bulkBarActionClass({ iconOnly, destructive }), className)}
      {...props}
    >
      {icon}
      {!iconOnly && <span className="hidden md:inline">{label}</span>}
    </Button>
  )
}

/** The issue list's selectable-row look, shared by the issue row and the
 *  chat row (#EXP-1388, owner on #1283): md+ a flat glass-row fill on hover
 *  (the checkbox column says what is selected), phones a selected row on
 *  the glass-active card with the active hairline. `rest` = the phone's
 *  unselected look (the issue row's card, the chat row's flat line). */
export function selectableRowClass({ selected, rest }: { selected: boolean; rest: string }): string {
  return cn(
    `md:hover:bg-glass-row max-md:rounded-md max-md:border`,
    selected ? `max-md:bg-glass-active max-md:border-glass-stroke-active` : rest
  )
}

/** A row's selection cell while its list selects (the issue list's phone
 *  selection cell ×4): THE checkbox square (CheckboxMark, 16/1/4) centred
 *  in the 20px cell, empty or checked; never a round glyph. */
export function SelectionCheck({
  selected,
  className,
  ...props
}: ComponentProps<`span`> & { selected: boolean }) {
  return (
    <span
      role="checkbox"
      aria-checked={selected}
      className={cn(`flex size-5 shrink-0 items-center justify-center`, className)}
      {...props}
    >
      <CheckboxMark checked={selected} />
    </span>
  )
}
