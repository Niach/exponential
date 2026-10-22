import { MENU_SURFACE_CLASS } from "./menu-surface"

export const POPOVER_CLASSES = {
  // EXP-698 menu recipe: opaque card composite, 12 radius, NO blur and
  // NO shadow. A popover keeps its `p-4` form padding; menu-shaped
  // hosts (Command lists) pass `p-0` as they always have.
  content: `${MENU_SURFACE_CLASS} w-72 origin-(--radix-popover-content-transform-origin) p-4 outline-hidden`,
} as const
