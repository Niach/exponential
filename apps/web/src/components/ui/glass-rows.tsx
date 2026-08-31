import * as React from "react"
import { Check, ChevronRight } from "lucide-react"

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
  count,
  trailing,
  className,
}: {
  label: string
  count?: number
  trailing?: React.ReactNode
  className?: string
}) {
  return (
    <div
      data-slot="glass-section-header"
      className={cn(`flex items-center gap-1.5 px-1 pt-1 pb-2`, className)}
    >
      <span className="text-sm font-medium text-foreground/70">{label}</span>
      {count !== undefined && (
        <span className="text-xs text-foreground/50">{count}</span>
      )}
      {trailing && (
        <div className="ml-auto flex items-center gap-1.5">{trailing}</div>
      )}
    </div>
  )
}

function GlassRow({
  interactive = false,
  className,
  onClick,
  onKeyDown,
  ...props
}: React.ComponentProps<`div`> & { interactive?: boolean }) {
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
      className={cn(
        `flex items-center gap-3 rounded-md border border-glass-stroke bg-glass-row p-3`,
        interactive &&
          `cursor-pointer transition-colors duration-fast outline-none hover:bg-glass-active/50 focus-visible:ring-[3px] focus-visible:ring-ring/50`,
        className
      )}
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
  ...inputProps
}: Omit<React.ComponentProps<typeof Input>, `className`> & {
  id: string
  label: string
  trailing?: React.ReactNode
  className?: string
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
        className="h-auto min-w-0 flex-1 rounded-none border-0 bg-transparent p-0 text-right text-sm text-foreground/70 shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm"
        {...inputProps}
      />
      {trailing}
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

export {
  GlassSectionHeader,
  GlassRow,
  GlassGroup,
  GlassTabsRow,
  GlassInputRow,
  GlassPickerRow,
  GlassToggleRow,
}
export type { GlassPickerOption }
