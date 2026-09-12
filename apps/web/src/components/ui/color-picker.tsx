import { useState } from "react"
import { Button } from "@/components/ui/button"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"

interface ColorPickerProps {
  /** The picked colour (a `LABEL_COLORS` hex). Empty = nothing picked. */
  value: string
  onChange: (color: string) => void
  /** Restricts the grid (defaults to the shared label palette). */
  colors?: string[]
  id?: string
  /** Read-only surfaces still SHOW the swatch; the grid just never opens. */
  disabled?: boolean
}

// EXP-862: THE colour picker — the twin of `IconPicker`, so the board form's
// icon and colour triggers are ONE control repeated: the same rounded square
// showing the current pick, opening the existing `ColorSwatchGrid` in a
// popover. Mirrored on desktop (`board_form::color_picker`), iOS and Android.
export function ColorPicker({
  value,
  onChange,
  colors,
  id,
  disabled = false,
}: ColorPickerProps) {
  const [open, setOpen] = useState(false)
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          id={id}
          type="button"
          variant="outline"
          disabled={disabled}
          aria-label={value ? `Color: ${value}` : `Pick a color`}
          title={value || `Pick a color`}
          // EXP-771 shape rule: an icon-only ACTION is a circle (the Button
          // base), a PICKER is a rounded SQUARE. The swatch it previews is a
          // circle, like the cells of the grid it opens.
          className={`h-9 w-9 shrink-0 rounded-md p-0 ${value ? `` : `border-dashed text-muted-foreground`}`}
        >
          {value ? (
            <span
              className="size-4 rounded-full"
              style={{ backgroundColor: value }}
            />
          ) : (
            <span className="size-4 rounded-full border border-dashed border-current" />
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-auto p-3">
        {/* 8 × 28px cells + 7 × 6px gaps — the icon grid's column count, so
            the two popovers line up swatch for swatch. */}
        <div className="w-[266px]">
          <ColorSwatchGrid
            colors={colors}
            value={value}
            onChange={(color) => {
              onChange(color)
              setOpen(false)
            }}
          />
        </div>
      </PopoverContent>
    </Popover>
  )
}
