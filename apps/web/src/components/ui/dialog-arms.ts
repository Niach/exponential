// EXP-687 — the four `< sm` presentations a Dialog can take. Kept in their
// own module because ui/dialog.tsx and ui/alert-dialog.tsx both paint from
// them and the strings ARE the spec: one bottom sheet, one opaque #18181B
// surface, one 24px top radius, everywhere.
//
// Each arm carries its OWN mobile-only classes so nothing leaks between them,
// and re-states the `sm:` padding/zoom the shared base leaves to the arm — so
// from `sm` up all four are the identical centered glass panel and DESKTOP
// PRESENTATION IS UNCHANGED by anything here.
//
// `sheet` is the default (EXP-687 flipped it from the EXP-255 full-screen
// page): content-fitted up to 90dvh, matching the native "fitted" detent.
// `sheet-full` is the fixed 94dvh detent EXP-616 introduced for the tall
// forms (Start coding, New action, Device settings) whose content-sized
// height read as a cut-off page. `alert` is the compact centered confirm —
// native `.alert` parity, deliberately NOT a sheet. `page` is the legacy
// full-screen arm, now only for the image lightbox.

/** Shared by `sheet` and `sheet-full`. Opaque, no blur: it fills the screen
 * edge to edge below `sm`, so there is nothing to see through. */
const MOBILE_SHEET_BASE = `max-sm:inset-x-0 max-sm:bottom-0 max-sm:w-full max-sm:rounded-t-3xl max-sm:border-t max-sm:border-glass-stroke-card max-sm:bg-glass-bottom max-sm:px-4 max-sm:pt-0 max-sm:pb-[max(1rem,env(safe-area-inset-bottom))] max-sm:data-[state=closed]:slide-out-to-bottom max-sm:data-[state=open]:slide-in-from-bottom sm:p-6 sm:data-[state=closed]:zoom-out-95 sm:data-[state=open]:zoom-in-95`

export const MOBILE_SHEET = `${MOBILE_SHEET_BASE} max-sm:h-auto max-sm:max-h-[90dvh]`

export const MOBILE_SHEET_FULL = `${MOBILE_SHEET_BASE} max-sm:h-[94dvh]`

export const MOBILE_ALERT = `max-sm:top-1/2 max-sm:left-1/2 max-sm:w-[calc(100%-3rem)] max-sm:max-w-sm max-sm:max-h-[calc(100dvh-3rem)] max-sm:-translate-x-1/2 max-sm:-translate-y-1/2 max-sm:rounded-2xl max-sm:border max-sm:border-glass-stroke-card max-sm:bg-glass-bottom max-sm:p-5 max-sm:data-[state=closed]:zoom-out-95 max-sm:data-[state=open]:zoom-in-95 sm:p-6 sm:data-[state=closed]:zoom-out-95 sm:data-[state=open]:zoom-in-95`

export const MOBILE_PAGE = `max-sm:inset-0 max-sm:w-full max-sm:bg-background max-sm:p-6 max-sm:data-[state=closed]:zoom-out-95 max-sm:data-[state=open]:zoom-in-95 sm:p-6 sm:data-[state=closed]:zoom-out-95 sm:data-[state=open]:zoom-in-95`

export type DialogMobileArm = `sheet` | `sheet-full` | `alert` | `page`

export const MOBILE_ARMS: Record<DialogMobileArm, string> = {
  sheet: MOBILE_SHEET,
  [`sheet-full`]: MOBILE_SHEET_FULL,
  alert: MOBILE_ALERT,
  page: MOBILE_PAGE,
}
