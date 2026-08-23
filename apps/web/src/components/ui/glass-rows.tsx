import * as React from "react"
import { Check, ChevronRight } from "lucide-react"

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Sheet, SheetContent } from "@/components/ui/sheet"
import { Switch } from "@/components/ui/switch"
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
  ...props
}: React.ComponentProps<`div`> & { interactive?: boolean }) {
  return (
    <div
      data-slot="glass-row"
      className={cn(
        `flex items-center gap-3 rounded-md border border-glass-stroke bg-glass-row p-3`,
        interactive &&
          `cursor-pointer transition-colors duration-fast hover:bg-glass-active/50`,
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
const GLASS_PICKER_ROW = `flex w-full items-center gap-3 rounded-none border-0 bg-transparent px-4 py-3 shadow-none focus-visible:border-0 focus-visible:ring-0 data-[size=default]:h-auto`

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
            showCloseButton={false}
            className="flex max-h-[85dvh] flex-col gap-0 rounded-t-xl p-0 pb-[env(safe-area-inset-bottom)]"
          >
            <div className="px-4 pt-3 pb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">
              {label}
            </div>
            <div className="overflow-y-auto p-1">
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
  GlassPickerRow,
  GlassToggleRow,
}
export type { GlassPickerOption }
