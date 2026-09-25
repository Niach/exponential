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

import { designTokens } from "@exp/design-tokens"

/** `mutedForeground` → `muted-foreground`, so a token key names its own var. */
export function tokenSlug(key: string): string {
  return key.replace(/([a-z0-9])([A-Z])/g, `$1-$2`).toLowerCase()
}

/**
 * One swatch fill per entry of a token colour GROUP (EXP-941). The Style mode
 * draws `palette`, `semantic` and `diff` in full, and a demo may not carry a
 * colour literal — so the rules are generated from the same objects that
 * generate the `--<prefix>-*` vars in `styles.ts`. Adding a token adds its
 * swatch; there is nothing to keep in sync.
 */
function swatchFills(prefix: string, group: Record<string, string>): string {
  return Object.keys(group)
    .filter((key) => !key.startsWith(`$`))
    .map(
      (key) =>
        `.cmp-swatch .box.fill-${prefix}-${tokenSlug(key)} { background: var(--${prefix}-${tokenSlug(key)}); }`
    )
    .join(`\n`)
}

export const componentStyles = `
/* ---------------------------------------------------------------- layout */
.cmp-stack { display: grid; gap: 12px; }

/* -------------------------------------------------------- section header */
/* EXP-818: the group BAND — a strip on the section fill over its flat rows. */
.cmp-section-header { display: flex; align-items: center; gap: 6px; padding: 6px 12px; margin-bottom: 4px; border-radius: var(--r-md); background: var(--section); }
.cmp-section-header .title { font-size: 14px; line-height: 20px; font-weight: 500; color: var(--fg-85); }
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

/* github-connection (FEED-42): the status block BEFORE the repo list. */
.cmp-github-status { display: grid; gap: 8px; padding: 0 4px; font-size: 14px; color: var(--muted-fg); }
.cmp-github-status .glyph, .cmp-repo-picker .glyph { flex: none; width: 16px; height: 16px; }
.cmp-github-line { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.cmp-github-text { min-width: 0; }
.cmp-github-warn { color: var(--destructive); }
.cmp-github-dot { flex: none; width: 8px; height: 8px; margin: 0 4px; border-radius: 50%; background: var(--ok); }
.cmp-github-accounts { display: grid; gap: 4px; padding-left: 20px; }
.cmp-github-account { display: flex; align-items: center; gap: 8px; min-width: 0; }
.cmp-github-account .glyph { width: 14px; height: 14px; }
.cmp-github-login { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg-85); }
.cmp-github-link { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--muted-fg); }
.cmp-github-link:hover { color: var(--fg); }
.cmp-github-status .cmp-github-link .glyph, .cmp-repo-picker .cmp-github-link .glyph { width: 12px; height: 12px; }
.cmp-github-caption { margin: 0; padding: 0 4px; font-size: 12px; line-height: 18px; color: var(--fg-50); }
.cmp-github-indent { padding-left: 20px; }
.cmp-github-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.cmp-github-status .cmp-github-actions { padding-left: 20px; }

/* repo-picker (FEED-42): banner, search, tap-to-add rows, the dashed footer. */
.cmp-repo-picker-banner { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 10px 12px; border-radius: var(--r-md); border: 1px solid var(--stroke-soft); background: var(--row); font-size: 13px; color: var(--warn); }
.cmp-repo-picker-banner .cmp-github-text { flex: 1; color: var(--fg-85); }
.cmp-repo-picker-row { display: flex; align-items: center; gap: 10px; padding: 10px 12px; font-size: 14px; color: var(--muted-fg); }
.cmp-repo-picker-row:hover { background: var(--active-50); }
.cmp-repo-picker-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; font-size: 13px; color: var(--fg); }
.cmp-repo-picker-lock { flex: none; display: flex; }
.cmp-repo-picker .cmp-repo-picker-lock .glyph { width: 14px; height: 14px; }
.cmp-repo-picker-footer { display: grid; gap: 8px; padding: 12px; border-radius: var(--r-md); border: 1px dashed var(--stroke-strong); }
.cmp-repo-picker-lookup { display: flex; align-items: center; gap: 8px; }
.cmp-repo-picker-lookup .cmp-text-field { flex: 1; min-width: 0; }
.cmp-repo-picker-error { margin: 0; padding: 0 4px; font-size: 12px; color: var(--destructive); }
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

/* ------------------------------------------------------------- rich tab */
/* The STRIP tab — desktop's top tab strip and session bar, web's agent dock.
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

/* The GHOST (EXP-862): a SECONDARY icon button keeps the box and loses the
   shape. No circle, no fill, no stroke at rest, so the circle above is left
   meaning exactly one thing on a surface: press THIS. Hover is the row wash
   under the MD corner, the same one a list row takes. */
.cmp-ghost-icon-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: var(--ctl-md);
  height: var(--ctl-md);
  border-radius: var(--r-md);
  background: transparent;
  border: none;
  color: var(--fg-70);
  cursor: pointer;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.cmp-ghost-icon-button:hover { background: var(--active); color: var(--fg); }
.cmp-ghost-icon-button .glyph { width: 16px; height: 16px; }

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

/* ------------------------------------------------------------- app shell */
/* EXP-723, the CUTOUT, as amended by EXP-771. Four parts: the ground (the page
   gradient), a nav column sitting straight on it with no fill of its own, the
   content as a card inset 10 on the sides and 6 at the bottom, and the two
   36px CHROME bands — the title strip above the card and the session band
   below — which sit on the bare ground too, symmetrically. The wash is
   translucent so the card keeps the same contrast wherever the gradient has
   got to. */
.cmp-app-shell {
  display: grid;
  grid-template-rows: 36px 1fr 36px;
  height: 300px;
  border-radius: var(--r-lg);
  background: linear-gradient(180deg, var(--bg-top), var(--bg-bottom));
  overflow: hidden;
}
/* Each row clips: a grid row sizes to its widest child, and one tab too many
   would otherwise push the card's right inset off. */
.cmp-app-shell > * { min-width: 0; overflow: hidden; }
.cmp-app-shell > .content { display: flex; min-height: 0; padding: 0 10px 6px 0; }
.cmp-app-shell .nav {
  flex: none;
  width: 132px;
  display: grid;
  align-content: start;
  gap: 2px;
  padding: 0 8px;
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
  display: grid;
  align-content: start;
  gap: 10px;
  padding: 14px 12px;
  border: 1px solid var(--stroke);
  border-radius: var(--r-lg);
  background: var(--panel);
  overflow: hidden;
}
.cmp-app-shell .panel .line { height: 8px; border-radius: 9999px; background: var(--row); }
.cmp-app-shell .panel .line:nth-child(2) { width: 72%; }
.cmp-app-shell .panel .line:nth-child(3) { width: 46%; }
/* The two bands are the SHELL's children, not the panel's: they draw no fill
   and no border, so the ground shows straight through them, and only their
   chips are inset 8. A fill here turns the ground into a second card. */
.cmp-app-shell > .header, .cmp-app-shell > .dock {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px;
}
.cmp-app-shell > .header .title { font-size: 13px; font-weight: 500; }

/* ----------------------------------------------------------- session tree */
/* EXP-996: the sessions list as a TREE — a group row over the runs of one
   workflow or one PR stack, children indented under their parent. The row
   rhythm is the app's compact list row (32px, 14px of indent per level, the
   x4 TREE_INDENT), so the specimen and the product read alike. */
.cmp-session-tree { display: flex; flex-direction: column; width: 420px; max-width: 100%; }
.cmp-session-tree-row {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 8px;
  border-radius: var(--r-md);
  font-size: 13px;
}
/* One nesting level: the row's own 8px plus TREE_INDENT. */
.cmp-session-tree-row[data-depth="1"] { padding-left: 22px; }
/* A group is structure, not work: it wears the row fill, never a state dot. */
.cmp-session-tree-group { background: var(--row); }
.cmp-session-tree-icon { display: flex; flex: none; color: var(--muted-fg); }
.cmp-session-tree-icon .glyph { width: 14px; height: 14px; }
.cmp-session-tree-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}
/* Only a WORKFLOW group's name leads anywhere; a stack has no page. */
.cmp-session-tree-link { text-decoration: underline; text-underline-offset: 2px; }
.cmp-session-tree-count {
  flex: none;
  margin-left: auto;
  font-family: ui-monospace, monospace;
  font-size: 11px;
  color: var(--muted-fg);
}
.cmp-session-tree-dot {
  width: 6px;
  height: 6px;
  flex: none;
  border-radius: 50%;
  background: var(--muted-fg);
}
.cmp-session-tree-dot[data-live] { background: var(--ok); }
.cmp-session-tree-id {
  flex: none;
  font-family: ui-monospace, monospace;
  font-size: 11px;
  color: var(--muted-fg);
}
.cmp-session-tree-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* -------------------------------------------------------- work header badge */
/* EXP-1079: the graph badge is a pill; where it STANDS sizes it. Beside the
   face toggle it wears the toggle's 36px rung with a 16px glyph — the md pill
   stretched to the toggle (web \`placement="header"\`, IDE \`badge_size\`). */
.cmp-pill[data-placement="header"] { height: var(--ctl-lg); }
.cmp-pr-graph { display: grid; gap: 12px; }
.cmp-pr-graph-cluster { display: flex; align-items: center; gap: 4px; }
.cmp-pr-graph-stop { color: var(--destructive); }
.cmp-pr-graph-toggle {
  display: inline-flex;
  align-items: center;
  height: var(--ctl-lg);
  padding: 3px;
  border-radius: 9999px;
  background: var(--row);
  border: 1px solid var(--stroke-section);
}
.cmp-pr-graph-segment {
  display: inline-flex;
  align-items: center;
  height: 100%;
  padding: 0 12px;
  border-radius: 9999px;
  font-size: 14px;
  font-weight: 500;
  color: var(--fg-70);
}
.cmp-pr-graph-segment[data-active] { background: var(--active); color: var(--fg); }
/* The popover: the badge's overlay, 320 wide, the session tree's rows. */
.cmp-pr-graph-popover {
  display: grid;
  gap: 8px;
  width: 320px;
  max-width: 100%;
  padding: 8px;
  border-radius: var(--r-lg);
  background: var(--menu-bg);
  border: 1px solid var(--stroke);
}
.cmp-pr-graph-section { display: grid; gap: 2px; }
.cmp-pr-graph-caption { padding: 0 4px; font-size: 12px; color: var(--muted-fg); }

/* ------------------------------------------------------------ session bar */
/* The bottom strip of coding tabs (EXP-769): rich tabs, then the Chat and add
   tools right after the last one. Since EXP-771 it hangs BELOW the card on the
   bare ground — no fill, no border, no hairline — so the demo has to draw the
   card it hangs under, 6px above it. */
.cmp-session-ground {
  padding: 10px 10px 0;
  border-radius: var(--r-lg);
  background: linear-gradient(180deg, var(--bg-top), var(--bg-bottom));
  overflow: hidden;
}
.cmp-session-ground .card {
  display: grid;
  align-content: start;
  gap: 10px;
  height: 84px;
  margin-bottom: 6px;
  padding: 14px 12px;
  border: 1px solid var(--stroke);
  border-radius: var(--r-lg);
  background: var(--panel);
  overflow: hidden;
}
.cmp-session-ground .card .line { height: 8px; border-radius: 9999px; background: var(--row); }
.cmp-session-ground .card .line:nth-child(2) { width: 62%; }
.cmp-session-bar {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 36px;
  padding: 0 8px;
}
.cmp-session-bar .tool {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: none;
  color: var(--fg-50);
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.cmp-session-bar .tool:hover { background: var(--active); color: var(--fg); }
.cmp-session-bar .tool .glyph { width: 14px; height: 14px; }
.cmp-app-shell .add {
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
.cmp-app-shell .add:hover { background: var(--active); color: var(--fg); }
.cmp-app-shell .add .glyph { width: 14px; height: 14px; }

/* ------------------------------------------------------------ page header */
/* EXP-771: one header for every settings page. The SCROLL region is the full
   pane, so the scrollbar rides the viewport edge; the text rides a centred
   896 column inside it, which is why the two are separate elements. */
.cmp-page-header { width: 100%; max-height: 320px; overflow-y: auto; }
.cmp-page-header .content { max-width: 896px; margin: 0 auto; padding: 24px; }
.cmp-page-header .title { font-size: 24px; line-height: 32px; font-weight: 700; }
.cmp-page-header .desc { margin-top: 4px; font-size: 14px; color: var(--muted-fg); }
.cmp-page-header .cmp-divider { margin: 16px 0; }

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

/* EXP-909: the same report in ONE line — three wire labels, three 4px meters,
   three percents. The fills are fixed here because a demo may carry no inline
   style; the real component reads them off miniWindows. */
.cmp-usage-mini { display: flex; align-items: center; gap: 12px; }
.cmp-usage-mini .line { display: flex; flex: 1; align-items: center; gap: 6px; font-size: 11px; }
.cmp-usage-mini .label { color: var(--muted-fg); }
.cmp-usage-mini .amount { color: var(--muted-fg); font-variant-numeric: tabular-nums; }
.cmp-usage-mini .track { flex: 1; height: 4px; border-radius: 9999px; background: var(--stroke-strong); overflow: hidden; }
.cmp-usage-mini .fill { display: block; height: 100%; border-radius: 9999px; background: var(--fg-30); }
.cmp-usage-mini .line:nth-child(1) .fill { width: 4%; }
.cmp-usage-mini .line:nth-child(2) .fill { width: 73%; }
.cmp-usage-mini .line:nth-child(3) .fill { width: 100%; background: var(--destructive); }

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

/* ---------------------------------------------------------------- dialog */
/* The centred modal (EXP-941). A radius-XL card on the OPAQUE card fill under
   a card hairline — a dialog dims the page behind it, and an alpha fill would
   still show the row it covers — with a title, one line of body and a footer
   whose LAST capsule is the primary. Cancel is borderless: two boxed buttons
   side by side ask the reader to choose between two equals. */
.cmp-dialog {
  max-width: 340px;
  padding: 20px;
  border-radius: var(--r-xl);
  border: 1px solid var(--stroke);
  background: var(--opaque-card);
}
.cmp-dialog .title { font-size: 16px; font-weight: 600; }
.cmp-dialog .text { margin-top: 8px; font-size: 14px; color: var(--fg-70); }
.cmp-dialog .footer { display: flex; align-items: center; justify-content: flex-end; gap: 8px; margin-top: 20px; }
.cmp-dialog .footer .cmp-pill.borderless { background: transparent; border-color: transparent; }
.cmp-dialog .footer .cmp-pill.borderless:hover { background: var(--active); }

/* ------------------------------------------------------------ the launcher */
/* EXP-1019 — the start-coding dialog. The frame is the "dialog" control and
   the card is "composer"; what is SPECIAL here is the order, which is the
   whole design: the SUBJECT leads as a headline (the contract's verb, then
   the subject chips), and the field under it has dropped to the secondary
   half — "Additional instructions (optional)…" — because the thing that will
   run is already picked. The tool row and the round submit ride the card, the
   muted options line hangs under it. Drawn by hand: no one component owns
   this arrangement, and the two it is made of are documented on their own. */
.cmp-launch { display: grid; gap: 8px; }
.cmp-launch .caption { padding: 0 4px; font-size: 12px; color: var(--fg-50); }
.cmp-launch .header { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 0 4px; }
.cmp-launch .title { font-size: 18px; line-height: 22px; font-weight: 600; }
.cmp-launch .card {
  border-radius: var(--r-xl);
  border: 1px solid var(--stroke);
  background: var(--card);
}
.cmp-launch .field { padding: 12px 12px 4px; font-size: 14px; line-height: 20px; color: var(--fg-50); }
.cmp-launch .tool-row { display: flex; align-items: center; gap: 2px; padding: 4px 8px 8px; }
/* The ONE circle on the card is the send, and it takes the accent glyph. */
.cmp-launch .tool-row .cmp-icon-button { margin-left: auto; color: var(--primary); }
.cmp-launch .footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 12px;
  padding: 0 4px;
  font-size: 12px;
  color: var(--muted-fg);
}
.cmp-launch .footer .value { color: var(--fg-85); }
.cmp-launch .footer .cmp-switch { flex: none; }

/* ------------------------------------------------------------------ type */
/* The type scale as SPECIMENS, set in the page's own font: Inter is not loaded
   here (no webfont fetch, a file:// page has to work), so the family is named as a
   value rather than faked with a fallback that is not it. */
.cmp-type { display: grid; gap: 14px; }
.cmp-type .line { display: grid; gap: 2px; }
.cmp-type .label { font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted-fg); }
.cmp-type .value { font-size: 11px; color: var(--fg-50); font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; }
.cmp-type .text { color: var(--fg-85); }
.cmp-type .text.size-type-base { font-size: var(--type-base); }
.cmp-type .text.size-type-body { font-size: var(--type-body); line-height: var(--type-body-lh); }
.cmp-type .text.size-type-tool { font-size: var(--type-tool); line-height: var(--type-tool-lh); }

/* Every colour of the three token groups, generated (see swatchFills). */
${swatchFills(`pal`, designTokens.palette)}
${swatchFills(`sem`, designTokens.semantic)}
${swatchFills(`diff`, designTokens.diff)}
`
