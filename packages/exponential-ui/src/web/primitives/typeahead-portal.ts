/** The attribute the anchored arm stamps on its portal. It lives outside the
 *  host's subtree, so a Radix modal would take a click on it as an OUTSIDE
 *  interaction and close: dialog hosts whitelist this selector in their
 *  `onInteractOutside`, and treat a mounted one as "a menu is open" when
 *  routing Escape. */
export const TYPEAHEAD_PORTAL_SELECTOR = `[data-editor-autocomplete]`

/** Whether a Radix outside-interaction (its `target` is the pointed-at
 *  node) landed inside the anchored arm's portal. `DialogContent` and
 *  `SheetContent` ignore that one by default. */
export function isTypeaheadPortalInteraction(event: {
  target: EventTarget | null
}): boolean {
  return (
    event.target instanceof Element &&
    event.target.closest(TYPEAHEAD_PORTAL_SELECTOR) !== null
  )
}
