// EXP-687 — the shared chrome of every mobile bottom sheet, whether it was
// painted by ui/dialog.tsx's `sheet` arm or by ui/sheet.tsx's `bottom` side.
// One grabber, one drag gesture, no ✕: the sheet closes by tapping outside or
// by dragging it down (native parity across iOS/Android/web).
export const SHEET_CHROME_CLASSES = {
  /** The pill at the top edge of a sheet. Purely decorative, but it drags. */
  grabber: `mx-auto mt-2 -mb-1 h-1 w-9 shrink-0 rounded-full bg-foreground/30`,
} as const
