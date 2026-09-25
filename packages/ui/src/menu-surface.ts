// EXP-887 — the ONE floating-menu surface recipe.
//
// Seven contents painted the same glass box by copy-paste: Select, the
// dropdown menu and its submenu, the context menu and its submenu, Popover
// and HoverCard. This is their common core — the fill, the hairline, the
// radius, the text colour and the open/close animation suite. Everything a
// single surface owns on its own (its Radix transform-origin variable, its
// height bound, its overflow rule) stays inline at the call site, because
// those genuinely differ per menu.
export const MENU_SURFACE_CLASS = `z-50 rounded-lg border border-glass-stroke-card bg-glass-card-opaque text-popover-foreground data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95`

// EXP-1074 — and the ONE menu ROW recipe.
//
// The GEOMETRY of a menu is not the call site's: every menu-like row (the
// context and dropdown items, their sub-triggers, check/radio rows, Select
// items, Command rows, the typeahead rows) reads the --menu-* vars in
// styles.css, which mirror packages/design-tokens/tokens.json `menu` — the
// pointer density from md up, the touch density below — so one change moves
// every web menu, and the styleguide's specimen (`menu-specimen.tsx`) wears
// these SAME constants on plain elements. The dropdown and context items
// stay two Radix roots (pointer- vs trigger-anchored); their classes are one.
export const MENU_CONTENT_CLASS = `min-w-(--menu-min-width) max-w-(--menu-max-width) p-(--menu-surface-padding)`

const MENU_ROW_BASE = `flex min-h-(--menu-item-height) cursor-default items-center gap-(--menu-item-gap) rounded-sm px-(--menu-item-padding-x) py-1 text-sm text-foreground/90 outline-hidden select-none focus:bg-glass-active focus:text-foreground data-[inset]:pl-8 data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 data-[variant=destructive]:focus:text-destructive dark:data-[variant=destructive]:focus:bg-destructive/20 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-(--menu-icon-size) [&_svg:not([class*='text-'])]:text-muted-foreground data-[variant=destructive]:*:[svg]:text-destructive!`

export const MENU_ITEM_CLASS = `relative ${MENU_ROW_BASE} data-[disabled]:pointer-events-none data-[disabled]:opacity-50`

/** A submenu's label never wraps: the value beside it truncates instead.
 *  The value (a `MENU_VALUE_CLASS` span) and the chevron both push right;
 *  with a value present the chevron gives up its auto margin, so every value
 *  ends flush against the chevron column instead of floating mid-row. */
export const MENU_SUB_TRIGGER_CLASS = `${MENU_ROW_BASE} whitespace-nowrap data-[state=open]:bg-glass-active data-[variant=destructive]:data-[state=open]:bg-destructive/10 data-[variant=destructive]:data-[state=open]:text-destructive [&>span+svg:last-child]:ml-0`

/** The checkbox / radio rows: the indicator sits in the `pl-8` inset. */
export const MENU_CHECK_ITEM_CLASS = `relative flex min-h-(--menu-item-height) cursor-default items-center gap-(--menu-item-gap) rounded-sm py-1 pr-(--menu-item-padding-x) pl-8 text-sm text-foreground/90 outline-hidden select-none focus:bg-glass-active focus:text-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-(--menu-icon-size)`

export const MENU_LABEL_CLASS = `px-(--menu-item-padding-x) py-1.5 text-sm font-medium data-[inset]:pl-8`

export const MENU_SEPARATOR_CLASS = `-mx-(--menu-surface-padding) my-(--menu-surface-padding) h-px bg-glass-stroke`

export const MENU_SHORTCUT_CLASS = `ml-auto text-xs tracking-widest text-muted-foreground`

/** The current VALUE beside a submenu's label ("Status  Backlog ›"): takes
 *  what is left of the row and truncates, never the label's room. */
export const MENU_VALUE_CLASS = `min-w-0 max-w-[7rem] shrink text-right normal-case tracking-normal truncate`

export const MENU_CHEVRON_CLASS = `ml-auto size-(--menu-icon-size) shrink-0`
