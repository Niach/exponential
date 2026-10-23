import {
  MENU_CHECK_ITEM_CLASS,
  MENU_CHEVRON_CLASS,
  MENU_CONTENT_CLASS,
  MENU_ITEM_CLASS,
  MENU_LABEL_CLASS,
  MENU_SEPARATOR_CLASS,
  MENU_SHORTCUT_CLASS,
  MENU_SUB_TRIGGER_CLASS,
  MENU_SURFACE_CLASS,
} from "./menu-surface"

export const DROPDOWN_MENU_CLASSES = {
  // EXP-698 menu recipe: opaque card composite, 12 radius, NO blur and
  // NO shadow — see `bg-glass-card-opaque` in styles.css.
  content: `${MENU_SURFACE_CLASS} ${MENU_CONTENT_CLASS} max-h-(--radix-dropdown-menu-content-available-height) origin-(--radix-dropdown-menu-content-transform-origin) overflow-x-hidden overflow-y-auto`,
  item: MENU_ITEM_CLASS,
  checkItem: MENU_CHECK_ITEM_CLASS,
  itemIndicator: `pointer-events-none absolute left-2 flex size-3.5 items-center justify-center`,
  checkIcon: `size-4`,
  radioIcon: `size-2 fill-current`,
  label: MENU_LABEL_CLASS,
  separator: MENU_SEPARATOR_CLASS,
  shortcut: MENU_SHORTCUT_CLASS,
  subTrigger: MENU_SUB_TRIGGER_CLASS,
  subChevron: MENU_CHEVRON_CLASS,
  // Portaled: un-portaled sub content lives inside the parent content's DOM,
  // and the parent's `glass-panel` backdrop-filter makes it a containing
  // block whose overflow-x-hidden clips the fixed-positioned submenu to
  // invisibility in Safari/WebKit (EXP-305).
  subContent: `${MENU_SURFACE_CLASS} ${MENU_CONTENT_CLASS} origin-(--radix-dropdown-menu-content-transform-origin) overflow-hidden`,
} as const
