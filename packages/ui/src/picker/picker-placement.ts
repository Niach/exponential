// N-14 (picker-presentation.json, x4): WHERE a picker's rows open. The host
// decides the presentation (N-6: inline inside a dialog, popover elsewhere),
// a caller's explicit `requested` wins; an inline picker then discloses its
// rows into the ROW host's slot under the whole row (the composer options
// row), or directly under its own trigger anywhere else.

export type PickerPlacementPresentation = `popover` | `inline`

/** `surface` = the popover / phone sheet; `rowSlot` = the slot under the
 *  whole row host; `underTrigger` = in the host's flow under the trigger. */
export type PickerPlacementBody = `surface` | `rowSlot` | `underTrigger`

export interface PickerPlacementInput {
  inDialog: boolean
  inRow: boolean
  requested: PickerPlacementPresentation | null
}

export interface PickerPlacement {
  presentation: PickerPlacementPresentation
  body: PickerPlacementBody
}

/** The rows' cap in px (`inlineMaxHeight`; Tailwind `max-h-72`). */
export const PICKER_INLINE_MAX_HEIGHT = 288

export function pickerPlacement({
  inDialog,
  inRow,
  requested,
}: PickerPlacementInput): PickerPlacement {
  const presentation = requested ?? (inDialog ? `inline` : `popover`)
  if (presentation === `popover`) return { presentation, body: `surface` }
  return { presentation, body: inRow ? `rowSlot` : `underTrigger` }
}
