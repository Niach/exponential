import * as React from "react"
import { formatDateForMutation } from "@exp/db-schema/domain"

import { Button } from "./button"
import { Calendar } from "./calendar"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "./mobile-popover"
import { Pill } from "./pill"

// EXP-941 — the ONE date picker. `Popover` + `Calendar` was inlined three
// times (the issue properties panel, the editor chips, the mobile properties
// tray), each one converting between a `Date` and the wire's `YYYY-MM-DD`
// its own way — and two of them through `new Date(value)`, which the spec
// parses as UTC and which therefore renders the PREVIOUS day west of
// Greenwich. This component speaks the wire format on both sides and converts
// in exactly one place:
//
//   parse     `new Date(`${value}T00:00:00`)`   — no zone suffix = LOCAL
//   serialise `formatDateForMutation(date)`     — local getters, never ISO
//
// A due date is a DATE, never an instant (REV2-49), so there is no time arm.
const DueDateGlyph = conceptIcon(`ui-due-date`)

/** `2026-03-08` → a local midnight `Date`; anything unparseable → null. */
export function parseDateValue(value: string | null | undefined): Date | null {
  if (!value) {
    return null
  }
  const parsed = new Date(`${value}T00:00:00`)
  return Number.isNaN(parsed.getTime()) ? null : parsed
}

/** `Mar 8` — the short form every trigger shows. */
export function formatDateLabel(date: Date) {
  return date.toLocaleDateString(`en-US`, { month: `short`, day: `numeric` })
}

export interface DatePickerProps {
  /** `YYYY-MM-DD`, or null for "no date". */
  value: string | null
  onChange: (value: string | null) => void
  /** A bespoke trigger. Must be ONE element — it is wrapped `asChild`. */
  renderTrigger?: (state: {
    label: string
    value: string | null
  }) => React.ReactNode
  /** A "Clear" row under the grid that reports null. On by default. */
  clearable?: boolean
  mobileTitle?: string
  /** The trigger's text while no date is set. */
  placeholder?: string
  align?: `start` | `center` | `end`
  disabled?: boolean
  "data-testid"?: string
}

export function DatePicker({
  value,
  onChange,
  renderTrigger,
  clearable = true,
  mobileTitle = `Due date`,
  placeholder = `Due date`,
  align = `start`,
  disabled = false,
  ...rest
}: DatePickerProps) {
  const [open, setOpen] = React.useState(false)
  const date = parseDateValue(value)
  const label = date ? formatDateLabel(date) : placeholder

  return (
    <MobilePopover
      open={disabled ? false : open}
      onOpenChange={(next) => {
        if (!disabled) {
          setOpen(next)
        }
      }}
    >
      <MobilePopoverTrigger asChild disabled={disabled}>
        {renderTrigger ? (
          renderTrigger({ label, value })
        ) : (
          <Pill
            mode="action"
            disabled={disabled}
            leading={<DueDateGlyph aria-hidden />}
            className={cn(`max-w-full`, !date && `text-muted-foreground`)}
          >
            <span className="min-w-0 truncate">{label}</span>
          </Pill>
        )}
      </MobilePopoverTrigger>
      <MobilePopoverContent
        align={align}
        collisionPadding={12}
        mobileTitle={mobileTitle}
        data-testid={rest[`data-testid`]}
        className="w-auto p-0"
      >
        <Calendar
          mode="single"
          selected={date ?? undefined}
          defaultMonth={date ?? undefined}
          onSelect={(picked) => {
            onChange(formatDateForMutation(picked ?? null))
            setOpen(false)
          }}
        />
        {clearable && (
          <div className="border-t border-glass-stroke p-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="w-full justify-start rounded-md font-normal text-muted-foreground"
              onClick={() => {
                onChange(null)
                setOpen(false)
              }}
            >
              Clear
            </Button>
          </div>
        )}
      </MobilePopoverContent>
    </MobilePopover>
  )
}
