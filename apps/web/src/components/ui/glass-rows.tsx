import * as React from "react"
import { Check, ChevronRight } from "lucide-react"
import { Slot } from "radix-ui"

import { conceptIcon } from "@/lib/icons.generated"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsList } from "@/components/ui/tabs"
import { useIsMobile } from "@/hooks/use-mobile"
import { cn } from "@/lib/utils"

// EXP-616 — web ports of the iOS glass vocabulary: the row/section ladder from
// GlassTheme.swift (plain-text section headers, 10px glass rows, the grouped
// form card with hairline dividers) and the picker/toggle rows from
// GlassOptionRows.swift (label leading, value trailing, whole row tappable).
// `border-glass-stroke` IS the row stroke: styles.css maps
// `--color-glass-stroke: var(--glass-stroke-row)`, and there is no
// `glass-stroke-row` colour utility.

function GlassSectionHeader({
  label,
  leading,
  trailing,
  className,
}: {
  label: string
  /** Optional glyph before the label (e.g. the board icon on Reviews). */
  leading?: React.ReactNode
  trailing?: React.ReactNode
  className?: string
}) {
  // EXP-818: the GROUP BAND — the Linear group header. A full-width strip
  // filled `bg-glass-section` with the group's name, sitting directly over
  // its flat rows (`ListRow`) with a 4px gap; rows read as a table under a
  // highlighted header, not as a stack of cards. Desktop
  // `surface::glass_section_band` twin.
  return (
    <div
      data-slot="glass-section-header"
      className={cn(
        `mb-1 flex items-center gap-1.5 rounded-md bg-glass-section px-3 py-1.5`,
        className
      )}
    >
      {leading}
      <span className="min-w-0 truncate text-sm font-medium text-foreground/85">{label}</span>
      {trailing && (
        <div className="ml-auto flex items-center gap-1.5">{trailing}</div>
      )}
    </div>
  )
}

// EXP-818: ONE flat list row — no stroke, no fill of its own: rows stack
// with NO gap under a `GlassSectionHeader` band and read as a table; the
// `bg-glass-row` wash is the only thing a hover paints, and the active row
// takes `bg-glass-active`. Every list wears this since EXP-818 (`GlassRow`
// stays for the few real cards). Desktop `surface::flat_row` twin.
const LIST_ROW = `flex items-center gap-3 rounded-md p-3`
const LIST_ROW_INTERACTIVE = `cursor-pointer transition-colors duration-fast outline-none hover:bg-glass-row focus-visible:ring-[3px] focus-visible:ring-ring/50`

function ListRow({
  interactive = false,
  active = false,
  asChild = false,
  className,
  onClick,
  onKeyDown,
  ...props
}: React.ComponentProps<`div`> & {
  interactive?: boolean
  /** The selected row (a master-detail's open item). */
  active?: boolean
  /** Render the row as its single child (a `Link`/`<a>`), like `Button`. */
  asChild?: boolean
}) {
  const rowClassName = cn(
    LIST_ROW,
    interactive && LIST_ROW_INTERACTIVE,
    active && `bg-glass-active`,
    className
  )
  if (asChild) {
    return (
      <Slot.Root
        data-slot="list-row"
        onClick={onClick}
        onKeyDown={onKeyDown}
        className={rowClassName}
        {...props}
      />
    )
  }
  const clickable = interactive && onClick != null
  return (
    <div
      data-slot="list-row"
      role={clickable ? `button` : undefined}
      tabIndex={clickable ? 0 : undefined}
      onClick={onClick}
      onKeyDown={
        clickable
          ? (e) => {
              onKeyDown?.(e)
              if (e.defaultPrevented || e.target !== e.currentTarget) return
              if (e.key === `Enter` || e.key === ` `) {
                e.preventDefault()
                e.currentTarget.click()
              }
            }
          : onKeyDown
      }
      className={rowClassName}
      {...props}
    />
  )
}

const GLASS_ROW = `flex items-center gap-3 rounded-md border border-glass-stroke bg-glass-row p-3`
const GLASS_ROW_INTERACTIVE = `cursor-pointer transition-colors duration-fast outline-none hover:bg-glass-active/50 focus-visible:ring-[3px] focus-visible:ring-ring/50`

function GlassRow({
  interactive = false,
  asChild = false,
  className,
  onClick,
  onKeyDown,
  ...props
}: React.ComponentProps<`div`> & {
  interactive?: boolean
  /** Render the row as its single child (a `Link`/`<a>`), like `Button`. */
  asChild?: boolean
}) {
  const rowClassName = cn(
    GLASS_ROW,
    interactive && GLASS_ROW_INTERACTIVE,
    className
  )

  // The child arm already IS a link/button: it brings its own role, focus and
  // Enter handling, so the div arm's synthetic keyboard plumbing stays out.
  if (asChild) {
    return (
      <Slot.Root
        data-slot="glass-row"
        onClick={onClick}
        onKeyDown={onKeyDown}
        className={rowClassName}
        {...props}
      />
    )
  }

  const clickable = interactive && onClick != null
  return (
    <div
      data-slot="glass-row"
      role={clickable ? `button` : undefined}
      tabIndex={clickable ? 0 : undefined}
      onClick={onClick}
      onKeyDown={
        clickable
          ? (e) => {
              onKeyDown?.(e)
              // Only the row itself — nested buttons/links run their own
              // Enter/Space and the bubbled event must not double-fire.
              if (e.defaultPrevented || e.target !== e.currentTarget) return
              if (e.key === `Enter` || e.key === ` `) {
                e.preventDefault()
                e.currentTarget.click()
              }
            }
          : onKeyDown
      }
      className={rowClassName}
      {...props}
    />
  )
}

function GlassGroup({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="glass-group"
      className={cn(
        `flex flex-col divide-y divide-glass-stroke overflow-hidden rounded-lg bg-glass-row`,
        className
      )}
      {...props}
    />
  )
}

// EXP-694 — the EMBEDDED tab row. A segmented strip stops being a
// free-floating capsule floating above a card and becomes the group's FIRST
// ROW: full width, no fill of its own, no capsule border, 8px of padding on
// every side, and the hairline underneath comes from the group's `divide-y`.
// The segments themselves are unchanged (equal width, rounded pills, the
// active one filled `bg-glass-active`). Mirrors the iOS/Android
// `GlassSegmentedControl` embedded style and the desktop `glass_tabs_row`.
const GLASS_TABS_ROW = `flex h-auto w-full rounded-none border-0 bg-transparent p-2 [&>[data-slot=tabs-trigger]]:h-auto [&>[data-slot=tabs-trigger]]:flex-1 [&>[data-slot=tabs-trigger]]:py-1.5`

function GlassTabsRow({
  value,
  onValueChange,
  className,
  children,
}: {
  value: string
  onValueChange: (value: string) => void
  className?: string
  /** `TabsTrigger`s — the row supplies the `Tabs` root they need. */
  children: React.ReactNode
}) {
  return (
    <Tabs
      data-slot="glass-tabs-row"
      value={value}
      onValueChange={onValueChange}
      className="gap-0"
    >
      <TabsList className={cn(GLASS_TABS_ROW, className)}>{children}</TabsList>
    </Tabs>
  )
}

/** A text field that reads as a picker row: label leading, the value typed
 * trailing, no field chrome of its own — the group around it IS the field
 * (EXP-694, the Name row of the device sheet on every client). `trailing`
 * carries the row's own status glyph (the autosave spinner). */
function GlassInputRow({
  id,
  label,
  trailing,
  className,
  inputClassName,
  ...inputProps
}: Omit<React.ComponentProps<typeof Input>, `className`> & {
  id: string
  label: string
  trailing?: React.ReactNode
  className?: string
  /** Extra classes on the field itself — a native `type="time"` widget
   * ignores `text-right`, so such rows pass `ml-auto w-auto flex-none` to
   * park the whole control at the trailing edge (EXP-698 r4). */
  inputClassName?: string
}) {
  return (
    <div
      data-slot="glass-input-row"
      className={cn(`flex items-center gap-3 px-4 py-3`, className)}
    >
      <Label htmlFor={id} className="shrink-0 font-normal text-foreground">
        {label}
      </Label>
      <Input
        id={id}
        className={cn(
          `h-auto min-w-0 flex-1 rounded-none border-0 bg-transparent p-0 text-right text-sm text-foreground/70 shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm`,
          inputClassName
        )}
        {...inputProps}
      />
      {trailing}
    </div>
  )
}

// EXP-768 — the SEARCH row that heads a picker group on every client (the
// Android `GlassTextField(bordered = false)` first row of the start-coding
// sheet, iOS `GlassSheetSearchField(bordered: false)`): the search glyph
// leading, a chrome-less field filling the row, and the group's hairline
// underneath. The list it filters follows as the group's next child.
const SearchGlyph = conceptIcon(`nav-search`)

function GlassSearchRow({
  value,
  onChange,
  placeholder,
  className,
  ...inputProps
}: Omit<
  React.ComponentProps<typeof Input>,
  `value` | `onChange` | `className` | `placeholder`
> & {
  value: string
  onChange: (value: string) => void
  placeholder: string
  className?: string
}) {
  return (
    <div
      data-slot="glass-search-row"
      className={cn(`flex shrink-0 items-center gap-3 px-4`, className)}
    >
      <SearchGlyph className="size-4 shrink-0 text-muted-foreground" />
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="h-auto min-w-0 flex-1 rounded-none border-0 bg-transparent px-0 py-3 text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm"
        {...inputProps}
      />
    </div>
  )
}

type GlassPickerOption = {
  value: string
  label: React.ReactNode
  disabled?: boolean
}

// The row shell shared by both arms. On the desktop arm these must BEAT the
// stock SelectTrigger classes: `data-[size=default]:h-9` only loses to the
// same data-variant. Since EXP-616 the trigger's fill/hover are unprefixed
// (`bg-glass-row` / `hover:bg-glass-active/50`), so a plain `bg-transparent`
// clears the fill and the hover is inherited rather than restated — the row
// draws the group's own fill one level up.
const GLASS_PICKER_ROW = `flex w-full items-center gap-3 rounded-none border-0 bg-transparent px-4 py-3 shadow-none focus-visible:border-0 focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50 data-[size=default]:h-auto`

function GlassPickerRow({
  label,
  value,
  onValueChange,
  options,
  placeholder,
  disabled,
  renderValue,
  className,
}: {
  label: string
  value: string | undefined
  onValueChange: (v: string) => void
  options: GlassPickerOption[]
  placeholder?: string
  disabled?: boolean
  renderValue?: (option: GlassPickerOption | undefined) => React.ReactNode
  className?: string
}) {
  const isMobile = useIsMobile()
  const [open, setOpen] = React.useState(false)
  const selected = options.find((option) => option.value === value)

  if (isMobile) {
    return (
      <>
        <button
          type="button"
          data-slot="glass-picker-row"
          disabled={disabled}
          onClick={() => setOpen(true)}
          className={cn(
            `flex w-full items-center gap-3 px-4 py-3 text-left transition-colors duration-fast hover:bg-glass-active/50 disabled:pointer-events-none disabled:opacity-50`,
            className
          )}
        >
          <span className="text-sm text-foreground">{label}</span>
          <span className="ml-auto truncate text-sm text-foreground/70">
            {renderValue
              ? renderValue(selected)
              : (selected?.label ?? placeholder)}
          </span>
          <ChevronRight className="size-3.5 shrink-0 text-foreground/50" />
        </button>
        <Sheet open={open} onOpenChange={setOpen}>
          <SheetContent
            side="bottom"
            className="flex flex-col gap-0 p-0 pb-[env(safe-area-inset-bottom)]"
          >
            <SheetHeader className="pb-2">
              <SheetTitle>{label}</SheetTitle>
            </SheetHeader>
            <div className="min-h-0 flex-1 overflow-y-auto p-1">
              {options.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  disabled={option.disabled}
                  className="flex h-11 w-full items-center gap-3 rounded-md px-3 text-sm hover:bg-glass-row disabled:pointer-events-none disabled:opacity-50"
                  onClick={() => {
                    onValueChange(option.value)
                    setOpen(false)
                  }}
                >
                  <span className="flex-1 truncate text-left">
                    {option.label}
                  </span>
                  {option.value === value && (
                    <Check className="size-4 shrink-0 text-muted-foreground" />
                  )}
                </button>
              ))}
            </div>
          </SheetContent>
        </Sheet>
      </>
    )
  }

  return (
    <Select value={value} onValueChange={onValueChange} disabled={disabled}>
      <SelectTrigger
        data-slot="glass-picker-row"
        className={cn(GLASS_PICKER_ROW, className)}
      >
        <span className="text-sm text-foreground">{label}</span>
        <span className="ml-auto truncate text-sm text-foreground/70">
          {renderValue ? (
            (renderValue(selected) ?? placeholder)
          ) : (
            <SelectValue placeholder={placeholder} />
          )}
        </span>
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem
            key={option.value}
            value={option.value}
            disabled={option.disabled}
          >
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function GlassToggleRow({
  id,
  label,
  checked,
  onCheckedChange,
  disabled,
  description,
  className,
}: {
  id: string
  label: React.ReactNode
  checked: boolean
  onCheckedChange: (v: boolean) => void
  disabled?: boolean
  description?: React.ReactNode
  className?: string
}) {
  return (
    <label
      htmlFor={id}
      data-slot="glass-toggle-row"
      className={cn(
        `flex cursor-pointer items-center gap-3 px-4 py-3`,
        disabled && `cursor-not-allowed opacity-50`,
        className
      )}
    >
      <span className="flex flex-1 flex-col gap-0.5">
        <span className="text-sm font-normal">{label}</span>
        {description && (
          <span className="text-xs text-foreground/50">{description}</span>
        )}
      </span>
      <Switch
        id={id}
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
      />
    </label>
  )
}

/** The glass skin for a stock `SelectTrigger`/`PopoverTrigger` that sits INSIDE
 * a form rather than being a picker row of its own (EXP-698 — one constant,
 * not a per-file copy). */
const GLASS_SELECT_TRIGGER = `border-glass-stroke bg-glass-row shadow-none dark:bg-glass-row dark:hover:bg-glass-active/50`

export {
  GLASS_SELECT_TRIGGER,
  GlassSectionHeader,
  GlassRow,
  GlassGroup,
  GlassTabsRow,
  GlassInputRow,
  GlassSearchRow,
  GlassPickerRow,
  GlassToggleRow,
  ListRow,
}
export type { GlassPickerOption }
