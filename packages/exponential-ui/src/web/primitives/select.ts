import { MENU_SURFACE_CLASS } from "./menu-surface"

export const SELECT_CLASSES = {
  // EXP-616: glass by default, matching Input/Textarea. EXP-698: 12
  // radius, card fill + card hairline, focus on the stroke, no ring.
  trigger: `flex w-fit items-center justify-between gap-2 rounded-lg border border-glass-stroke-card bg-glass-card px-3 py-2 text-sm whitespace-nowrap shadow-none transition-[color,border-color] duration-fast outline-none hover:bg-glass-active/50 focus-visible:border-glass-stroke-active focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 data-[placeholder]:text-foreground/50 data-[size=default]:h-9 data-[size=sm]:h-8 *:data-[slot=select-value]:line-clamp-1 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-2 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-(--menu-icon-size) [&_svg:not([class*='text-'])]:text-muted-foreground`,
  triggerIcon: `size-4 opacity-50`,
  // EXP-698 menu recipe: opaque card composite, 12 radius, NO blur and
  // NO shadow — see `bg-glass-card-opaque` in styles.css.
  content: `${MENU_SURFACE_CLASS} relative max-h-(--radix-select-content-available-height) min-w-(--menu-min-width) max-w-(--menu-max-width) origin-(--radix-select-content-transform-origin) overflow-x-hidden overflow-y-auto`,
  /** `position="popper"` only. */
  contentPopper: `data-[side=bottom]:translate-y-1 data-[side=left]:-translate-x-1 data-[side=right]:translate-x-1 data-[side=top]:-translate-y-1`,
  viewport: `p-(--menu-surface-padding)`,
  /** `position="popper"` only. */
  viewportPopper: `h-[var(--radix-select-trigger-height)] w-full min-w-[var(--radix-select-trigger-width)] scroll-my-1`,
  label: `px-(--menu-item-padding-x) py-1.5 text-xs text-muted-foreground`,
  item: `relative flex min-h-(--menu-item-height) w-full cursor-pointer items-center gap-(--menu-item-gap) rounded-sm py-1 pr-8 pl-(--menu-item-padding-x) text-sm text-foreground/90 outline-hidden select-none focus:bg-glass-active focus:text-foreground data-[disabled]:cursor-default data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-(--menu-icon-size) [&_svg:not([class*='text-'])]:text-muted-foreground *:[span]:last:flex *:[span]:last:items-center *:[span]:last:gap-2`,
  itemIndicator: `absolute right-2 flex size-3.5 items-center justify-center`,
  itemCheck: `size-4`,
  separator: `pointer-events-none -mx-1 my-1 h-px bg-glass-stroke`,
  scrollButton: `flex cursor-default items-center justify-center py-1`,
  scrollIcon: `size-4`,
} as const
