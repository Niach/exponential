export const TOOLTIP_CLASSES = {
  // Glass tooltip (EXP-269) — no Arrow: a translucent arrow
  // double-renders against the blurred panel behind it. EXP-277:
  // on the shared glass-panel recipe (same 95% step it already sat
  // on; unifies blur radius + shadow).
  content: `glass-panel z-50 w-fit origin-(--radix-tooltip-content-transform-origin) rounded-lg px-3 py-1.5 text-xs text-balance text-popover-foreground motion-popover`,
} as const
