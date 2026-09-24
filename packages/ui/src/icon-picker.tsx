import { useState } from "react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { Button } from "./button"
import { type IconOptions, IconSwatchGrid } from "./icon-swatch-grid"
import { Picker, type PickerItem } from "./picker/picker"
import { BOARD_ICON_OPTIONS } from "./board-icons"
import { conceptIcon } from "./icons.generated"

interface IconPickerProps<T extends string> {
  // Empty string = nothing picked (only reachable with `allowsNone`).
  value: T | ``
  onChange: (icon: T | ``) => void
  // The set to offer (EXP-924): the board set unless a surface names another
  // (`DEVICE_ICON_OPTIONS`). The grid sizes itself to a short set.
  options?: IconOptions<T>
  // Tints the selected glyph for a live preview (board color).
  color?: string
  // Offers a "No icon" reset and renders a placeholder when unset.
  allowsNone?: boolean
  id?: string
  // Read-only surfaces (a non-owner's action editor) still SHOW the glyph;
  // the grid just never opens.
  disabled?: boolean
  // The sheet's title on a phone.
  mobileTitle?: string
}

const PlaceholderIcon = conceptIcon(`ui-icon-placeholder`)

// EXP-575: THE icon picker — a single slim swatch showing the current pick
// that opens the curated grid. Every surface that picks an icon (board forms,
// action editor, action inputs, widget launcher) renders this, so the
// 60-glyph grid never sits inline in a form again. Mirrored on desktop
// (`ui::icon_picker`), iOS (`IconPicker`) and Android (`IconPicker`).
//
// EXP-1021 re-homed it onto the shared `Picker` primitive: the grid is the
// surface's `panel`, so the set of glyphs is a POPOVER on a pointer device
// and the same bottom sheet as every other picker on a phone — which is
// what it never was before. The trigger, the set parameter (EXP-924) and
// the "No icon" reset are unchanged.
export function IconPicker<T extends string = BoardIcon>({
  value,
  onChange,
  options = BOARD_ICON_OPTIONS as unknown as IconOptions<T>,
  color,
  allowsNone = false,
  id,
  disabled = false,
  mobileTitle = `Icon`,
}: IconPickerProps<T>) {
  const [open, setOpen] = useState(false)
  const Icon = options.find((option) => option.name === value)?.icon
  const items: PickerItem<T>[] = options.map((option) => ({
    value: option.name,
    label: option.name,
    icon: option.icon,
  }))
  return (
    <Picker
      mode="single"
      items={items}
      value={value === `` ? null : value}
      onChange={(icon) => {
        onChange(icon)
        setOpen(false)
      }}
      open={open}
      onOpenChange={setOpen}
      disabled={disabled}
      mobileTitle={mobileTitle}
      align="start"
      className="w-auto p-3"
      trigger={
        <Button
          id={id}
          type="button"
          variant="outline"
          disabled={disabled}
          aria-label={value ? `Icon: ${value}` : `Pick an icon`}
          title={value || `Pick an icon`}
          // EXP-771 shape rule: an icon-only ACTION is a circle (the Button
          // base), an icon PICKER is a rounded SQUARE — it previews a swatch,
          // and the grid it opens is squares (`IconSwatchGrid`). `rounded-md`
          // is 10px here (`--radius` 12px − 2), the swatches' radius.
          className={`h-9 w-9 shrink-0 rounded-md p-0 ${Icon ? `` : `border-dashed text-muted-foreground`}`}
        >
          {Icon ? (
            <Icon className="h-4 w-4" style={color ? { color } : undefined} />
          ) : (
            <PlaceholderIcon className="h-4 w-4" />
          )}
        </Button>
      }
      panel={
        <div className="p-3">
          {/* 8 × 1.75rem cells + 7 × 0.375rem gaps — the natives' column
              count, in rem because the cells are. 96 glyphs are 12 rows, so
              the grid scrolls inside a short viewport instead of running off
              it: the stable gutter keeps the scrollbar out of the eighth
              column and the 2px padding keeps the hover scale unclipped. A
              set shorter than one row (the device icons) hugs its cells. */}
          <div
            className={
              options.length < 8
                ? `w-max`
                : `-m-0.5 box-content max-h-[min(27rem,calc(var(--radix-popover-content-available-height)-4rem))] w-[16.625rem] overflow-y-auto p-0.5 [scrollbar-gutter:stable] [scrollbar-width:thin]`
            }
          >
            <IconSwatchGrid
              value={value}
              options={options}
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
        </div>
      }
    />
  )
}
