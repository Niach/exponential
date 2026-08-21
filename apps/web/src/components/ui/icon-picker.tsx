import { useState } from "react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { Button } from "@/components/ui/button"
import { IconSwatchGrid } from "@/components/ui/icon-swatch-grid"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { BOARD_ICON_COMPONENTS } from "@/lib/board-icons"
import { conceptIcon } from "@/lib/icons.generated"

interface IconPickerProps {
  // Empty string = nothing picked (only reachable with `allowsNone`).
  value: BoardIcon | ``
  onChange: (icon: BoardIcon | ``) => void
  // Tints the selected glyph for a live preview (board color).
  color?: string
  // Offers a "No icon" reset and renders a placeholder when unset.
  allowsNone?: boolean
  id?: string
}

const PlaceholderIcon = conceptIcon(`ui-icon-placeholder`)

// EXP-575: THE icon picker — a single slim swatch showing the current pick
// that opens the curated grid in a popover. Every surface that picks an icon
// (board forms, action editor, action inputs, widget launcher) renders this,
// so the 60-glyph grid never sits inline in a form again. Mirrored on
// desktop (`ui::icon_picker`), iOS (`IconPicker`) and Android (`IconPicker`).
export function IconPicker({
  value,
  onChange,
  color,
  allowsNone = false,
  id,
}: IconPickerProps) {
  const [open, setOpen] = useState(false)
  const Icon = value ? BOARD_ICON_COMPONENTS[value] : undefined
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          id={id}
          type="button"
          variant="outline"
          aria-label={value ? `Icon: ${value}` : `Pick an icon`}
          title={value || `Pick an icon`}
          className={`h-9 w-9 shrink-0 p-0 ${Icon ? `` : `border-dashed text-muted-foreground`}`}
        >
          {Icon ? (
            <Icon className="h-4 w-4" style={color ? { color } : undefined} />
          ) : (
            <PlaceholderIcon className="h-4 w-4" />
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-auto p-3">
        {/* 8 × 28px cells + 7 × 6px gaps — same column count as the natives. */}
        <div className="w-[266px]">
          <IconSwatchGrid
            value={value}
            color={color}
            onChange={(icon) => {
              onChange(icon)
              setOpen(false)
            }}
          />
        </div>
        {allowsNone && value && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="mt-2 h-7 w-full text-muted-foreground"
            onClick={() => {
              onChange(``)
              setOpen(false)
            }}
          >
            No icon
          </Button>
        )}
      </PopoverContent>
    </Popover>
  )
}
