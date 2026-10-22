/** Below `sm` a sheet-arm dialog IS the bottom sheet (EXP-687 → the UI
 *  cleanup batch's "one phone sheet"). */
export const DIALOG_PHONE_QUERY = `(max-width: 639px)`

export const DIALOG_CLASSES = {
  overlay: `fixed inset-0 z-50 bg-black/60 backdrop-blur-sm data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:duration-fast data-[state=closed]:ease-accelerate data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:duration-standard data-[state=open]:ease-decelerate`,
  // ONE phone sheet: a sheet arm below `sm` renders THROUGH `SheetContent`
  // (its grabber, drag-to-dismiss, overlay-tap close and opaque surface)
  // with these on top; `phoneSheetFull` is the `sheet-full` detent.
  phoneSheet: `gap-4 overflow-hidden px-4 pt-0 pb-[max(1rem,env(safe-area-inset-bottom))] outline-none`,
  phoneSheetFull: `h-[94dvh] max-h-[94dvh]`,
  // The panel is a flex COLUMN that never scrolls itself (EXP-369):
  // header and footer stay pinned and the scrolling happens inside
  // `DialogBody` (`flex-1 min-h-0 overflow-y-auto`). Any dialog whose
  // content can outgrow the viewport MUST wrap that content in a
  // DialogBody — without one, tall content is clipped rather than
  // scrolled (short confirm dialogs only hit that below ~200px of
  // viewport height).
  // Below `sm` the presentation is one of the four arms in
  // ui/dialog-arms.ts, `sheet` by default (EXP-687): a bottom sheet
  // with a grabber and no ✕, dismissed by dragging it down or tapping
  // outside. From `sm` up it is the centered frosted panel (EXP-269),
  // capped to the viewport — THE SAME for every arm, which is why the
  // arms own all the padding; the enter is `motion-dialog` (EXP-1243),
  // the sheet arms swap it for `motion-sheet` below `sm`. Callers that reposition or
  // re-cap the panel must sm:-prefix those classes so they compose
  // with (and tailwind-merge away) the sm: base here.
  // A column flex container also fixes EXP-178 for free: items take
  // the container's definite width and long nowrap lines (e.g. an
  // issue title in a picker row) overflow their item instead of
  // inflating the panel the way a grid track's min-content did.
  // sm:w-[calc(100%-2rem)] keeps the 1rem side gutter the old
  // unprefixed max-w-[calc(100%-2rem)] used to give, without sitting in
  // the max-w-* tailwind-merge group — so a caller's sm:max-w-* still
  // caps the panel instead of dropping the gutter.
  content: `fixed z-50 flex flex-col gap-4 overflow-hidden shadow-lg outline-none motion-dialog sm:inset-auto sm:top-[50%] sm:left-[50%] sm:max-h-[calc(100dvh-2rem)] sm:w-[calc(100%-2rem)] sm:max-w-lg sm:translate-x-[-50%] sm:translate-y-[-50%] sm:rounded-2xl sm:border sm:border-glass-stroke-card sm:bg-card/85 sm:shadow-2xl sm:shadow-black/40 sm:backdrop-blur-2xl`,
  /** The sheet arms' grabber on the centred panel: a phone only. */
  grabber: `hidden max-sm:block`,
  /** The hidden Close a drag-to-dismiss clicks. */
  dragClose: `hidden`,
  close: `absolute top-4 right-4 rounded-xs opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4`,
  // Sheets and alerts carry no ✕ on a phone (EXP-687); the
  // full-screen page arm is the one that still needs it.
  closePhoneHidden: `max-sm:hidden`,
  srOnly: `sr-only`,
  // Left-aligned on every viewport (EXP-687): a centered mobile title
  // reads as an alert, and sheets are not alerts.
  header: `flex flex-col gap-2 text-left`,
  // The one scrolling region of a dialog. Everything that can grow — a form's
  // fields, a long list, a wall of explanatory copy — goes in here so the header
  // above it and the footer below it stay pinned to the panel edges. Short
  // content just doesn't scroll: flex-1 only claims free space when the panel is
  // actually height-capped.
  body: `min-h-0 flex-1 overflow-y-auto`,
  // On a phone every action is the full-width `lg` capsule pinned to
  // the bottom of the sheet (EXP-687) — one button shape across web,
  // iOS `GlassSubmitButton` and Android `GlassSubmitButton`.
  footer: `flex flex-col-reverse gap-2 max-sm:[&>[data-slot=button]]:h-10 max-sm:[&>[data-slot=button]]:w-full max-sm:[&>[data-slot=button]]:px-6 sm:flex-row sm:justify-end`,
  /** The Cancel of a sheet arm on a phone (EXP-687): hidden, a sheet is
   *  dismissed by dragging it down or tapping outside; `alert`/`page` keep it. */
  cancelPhoneHidden: `max-sm:hidden`,
  title: `text-lg leading-none font-semibold`,
  description: `text-sm text-muted-foreground`,
} as const
