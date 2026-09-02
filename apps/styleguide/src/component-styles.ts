/**
 * The component gallery's stylesheet — the canonical glass control set,
 * hardcoded ONCE in plain CSS so the four clients can be held against it.
 *
 * HARD RULE: not one colour, radius or duration LITERAL lives in this file.
 * Every such value is a `var(--…)` declared in `styles.ts`'s `:root`, which is
 * itself generated from `@exp/design-tokens` — so a token change moves these
 * demos and a demo can never quietly disagree with the token it documents.
 * `components.test.ts` enforces it. Pixel spacings, gaps, font sizes and
 * widths ARE literals: those are Tailwind steps, not tokens.
 */

export const componentStyles = `
/* ---------------------------------------------------------------- layout */
.cmp-stack { display: grid; gap: 12px; }
.cmp-inline { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }

/* -------------------------------------------------------- section header */
.cmp-section-header { display: flex; align-items: center; gap: 6px; padding: 4px 4px 8px; }
.cmp-section-header .title { font-size: 14px; line-height: 20px; font-weight: 500; color: var(--fg-70); }
.cmp-section-header .trailing { margin-left: auto; }

/* ------------------------------------------------------- group container */
/* Borderless on purpose: the fill IS the edge, and hairlines between children
   are the only rules inside it. An outer stroke here double-draws against the
   card it sits on. */
.cmp-group { border-radius: var(--r-lg); background: var(--row); overflow: hidden; }
.cmp-group > * + * { border-top: 1px solid var(--stroke-soft); }

/* ------------------------------------------------------------- row shell */
.cmp-row-shell { display: flex; align-items: center; gap: 12px; padding: 12px 16px; font-size: 14px; }
.cmp-row-shell .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cmp-row-shell .text { display: grid; gap: 2px; flex: 1; min-width: 0; }
.cmp-row-shell .desc { font-size: 12px; color: var(--fg-50); }
.cmp-row-shell .value { flex: none; text-align: right; color: var(--fg-70); }
.cmp-row-shell .chevron { flex: none; display: inline-flex; width: 14px; height: 14px; color: var(--fg-50); }
.cmp-row-shell .chevron .glyph { width: 14px; height: 14px; }
.cmp-row-shell .trailing { flex: none; display: flex; align-items: center; gap: 8px; }
.cmp-row-shell input.value {
  min-width: 0;
  flex: 1;
  padding: 0;
  border: none;
  background: none;
  color: var(--fg-70);
  font: inherit;
  font-size: 14px;
  outline: none;
}
.cmp-row-shell input.value::placeholder { color: var(--fg-50); }

/* ------------------------------------------------------------ glass row */
/* The GAPPED list item — carries its own stroke because nothing separates it
   from its neighbours. The grouped shell above never does. */
.cmp-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border-radius: var(--r-md);
  background: var(--row);
  border: 1px solid var(--stroke-soft);
  transition: background var(--dur) var(--ease);
}
.cmp-row .label { flex: 1; min-width: 0; }
.cmp-row .trailing { flex: none; display: flex; align-items: center; gap: 8px; }
.cmp-row.interactive:hover { background: var(--active-50); }

/* --------------------------------------------------------------- switch */
.cmp-switch {
  position: relative;
  flex: none;
  width: 36px;
  height: 20px;
  border-radius: 9999px;
  background: var(--active);
  transition: background var(--dur) var(--ease);
}
.cmp-switch::after {
  content: "";
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--fg);
  transition: transform var(--dur) var(--ease);
}
.cmp-switch.on { background: var(--primary); }
.cmp-switch.on::after { background: var(--primary-fg); transform: translateX(16px); }

/* -------------------------------------------------------------- segments */
.cmp-tabs-row, .cmp-segmented { display: flex; align-items: center; }
/* Embedded: the FIRST row of a group, so it carries no fill and no stroke —
   the group already draws both. */
.cmp-tabs-row { width: 100%; padding: 8px; }
.cmp-segmented {
  height: var(--ctl-lg);
  padding: 3px;
  border-radius: 9999px;
  border: 1px solid var(--stroke-section);
  background: var(--section);
}
.cmp-tabs-row .tab, .cmp-segmented .tab {
  flex: 1;
  padding: 4px 8px;
  border-radius: 9999px;
  border: 1px solid transparent;
  font-size: 14px;
  line-height: 18px;
  text-align: center;
  white-space: nowrap;
  color: var(--muted-fg);
  transition: background var(--dur) var(--ease);
}
/* Embedded in a group the strip has no capsule to fill, so the segments take py-1.5. */
.cmp-tabs-row .tab { padding-top: 6px; padding-bottom: 6px; }
.cmp-tabs-row .tab.active, .cmp-segmented .tab.active {
  background: var(--active);
  border-color: var(--stroke-active);
  color: var(--fg);
}

/* ---------------------------------------------------------------- buttons */
.cmp-icon-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: var(--ctl-md);
  height: var(--ctl-md);
  border-radius: 50%;
  background: var(--card);
  border: 1px solid var(--stroke);
  color: var(--fg-70);
  cursor: pointer;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.cmp-icon-button:hover { background: var(--active); color: var(--fg); }
.cmp-icon-button .glyph { width: 16px; height: 16px; }

.cmp-button-xs {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: var(--ctl-sm);
  padding: 0 8px;
  border-radius: 9999px;
  border: 1px solid var(--stroke);
  background: var(--card);
  color: var(--fg-70);
  font: inherit;
  font-size: 12px;
  line-height: 16px;
  font-weight: 500;
  white-space: nowrap;
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.cmp-button-xs:hover { background: var(--active); color: var(--fg); }
.cmp-button-xs .glyph { width: 12px; height: 12px; }

/* The MOBILE sheet submit: full width, radius 10, solid. Web and desktop
   primaries stay capsules — see the status table. */
.cmp-button-primary {
  display: block;
  width: 100%;
  padding: 14px 16px;
  border-radius: var(--r-md);
  border: 1px solid transparent;
  background: var(--primary);
  color: var(--primary-fg);
  font: inherit;
  font-size: 14px;
  font-weight: 500;
  text-align: center;
  cursor: pointer;
}
.cmp-button-primary.disabled { background: var(--card); border-color: var(--stroke); color: var(--fg-50); }

/* ------------------------------------------------------------ pill / chip */
.cmp-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 9999px;
  background: var(--card);
  border: 1px solid var(--stroke);
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.cmp-pill:hover { background: var(--active); }
.cmp-pill.active { background: var(--active); border-color: var(--stroke-active); }
.cmp-pill .glyph { width: 12px; height: 12px; }

/* Static metadata, never a target — so no stroke and a softer radius. */
.cmp-chip {
  display: inline-flex;
  align-items: center;
  padding: 4px 8px;
  border-radius: var(--r-sm);
  background: var(--card);
  color: var(--fg);
  font-size: 12px;
  font-weight: 500;
}

/* ----------------------------------------------------------- text field */
.cmp-text-field {
  display: block;
  width: 100%;
  height: var(--input-h);
  padding: 0 12px;
  border-radius: var(--r-lg);
  background: var(--card);
  border: 1px solid var(--stroke);
  color: var(--fg);
  font: inherit;
  font-size: 14px;
  outline: none;
  transition: border-color var(--dur) var(--ease);
}
.cmp-text-field:focus { border-color: var(--stroke-active); }
.cmp-text-field::placeholder { color: var(--fg-50); }

/* ---------------------------------------------------------------- sheet */
.cmp-sheet {
  border-radius: var(--r-xl3) var(--r-xl3) 0 0;
  border-top: 1px solid var(--stroke);
  background: var(--bg-bottom);
  overflow: hidden;
}
.cmp-sheet .grabber { width: 36px; height: 4px; margin: 8px auto 0; border-radius: 9999px; background: var(--fg-30); }
/* The header gutter is 20, the content gutter 16 — the title optically aligns
   with row labels once the group's own 16 is added. */
.cmp-sheet .header { display: flex; align-items: center; gap: 8px; padding: 22px 20px 10px; }
.cmp-sheet .header .title { font-size: 18px; font-weight: 600; }
.cmp-sheet .header .trailing { margin-left: auto; }
.cmp-sheet .content { display: grid; gap: 12px; padding: 0 16px 16px; }

/* ----------------------------------------------------------------- menu */
/* Opaque by construction: the alpha fill is composited over the popover solid
   so a menu never shows the row it floats above. */
.cmp-menu {
  min-width: 180px;
  max-width: 280px;
  padding: 4px;
  border-radius: var(--r-lg);
  border: 1px solid var(--stroke);
  background: var(--menu-bg);
}
.cmp-menu .item {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 48px;
  padding: 0 12px;
  border-radius: var(--r-sm);
  font-size: 14px;
  color: var(--fg-90);
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.cmp-menu .item:hover { background: var(--active); }
.cmp-menu .item.destructive { color: var(--destructive); }
.cmp-menu .item .glyph { flex: none; width: 16px; height: 16px; }
.cmp-menu .divider { height: 1px; margin: 4px 0; background: var(--stroke-soft); }

/* -------------------------------------------------------------- tab bar */
.cmp-tab-bar {
  display: inline-flex;
  gap: 4px;
  padding: 4px;
  border-radius: 9999px;
  border: 1px solid var(--stroke-strong);
  background: var(--opaque-card);
}
.cmp-tab-bar .item {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  color: var(--fg-70);
  transition: background var(--dur) var(--ease);
}
.cmp-tab-bar .item.active { background: var(--active); color: var(--fg); }
.cmp-tab-bar .glyph { width: 20px; height: 20px; }

/* ------------------------------------------------------------- usage bar */
.cmp-usage-bar { display: grid; gap: 6px; }
.cmp-usage-bar .line { display: flex; align-items: baseline; gap: 8px; font-size: 12px; }
.cmp-usage-bar .amount { margin-left: auto; color: var(--muted-fg); font-variant-numeric: tabular-nums; }
.cmp-usage-bar .track { height: 6px; border-radius: 9999px; background: var(--stroke-strong); overflow: hidden; }
.cmp-usage-bar .fill { width: 62%; height: 100%; border-radius: 9999px; background: var(--fg-30); }
.cmp-usage-bar.warn .fill { width: 88%; background: var(--warn); }

/* -------------------------------------------------------------- divider */
.cmp-divider { height: 1px; background: var(--stroke-soft); }

/* --------------------------------------------------------------- tokens */
.cmp-swatches { display: grid; gap: 8px; }
.cmp-swatch { display: flex; align-items: center; gap: 12px; font-size: 12px; }
.cmp-swatch .box { flex: none; width: 48px; height: 30px; border-radius: var(--r-sm); }
.cmp-swatch .name { color: var(--fg); }
.cmp-swatch .value {
  margin-left: auto;
  color: var(--muted-fg);
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
}
.cmp-swatch .box.fill-section { background: var(--section); }
.cmp-swatch .box.fill-row { background: var(--row); }
.cmp-swatch .box.fill-card { background: var(--card); }
.cmp-swatch .box.fill-active { background: var(--active); }
.cmp-swatch .box.stroke-row { border: 1px solid var(--stroke-soft); }
.cmp-swatch .box.stroke-section { border: 1px solid var(--stroke-section); }
.cmp-swatch .box.stroke-card { border: 1px solid var(--stroke); }
.cmp-swatch .box.stroke-strong { border: 1px solid var(--stroke-strong); }
.cmp-swatch .box.stroke-active { border: 1px solid var(--stroke-active); }

.cmp-radius { display: flex; flex-wrap: wrap; gap: 12px; }
.cmp-radius .step { display: grid; gap: 6px; justify-items: center; font-size: 12px; color: var(--muted-fg); }
.cmp-radius .box { width: 64px; height: 64px; background: var(--card); border: 1px solid var(--stroke); }
.cmp-radius .box.r-sm { border-radius: var(--r-sm); }
.cmp-radius .box.r-md { border-radius: var(--r-md); }
.cmp-radius .box.r-lg { border-radius: var(--r-lg); }
.cmp-radius .box.r-xl { border-radius: var(--r-xl); }
.cmp-radius .box.r-xl2 { border-radius: var(--r-xl2); }
.cmp-radius .box.r-xl3 { border-radius: var(--r-xl3); }

.cmp-size { display: grid; gap: 8px; }
.cmp-size .line { display: flex; align-items: center; gap: 12px; font-size: 12px; color: var(--muted-fg); }
.cmp-size .label { width: 132px; flex: none; color: var(--fg); }
.cmp-size .value { color: var(--muted-fg); }
.cmp-size .bar { width: 120px; border-radius: var(--r-sm); background: var(--card); border: 1px solid var(--stroke); }
.cmp-size .bar.size-ctl-lg { height: var(--ctl-lg); }
.cmp-size .bar.size-ctl-md { height: var(--ctl-md); }
.cmp-size .bar.size-ctl-sm { height: var(--ctl-sm); }
.cmp-size .bar.size-input { height: var(--input-h); }
.cmp-size .bar.size-row { height: var(--row-h); }

.cmp-motion { display: grid; gap: 6px; }
.cmp-motion .line { display: flex; align-items: center; gap: 12px; font-size: 12px; color: var(--muted-fg); }
.cmp-motion .label { width: 156px; flex: none; }
.cmp-motion .track { width: 180px; flex: none; height: 22px; padding: 3px; border-radius: 9999px; background: var(--row); }
.cmp-motion .box { width: 16px; height: 16px; border-radius: 50%; background: var(--fg-30); transition: transform var(--dur) var(--ease); }
.cmp-motion .line:hover .box { transform: translateX(158px); }
.cmp-motion .box.dur-fast { transition-duration: var(--dur); }
.cmp-motion .box.dur-standard { transition-duration: var(--dur-standard); }
.cmp-motion .box.dur-slow { transition-duration: var(--dur-slow); }
.cmp-motion .box.ease-standard { transition-timing-function: var(--ease); }
.cmp-motion .box.ease-decelerate { transition-timing-function: var(--ease-decelerate); }
.cmp-motion .box.ease-accelerate { transition-timing-function: var(--ease-accelerate); }
`
