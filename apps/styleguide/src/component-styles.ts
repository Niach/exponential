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

/* relations-card (EXP-736): one row per link inside a group. */
.cmp-relation-row { display: flex; align-items: center; gap: 10px; padding: 10px 12px; font-size: 14px; min-width: 0; }
.cmp-relation-row .dot { flex: none; width: 10px; height: 10px; border-radius: 50%; border: 2px solid var(--ok); }
.cmp-relation-row .caption { flex: none; font-size: 12px; color: var(--fg-50); }
.cmp-relation-row .id { flex: none; font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; font-size: 12px; color: var(--fg-70); }
.cmp-relation-row .title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cmp-relation-row .trailing { flex: none; display: flex; align-items: center; }
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

/* ------------------------------------------------------------- rich tab */
/* The STRIP tab — desktop's top tab strip and terminal dock, web's agent dock.
   Not a pill: it carries a status, an identifier and a close, and a dozen of
   them sit side by side, so it draws no chrome until it is hovered or active. */
.cmp-rich-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 10px;
  border-radius: var(--r-md);
  color: var(--muted-fg);
  font: inherit;
  font-size: 14px;
  white-space: nowrap;
  cursor: pointer;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.cmp-rich-tab:hover { background: var(--row); }
.cmp-rich-tab.active { background: var(--active); color: var(--fg); }
.cmp-rich-tab .glyph { flex: none; width: 16px; height: 16px; }
.cmp-rich-tab .dot { flex: none; width: 6px; height: 6px; border-radius: 50%; color: var(--ok); background: currentColor; }
.cmp-rich-tab .title { max-width: 180px; overflow: hidden; text-overflow: ellipsis; }
.cmp-rich-tab .id {
  color: var(--fg-50);
  font-size: 12px;
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
}
.cmp-rich-tab .badge {
  padding: 0 4px;
  border-radius: var(--r-sm);
  background: var(--card);
  color: var(--fg-70);
  font-size: 12px;
}
/* The close is a ghost: no chrome of its own, or the strip turns into buttons. */
.cmp-rich-tab .close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 20px;
  height: 20px;
  margin-right: -4px;
  border-radius: var(--r-sm);
  color: var(--fg-50);
  transition: color var(--dur) var(--ease);
}
.cmp-rich-tab .close:hover { color: var(--fg); }
.cmp-rich-tab .close .glyph { width: 12px; height: 12px; }

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

/* ------------------------------------------------------------------ pill */
/* ONE capsule for every label-sized thing (EXP-698). What used to be a chip is
   readonly, what used to be a "header button" is sm + action: the same
   chrome, so the only decisions left are a SIZE and whether it is a target. */
.cmp-pill {
  display: inline-flex;
  align-items: center;
  border-radius: 9999px;
  background: var(--card);
  border: 1px solid var(--stroke);
  color: var(--fg-70);
  font: inherit;
  font-weight: 500;
  white-space: nowrap;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.cmp-pill[data-size="md"] { height: var(--ctl-md); gap: 6px; padding: 0 12px; font-size: 14px; }
.cmp-pill[data-size="sm"] { height: var(--ctl-sm); gap: 4px; padding: 0 8px; font-size: 12px; }
.cmp-pill .glyph { flex: none; }
.cmp-pill[data-size="md"] .glyph { width: 16px; height: 16px; }
.cmp-pill[data-size="sm"] .glyph { width: 12px; height: 12px; }
/* The optional status dot keeps its own colour through hover and selection. */
.cmp-pill .dot { flex: none; width: 6px; height: 6px; border-radius: 50%; color: var(--fg-50); background: currentColor; }
.cmp-pill[data-mode="action"], .cmp-pill[data-mode="select"] { cursor: pointer; }
.cmp-pill[data-mode="action"]:hover, .cmp-pill[data-mode="select"]:hover { background: var(--active); color: var(--fg); }
.cmp-pill[data-mode="select"].selected {
  background: var(--active);
  border-color: var(--stroke-active);
  color: var(--fg);
}
/* readonly is metadata: it keeps the rest chrome and is never a target. */
.cmp-pill[data-mode="readonly"] { cursor: default; }
/* EXP-698 r4 — primary is a PAINT flag, orthogonal to size and mode: the
   accent fill the ONE call to action in a row wears (Create, Start coding,
   Watch). Every other capsule in that row stays glass. It is meant for the
   action mode, and only there does it take a hover. */
.cmp-pill[data-primary] {
  background: var(--primary);
  border-color: transparent;
  color: var(--primary-fg);
}
.cmp-pill[data-mode="action"][data-primary]:hover {
  background: color-mix(in srgb, var(--primary) 90%, transparent);
  color: var(--primary-fg);
}

/* ---------------------------------------------------------------- avatar */
/* Picture first. Without one the initials sit on the PERSON'S hue — the
   avatar token list, index = fnv1a32(utf8(userId)) % 8 — as a 20% fill under
   the glyph at full strength. No stroke: the fill is the shape, and the same
   id lands on the same hue on all four clients. */
.cmp-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: var(--ctl-md);
  height: var(--ctl-md);
  border-radius: 50%;
  font-size: 12px;
  font-weight: 500;
}
/* The picture arm: a real photo fills the circle edge to edge. */
.cmp-avatar[data-photo] { background: var(--active); }
.cmp-avatar[data-hue="0"] { background: color-mix(in srgb, var(--avatar-0) 20%, transparent); color: var(--avatar-0); }
.cmp-avatar[data-hue="1"] { background: color-mix(in srgb, var(--avatar-1) 20%, transparent); color: var(--avatar-1); }
.cmp-avatar[data-hue="2"] { background: color-mix(in srgb, var(--avatar-2) 20%, transparent); color: var(--avatar-2); }
.cmp-avatar[data-hue="3"] { background: color-mix(in srgb, var(--avatar-3) 20%, transparent); color: var(--avatar-3); }
.cmp-avatar[data-hue="4"] { background: color-mix(in srgb, var(--avatar-4) 20%, transparent); color: var(--avatar-4); }
.cmp-avatar[data-hue="5"] { background: color-mix(in srgb, var(--avatar-5) 20%, transparent); color: var(--avatar-5); }
.cmp-avatar[data-hue="6"] { background: color-mix(in srgb, var(--avatar-6) 20%, transparent); color: var(--avatar-6); }
.cmp-avatar[data-hue="7"] { background: color-mix(in srgb, var(--avatar-7) 20%, transparent); color: var(--avatar-7); }

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

/* ------------------------------------------------------------ text area */
/* The field's recipe, grown: same fill, stroke and focus swap, three rows tall,
   and it GROWS with content — the drag handle is off everywhere, because a
   hand-resized box does not survive a re-render on any of the four clients. */
.cmp-textarea {
  display: block;
  width: 100%;
  min-height: 64px;
  padding: 8px 12px;
  border-radius: var(--r-lg);
  background: var(--card);
  border: 1px solid var(--stroke);
  color: var(--fg);
  font: inherit;
  font-size: 14px;
  line-height: 20px;
  resize: none;
  outline: none;
  transition: border-color var(--dur) var(--ease);
}
.cmp-textarea:focus { border-color: var(--stroke-active); }
.cmp-textarea::placeholder { color: var(--fg-50); }
/* Inside a group the ROW is the chrome, exactly as for the input row. */
.cmp-textarea.borderless { padding: 0; border-color: transparent; background: none; min-height: 60px; }
.cmp-row-shell .cmp-textarea { flex: 1; min-width: 0; }

/* ------------------------------------------------------------- app shell */
/* EXP-723, the CUTOUT. Three parts: the ground (the page gradient), a nav
   column sitting straight on it with no fill of its own, and the content as a
   card inset 10 on every side. The wash is translucent so the card keeps the
   same contrast wherever the gradient has got to; overflow hidden is what lets
   the dock strip take the two bottom corners. */
.cmp-app-shell {
  display: flex;
  height: 300px;
  border-radius: var(--r-lg);
  background: linear-gradient(180deg, var(--bg-top), var(--bg-bottom));
}
.cmp-app-shell .nav {
  flex: none;
  width: 132px;
  display: grid;
  align-content: start;
  gap: 2px;
  padding: 12px 8px;
}
.cmp-app-shell .nav .item {
  display: flex;
  align-items: center;
  height: 28px;
  padding: 0 8px;
  border-radius: var(--r-sm);
  font-size: 12px;
  color: var(--fg-70);
}
.cmp-app-shell .nav .item.active { background: var(--active); color: var(--fg); }
.cmp-app-shell .panel {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  margin: 10px;
  border: 1px solid var(--stroke);
  border-radius: var(--r-lg);
  background: var(--panel);
  overflow: hidden;
}
.cmp-app-shell .panel .header {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--stroke-soft);
}
.cmp-app-shell .panel .header .title { font-size: 13px; font-weight: 500; }
.cmp-app-shell .panel .content {
  flex: 1;
  min-height: 0;
  display: grid;
  align-content: start;
  gap: 10px;
  padding: 14px 12px;
}
.cmp-app-shell .panel .content .line { height: 8px; border-radius: 9999px; background: var(--row); }
.cmp-app-shell .panel .content .line:nth-child(2) { width: 72%; }
.cmp-app-shell .panel .content .line:nth-child(3) { width: 46%; }
/* The dock is the panel's LAST child, not an overlay — which is the whole
   reason the content column never needs a bottom inset. */
.cmp-app-shell .panel .dock {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px;
  border-top: 1px solid var(--stroke);
  background: var(--popover);
}

/* ------------------------------------------------------------ dock header */
/* The open dock's own chrome: window controls on top, tabs below. */
.cmp-dock-header {
  border: 1px solid var(--stroke);
  border-radius: var(--r-lg);
  background: var(--popover);
  overflow: hidden;
}
.cmp-dock-header .header {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
  height: 28px;
  padding: 0 6px;
}
.cmp-dock-header .strip {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px;
  border-top: 1px solid var(--stroke);
}
.cmp-dock-header .tool {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  border-radius: var(--r-sm);
  background: none;
  color: var(--fg-50);
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.cmp-dock-header .tool:hover { background: var(--active); color: var(--fg); }
.cmp-dock-header .tool .glyph { width: 14px; height: 14px; }
.cmp-app-shell .add, .cmp-dock-header .add {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  border-radius: var(--r-sm);
  background: none;
  color: var(--fg-50);
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.cmp-app-shell .add:hover, .cmp-dock-header .add:hover { background: var(--active); color: var(--fg); }
.cmp-app-shell .add .glyph, .cmp-dock-header .add .glyph { width: 14px; height: 14px; }

/* The COLLAPSED dock's other form (EXP-742): the glass card on the opaque
   popover fill, floating in the panel's bottom-right corner. It carries the
   strip's own rich tabs, so it is the strip folded into a corner rather than
   a new control; the dot and count are the whole bubble when nothing fits. */
.cmp-dock-bubble {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 4px 4px 6px;
  border: 1px solid var(--stroke);
  border-radius: var(--r-xl);
  background: var(--popover);
  cursor: pointer;
}
.cmp-dock-bubble .dot { flex: none; width: 6px; height: 6px; margin-left: 4px; border-radius: 50%; color: var(--ok); background: currentColor; }
.cmp-dock-bubble .amount {
  margin-right: 2px;
  color: var(--fg-70);
  font-size: 12px;
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
}
.cmp-dock-bubble .caption { padding: 0 4px; color: var(--fg-50); font-size: 12px; }
.cmp-dock-bubble .tool {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  border-radius: var(--r-sm);
  background: none;
  color: var(--fg-50);
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.cmp-dock-bubble .tool:hover { background: var(--active); color: var(--fg); }
.cmp-dock-bubble .tool .glyph { width: 14px; height: 14px; }

/* ---------------------------------------------------------- comment card */
/* The avatar rides the timeline gutter; the card holds everything else. The
   image is a LARGE tile, not a thumbnail: a screenshot you have to open is a
   screenshot nobody opens. */
.cmp-comment { display: flex; align-items: flex-start; gap: 10px; }
.cmp-comment .card {
  flex: 1;
  min-width: 0;
  padding: 10px 12px 12px;
  border: 1px solid var(--stroke);
  border-radius: var(--r-xl);
  background: var(--card);
}
.cmp-comment .header { display: flex; align-items: baseline; gap: 8px; }
.cmp-comment .name { font-size: 14px; font-weight: 500; }
.cmp-comment .caption { font-size: 12px; color: var(--fg-50); }
.cmp-comment .text { margin-top: 4px; font-size: 14px; color: var(--fg-90); }
.cmp-comment .image {
  margin-top: 8px;
  aspect-ratio: 16 / 9;
  max-height: 240px;
  border: 1px solid var(--stroke);
  border-radius: var(--r-lg);
  background: var(--section);
}
.cmp-comment .cmp-pill { margin-top: 8px; }
/* EXP-741: the card is the thread — replies under the body behind one
   hairline, then the muted reply row that closes every top-level card. */
.cmp-comment .replies { margin-top: 12px; padding-top: 4px; border-top: 1px solid var(--stroke); }
.cmp-comment .reply { display: flex; align-items: flex-start; gap: 8px; padding: 6px 0; }
.cmp-comment .reply .cmp-avatar { width: 20px; height: 20px; margin-top: 2px; font-size: 9px; }
.cmp-comment .reply-body { flex: 1; min-width: 0; }
.cmp-comment .reply-row { padding: 6px 0; font-size: 12px; color: var(--fg-50); cursor: pointer; }
.cmp-comment .reply-row:hover { color: var(--fg); }

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

/* ------------------------------------------------------------- composer */
/* ONE composer for comments, steering and support replies. The card is the
   chrome, so the field inside it is borderless; the tools are ghosts, because
   four boxed buttons under a box is three boxes too many. */
.cmp-composer {
  border-radius: var(--r-xl);
  background: var(--card);
  border: 1px solid var(--stroke);
}
/* Floating over a feed — the mobile bottom bar — it must be OPAQUE or the
   conversation reads straight through it. */
.cmp-composer.opaque { background: var(--opaque-card); border-color: var(--stroke-strong); }
.cmp-composer .strip { display: flex; flex-wrap: wrap; gap: 6px; padding: 6px 6px 0; }
.cmp-composer .strip .item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: var(--ctl-sm);
  padding: 0 8px;
  border-radius: var(--r-sm);
  background: var(--row);
  color: var(--fg-70);
  font-size: 12px;
}
.cmp-composer .strip .glyph { width: 12px; height: 12px; }
.cmp-composer .field {
  display: block;
  width: 100%;
  min-height: 36px;
  padding: 10px 12px;
  border: none;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 14px;
  line-height: 20px;
  resize: none;
  outline: none;
}
.cmp-composer .field::placeholder { color: var(--fg-50); }
.cmp-composer .tools { display: flex; align-items: center; gap: 2px; padding: 0 6px 6px; }
.cmp-composer .tool, .cmp-composer .submit {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: var(--ctl-sm);
  height: var(--ctl-sm);
  border-radius: 50%;
  border: none;
  background: none;
  color: var(--fg-50);
  cursor: pointer;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.cmp-composer .tool:hover { background: var(--active); color: var(--fg); }
/* Submit is the only tinted glyph in the row — still a ghost circle. */
.cmp-composer .submit { margin-left: auto; color: var(--primary); }
.cmp-composer .submit:hover { background: var(--active); }
.cmp-composer .glyph { width: 16px; height: 16px; }

/* ------------------------------------------------- markdown / steer feed */
/* The chat-sized block set. Only the person's turn gets a bubble; the agent's
   narration is bare text, because a wall of bubbles is unreadable at length. */
.cmp-markdown { display: grid; gap: 10px; }
.cmp-markdown .narration { display: flex; gap: 8px; font-size: 14px; line-height: 20px; color: var(--fg-90); }
.cmp-markdown .narration .glyph { flex: none; width: 12px; height: 12px; margin-top: 4px; color: var(--fg-50); }
.cmp-markdown .bubble {
  justify-self: end;
  max-width: 80%;
  padding: 8px 12px;
  border-radius: var(--r-lg);
  background: var(--active);
  border: 1px solid var(--stroke-strong);
  font-size: 14px;
  line-height: 20px;
}
/* Plan and question are the SAME neutral card: only the header line is tinted,
   so a question never reads as an error and a plan never as a success. */
.cmp-markdown .card {
  padding: 12px;
  border-radius: var(--r-xl);
  background: var(--card);
  border: 1px solid var(--stroke);
  font-size: 14px;
  line-height: 20px;
}
.cmp-markdown .card-head {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
  font-size: 12px;
  font-weight: 500;
  color: var(--primary);
}
.cmp-markdown .card.warn .card-head { color: var(--warn); }
.cmp-markdown .card-head .glyph { flex: none; width: 12px; height: 12px; }
.cmp-markdown .tool-row { display: flex; align-items: baseline; gap: 8px; font-size: 12px; }
.cmp-markdown .tool-row .label { flex: none; font-weight: 500; }
.cmp-markdown .tool-row .value {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--fg-50);
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
}
/* Inline code is TINTED here and only here: agent prose is mostly paths and
   flags, and the neutral chip the issue/comment renderers use vanishes into
   the bubble. Blue text over a 12% fill under a 20% hairline. */
.cmp-markdown code {
  padding: 1px 5px;
  border-radius: var(--r-sm);
  color: var(--code-text);
  background: var(--code-fill);
  box-shadow: inset 0 0 0 1px var(--code-stroke);
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  font-size: 12px;
}
/* A long block clamps instead of pushing the next turn off the screen. */
.cmp-markdown .fold { max-height: 160px; overflow: hidden; }
.cmp-markdown .show-more { display: inline-block; margin-top: 6px; font-size: 12px; color: var(--fg-70); cursor: pointer; }
.cmp-markdown .show-more:hover { color: var(--fg); }

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

/* --------------------------------------------------------------- bulk bar */
.cmp-bulk-bar {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 8px 10px;
  border-radius: var(--r-xl3);
  border: 1px solid var(--stroke-strong);
  background: var(--opaque-card);
}
.cmp-bulk-bar .item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: var(--ctl-md);
  padding: 0 8px;
  border-radius: var(--r-md);
  color: var(--muted-fg);
  font-size: 14px;
}
.cmp-bulk-bar .item.destructive { color: var(--destructive); }
.cmp-bulk-bar .value {
  padding: 0 4px;
  font-size: 14px;
  font-weight: 600;
  color: var(--fg);
}
.cmp-bulk-bar .glyph { width: 16px; height: 16px; }

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
