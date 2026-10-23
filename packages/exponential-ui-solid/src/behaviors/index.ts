// FEED-131: the HEADLESS layer of the Solid renderer: Radix's machinery on
// plain elements (no Kobalte; positioning = @floating-ui/dom, modality =
// aria-hidden's `hideOthers`), each module writing what Radix writes in the
// order Radix writes it (the web-parity check diffs the `style` attribute
// strings and the attribute order). The natives (`src/natives/`) and the
// primitive set (`src/primitives/`) import from here; nothing here reads the
// surface context: the overlay layer, `dir` and reduced motion come in as
// parameters. Internal to the package (`createElementSize` would clash with
// the renderer's own export of that name).
//
// | module | reproduces |
// |---|---|
// | `style-writer.ts` | react-dom's `setValueForStyles` (`styleWriter`, `bindStyle`) |
// | `portal.tsx` | Radix `Portal` (no wrapper element) |
// | `presence.ts` | Radix `Presence` |
// | `dismissable-layer.ts` | Radix `DismissableLayer` (+ the Select's lighter `dismissable`) |
// | `focus-scope.ts` | Radix `FocusScope` |
// | `focus-guards.ts` | Radix `FocusGuards` |
// | `scroll-lock.ts` | react-remove-scroll-bar (Radix's `RemoveScroll` body lock) |
// | `modal.ts` | Radix's modal content effects (`hideOthers` + the scroll lock) |
// | `popper.tsx` | Radix `Popper` (`PopperContent` + the `transformOrigin` middleware; the Select's `popperWrapper`) |
// | `grace-area.ts` | Radix's pointer grace polygon (Tooltip, Menu sub-triggers) |
// | `visually-hidden.tsx` | Radix `VisuallyHidden` |
// | `typeahead.ts` | Radix's `useTypeaheadSearch` + `getNextMatch` |
// | `menu.tsx` | Radix `Menu` (`MenuContentImpl`, `MenuItemImpl`, `MenuItem`, the keyboard tracking) |
// | `roving-focus.ts` | Radix `RovingFocusGroup` (Tabs, ToggleGroup, RadioGroup keys) |
// | `listbox.ts` | React's `useListNavigation` (listbox.ts) + Radix Select's trigger keys |
// | `picker-popover.tsx` | Radix `Popover` (Portal + Content), the date/time pickers' |
// | `collapsible.ts` | Radix `CollapsibleContent` (Accordion's content) |
// | `drag-to-dismiss.ts` | the Drawer's drag-to-dismiss (vaul-like) |
// | `element-size.ts` | Radix `useSize` |
// | `bubble-input.tsx` | Radix's `BubbleInput`/`BubbleSelect` + `isFormControl` |
// | `slider.ts` | Radix `Slider`'s maths + keys |
// | `image-loading-status.ts` | Radix Avatar's `useImageLoadingStatus` |
// | `search-match.ts` | the core's `search_matches` |

export * from "./style-writer"
export * from "./portal"
export * from "./presence"
export * from "./dismissable-layer"
export * from "./focus-scope"
export * from "./focus-guards"
export * from "./scroll-lock"
export * from "./modal"
export * from "./popper"
export * from "./grace-area"
export * from "./visually-hidden"
export * from "./typeahead"
export * from "./menu"
export * from "./roving-focus"
export * from "./listbox"
export * from "./picker-popover"
export * from "./collapsible"
export * from "./drag-to-dismiss"
export * from "./element-size"
export * from "./bubble-input"
export * from "./slider"
export * from "./image-loading-status"
export * from "./search-match"
