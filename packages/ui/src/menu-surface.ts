// EXP-887 — the ONE floating-menu surface recipe.
//
// Seven contents painted the same glass box by copy-paste: Select, the
// dropdown menu and its submenu, the context menu and its submenu, Popover
// and HoverCard. This is their common core — the fill, the hairline, the
// radius, the text colour and the open/close animation suite. Everything a
// single surface owns on its own (its Radix transform-origin variable, its
// width/height bounds, its padding, its overflow rule) stays inline at the
// call site, because those genuinely differ per menu.
export const MENU_SURFACE_CLASS = `z-50 rounded-lg border border-glass-stroke-card bg-glass-card-opaque text-popover-foreground data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95`
