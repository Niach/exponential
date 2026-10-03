import type { ComponentProps, ReactNode } from "react"
import { forwardRef } from "react"
import { Button } from "./button"

// THE phone properties sheet row: label left, value right, the whole row the
// target. The web counterpart of the natives' metadata rows (EXP-247; iOS
// `GlassMetaRow`, Android `MetaRow`). EXP-698 r4 matches Android exactly: the
// LABEL is the muted half and the value the readable one, the value's glyph
// rides with it as one trailing unit, and there is no chevron. EXP-1170: a
// set-valued property (Labels) shows its picks joined by ", " and opens the
// shared picker sheet, never a cloud of toggle chips.
export const PropertyRow = forwardRef<
  HTMLButtonElement,
  Omit<ComponentProps<typeof Button>, `value`> & {
    label: string
    value: ReactNode
  }
>(function PropertyRow({ label, value, ...props }, ref) {
  return (
    <Button
      ref={ref}
      type="button"
      variant="ghost"
      className="h-11 w-full justify-between rounded-none px-4 font-normal"
      {...props}
    >
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="flex min-w-0 items-center gap-1.5 text-sm text-foreground">
        {value}
      </span>
    </Button>
  )
})
