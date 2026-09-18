/**
 * The whole stylesheet, inlined into the single-file page. Colors come from
 * `@exp/design-tokens` so the gallery reads as the same product as the shots it
 * shows; the font is a system stack on purpose (zero runtime dependencies, no
 * webfont fetch, works over `file://`).
 */

import { componentStyles, tokenSlug } from "./component-styles.ts"

import { designTokens } from "@exp/design-tokens"

const { palette, glass, radius, size, motion } = designTokens
const bezier = (points: readonly number[]): string => `cubic-bezier(${points.join(`, `)})`
const ease = bezier(motion.ease.standard)

/**
 * Every colour of a token GROUP as `--<prefix>-<key>` lines for the `:root`
 * block (EXP-941). The Style mode's palette table draws one swatch per entry
 * and the hand-written demos may not carry a literal, so the whole group has
 * to arrive here — generated, never transcribed.
 */
function colorVars(prefix: string, group: Record<string, string>): string {
  return Object.entries(group)
    .filter(([key]) => !key.startsWith(`$`))
    .map(([key, value]) => `  --${prefix}-${tokenSlug(key)}: ${value};`)
    .join(`\n`)
}

const page = `
:root {
  color-scheme: dark;
  --bg-top: ${glass.backgroundTop};
  --bg-bottom: ${glass.backgroundBottom};
  --fg: ${palette.foreground};
  --muted-fg: ${palette.mutedForeground};
  --sidebar: ${palette.sidebar};
  --card: ${glass.fillCard};
  --panel: ${glass.fillPanel};
  --row: ${glass.fillRow};
  --active: ${glass.fillActive};
  --stroke: ${glass.strokeCard};
  --stroke-soft: ${glass.strokeRow};
  --stroke-strong: ${glass.strokeStrong};
  --input: ${palette.input};
  --ring: ${palette.ring};
  --ok: ${designTokens.semantic.green};
  --warn: ${designTokens.semantic.yellow};
  --r-sm: ${radius.sm}px;
  --r-md: ${radius.md}px;
  --r-lg: ${radius.lg}px;
  --shot-h: 520px;
  --dur: ${motion.duration.fast}ms;
  --ease: ${ease};
  /* Everything below exists for the Components group (EXP-698): its demos are
     forbidden a single colour / radius / duration literal, so every value they
     need is a token-derived var declared HERE. components.test.ts fails on a
     var(--x) in component-styles.ts that this block does not declare. */
  --section: ${glass.fillSection};
  --stroke-section: ${glass.strokeSection};
  --stroke-active: ${glass.strokeActive};
  --code-text: ${designTokens.semantic.codeText};
  --code-fill: ${designTokens.semantic.codeFill};
  --code-stroke: ${designTokens.semantic.codeStroke};
${Object.entries(designTokens.avatar).filter(([k]) => !k.startsWith(`$`)).map(([, v], i) => `  --avatar-${i}: ${v};`).join(`\n`)}
  --primary: ${palette.primary};
  --primary-fg: ${palette.primaryForeground};
  --popover: ${palette.popover};
  --card-solid: ${palette.card};
  --destructive: ${palette.destructive};
  --r-xl: ${radius.xl}px;
  --r-xl2: ${radius.xl2}px;
  --r-xl3: ${radius.xl3}px;
  --ctl-lg: ${size.controlLg}px;
  --ctl-md: ${size.controlMd}px;
  --ctl-sm: ${size.controlSm}px;
  --input-h: ${size.inputHeight}px;
  --row-h: ${size.rowHeight}px;
  --dur-standard: ${motion.duration.standard}ms;
  --dur-slow: ${motion.duration.slow}ms;
  --ease-decelerate: ${bezier(motion.ease.decelerate)};
  --ease-accelerate: ${bezier(motion.ease.accelerate)};
  --fg-90: color-mix(in oklab, var(--fg) 90%, transparent);
  --fg-85: color-mix(in oklab, var(--fg) 85%, transparent);
  --fg-70: color-mix(in oklab, var(--fg) 70%, transparent);
  --fg-50: color-mix(in oklab, var(--fg) 50%, transparent);
  --fg-30: color-mix(in oklab, var(--fg) 30%, transparent);
  --active-50: color-mix(in oklab, var(--active) 50%, transparent);
  --input-30: color-mix(in oklab, var(--input) 30%, transparent);
  --input-50: color-mix(in oklab, var(--input) 50%, transparent);
  --popover-85: color-mix(in oklab, var(--popover) 85%, transparent);
  /* An alpha fill over a SOLID: the only way a menu or a floating bar stops
     showing the row underneath it. Two layers, one background shorthand. */
  --menu-bg: linear-gradient(var(--card), var(--card)) var(--popover);
  --opaque-card: linear-gradient(var(--card), var(--card)) var(--card-solid);
  /* EXP-941, Style mode: the three colour groups as swatch fills, and the type
     scale as the sizes the specimen lines are set in. Generated from the same
     token objects the natives read, so a swatch cannot lie about its value. */
${colorVars(`pal`, palette)}
${colorVars(`sem`, designTokens.semantic)}
${colorVars(`diff`, designTokens.diff)}
  --type-family: ${designTokens.type.fontFamily};
  --type-base: ${designTokens.type.baseSize}px;
  --type-body: ${designTokens.transcript.bodySize}px;
  --type-body-lh: ${designTokens.transcript.bodyLineHeight}px;
  --type-tool: ${designTokens.transcript.toolSize}px;
  --type-tool-lh: ${designTokens.transcript.toolLineHeight}px;
  font-family: ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
}
/* EXP-887 — a byte-for-byte MIRROR of packages/ui/src/styles.css's root font
   size. The islands are shadow trees: the package stylesheet is adopted into
   each of them, but its own html font-size rule matches nothing in there, so
   a rem inside a real component resolves against THIS page's root instead. Set
   it wrong and every island renders at the wrong scale. Safe to mirror because
   the page's own CSS below is px-only — nothing else on the page moves.
   components.test.tsx locks the pair. */
html {
  font-size: 1.15625rem;
}

@media (max-width: 767px) {
  html {
    font-size: 1rem;
  }
}
* { box-sizing: border-box; }
html, body { height: 100%; }
body {
  margin: 0;
  color: var(--fg);
  background: linear-gradient(180deg, var(--bg-top), var(--bg-bottom)) fixed;
  font-size: 14px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
}
code, .mono { font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; }
a { color: inherit; text-decoration: none; }

.layout { display: grid; grid-template-columns: 260px minmax(0, 1fr); min-height: 100vh; }

/* Sidebar */
.sidebar {
  position: sticky;
  top: 0;
  align-self: start;
  height: 100vh;
  overflow-y: auto;
  border-right: 1px solid var(--stroke-soft);
  background: var(--sidebar);
  padding-bottom: 32px;
}
.brand { padding: 18px 16px 10px; }
.brand h1 { font-size: 13px; letter-spacing: 0.06em; text-transform: uppercase; margin: 0; color: var(--muted-fg); }
.brand p { margin: 4px 0 0; font-size: 12px; color: var(--muted-fg); }
.filter-wrap { position: sticky; top: 0; z-index: 2; padding: 8px 12px 10px; background: var(--sidebar); }
/* EXP-941 — the three modes, as the product's own segmented capsule: 36 tall,
   padding 3, the section fill under the section stroke, and the picked segment
   taking the active fill. It rides the sticky block with the filter, because
   the two are one control: pick a mode, then narrow it. */
.mode-bar {
  display: flex;
  gap: 3px;
  height: 36px;
  padding: 3px;
  margin-bottom: 8px;
  border-radius: 9999px;
  background: var(--section);
  border: 1px solid var(--stroke-section);
}
.mode-btn {
  /* Grow from the LABEL, never from an equal third: a third of the sidebar is
     narrower than the word Components, and a mode bar that ellipsises its own
     mode names is worse than an uneven one. */
  flex: 1 1 auto;
  min-width: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 0 6px;
  border: none;
  border-radius: 9999px;
  background: transparent;
  color: var(--muted-fg);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.mode-btn:hover { color: var(--fg); }
.mode-btn[aria-pressed="true"] { background: var(--active); color: var(--fg); }
.mode-btn .label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mode-btn .count { flex: none; font-size: 11px; color: var(--muted-fg); }
.mode-btn[aria-pressed="true"] .count { color: var(--fg-70); }

/* One mode's nav at a time — and one mode's sections, since a .view is already
   hidden unless it is the active entry. */
.mode-section { display: none; }
body[data-mode="views"] .mode-section[data-mode="views"],
body[data-mode="components"] .mode-section[data-mode="components"],
body[data-mode="style"] .mode-section[data-mode="style"] { display: block; }
/* The size toggle and the shot hints only mean anything against a screenshot. */
body:not([data-mode="views"]) .views-only { display: none; }
.filter {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  color: var(--fg);
  background: var(--row);
  border: 1px solid var(--input);
  border-radius: var(--r-md);
  font: inherit;
  font-size: 13px;
  outline: none;
}
.filter:focus { border-color: var(--ring); box-shadow: 0 0 0 2px var(--active); }
.filter::placeholder { color: var(--muted-fg); }
.group-label {
  padding: 14px 16px 6px;
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted-fg);
}
.nav-link {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 1px 8px;
  padding: 6px 8px;
  border-radius: var(--r-sm);
  font-size: 13px;
  transition: background var(--dur) var(--ease);
}
.nav-link:hover { background: var(--row); }
.nav-link.active { background: var(--active); }
.nav-link .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dots { display: flex; gap: 3px; flex: none; }
.dot { width: 7px; height: 7px; border-radius: 50%; }
.dot.ok { background: var(--fg); }
.dot.missing { background: transparent; border: 1px dashed var(--muted-fg); }
/* Awaiting a HUMAN, not the pipeline — a half-filled dot, so a manual gap never
   reads as a capture the automation forgot. */
.dot.manual { background: transparent; border: 1px solid var(--muted-fg); box-shadow: inset 0 -3px 0 var(--muted-fg); }
.dot.na { background: transparent; border: 1px solid var(--stroke-strong); }
/* A component that exists on that platform but still disagrees with the
   canonical form: present, so not missing; wrong, so not ok. */
.dot.leftover { background: var(--warn); }
.nav-empty { padding: 10px 16px; font-size: 12px; color: var(--muted-fg); }
.hidden { display: none !important; }

/* Main */
.main { padding: 28px 32px 64px; min-width: 0; }
.toolbar { display: flex; align-items: center; gap: 10px; margin-bottom: 18px; flex-wrap: wrap; }
.toolbar .spacer { flex: 1; }
.btn {
  height: 28px;
  padding: 0 10px;
  color: var(--fg);
  background: var(--row);
  border: 1px solid var(--stroke);
  border-radius: var(--r-sm);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.btn:hover { background: var(--active); }
.btn[aria-pressed="true"] { background: var(--active); border-color: var(--stroke-strong); }
.meta-note { font-size: 12px; color: var(--muted-fg); }

.view { display: none; }
.view.active { display: block; }
.view h2 { margin: 0; font-size: 22px; font-weight: 600; letter-spacing: -0.01em; }
.view-id {
  display: inline-block;
  margin-top: 6px;
  padding: 2px 6px;
  font-size: 12px;
  color: var(--muted-fg);
  background: var(--row);
  border: 1px solid var(--stroke-soft);
  border-radius: var(--r-sm);
}
.blurb { max-width: 72ch; margin: 10px 0 0; color: var(--muted-fg); }

.rail {
  display: flex;
  gap: 18px;
  align-items: flex-start;
  margin-top: 22px;
  padding-bottom: 14px;
  overflow-x: auto;
}
figure.shot {
  flex: none;
  margin: 0;
  padding: 10px;
  background: var(--card);
  border: 1px solid var(--stroke);
  border-radius: var(--r-lg);
}
figure.shot figcaption {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 0 2px 8px;
  font-size: 12px;
}
figcaption .platform { font-weight: 600; }
figcaption .dims { color: var(--muted-fg); }
figure.shot img {
  display: block;
  height: auto;
  max-height: var(--shot-h);
  width: auto;
  max-width: 100%;
  border-radius: var(--r-sm);
  background: var(--bg-top);
  cursor: zoom-in;
}
body.actual figure.shot img { max-height: none; max-width: none; }
.placeholder {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 6px;
  padding: 16px;
  text-align: center;
  color: var(--muted-fg);
  border: 1px dashed var(--stroke-strong);
  border-radius: var(--r-sm);
  background: repeating-linear-gradient(135deg, transparent, transparent 8px, var(--row) 8px, var(--row) 16px);
}
.placeholder .state { font-size: 12px; color: var(--fg); opacity: 0.75; }
.placeholder .why { font-size: 12px; max-width: 34ch; margin: 0 auto; }

/* Platforms the view does not claim: one collapsed line, never a card. */
.na-note { margin: 10px 0 0; font-size: 12px; color: var(--muted-fg); }
.na-note summary { cursor: pointer; }
.na-note ul { margin: 6px 0 0; padding-left: 18px; display: grid; gap: 4px; }
.na-note b { color: var(--fg); font-weight: 500; }

/* Components (EXP-698) — code, not screenshots. The demo sits on a phone-width
   canvas over the SAME page gradient the controls are designed against, so the
   white-alpha fills read at the weight they have in the app; a dashed edge says
   this is the specimen, not a screenshot of one. */
.view.component .cmp-demo {
  width: min(100%, 420px);
  margin-top: 22px;
  padding: 20px;
  border: 1px dashed var(--stroke-soft);
  border-radius: var(--r-xl);
  background: linear-gradient(180deg, var(--bg-top), var(--bg-bottom));
}
/* The icon registry is a TABLE of 228 rows, not a control specimen: it gets
   the full reading column instead of the phone-width canvas every other demo
   is measured against. */
.view.component[data-view="tokens-icons"] .cmp-demo { width: min(100%, 880px); }
.cmp-status { margin-top: 22px; border-collapse: collapse; font-size: 12px; }
.cmp-status th {
  width: 64px;
  padding: 6px 12px 6px 0;
  text-align: left;
  font-weight: 500;
  color: var(--muted-fg);
  vertical-align: top;
}
.cmp-status td { padding: 6px 12px 6px 0; border-top: 1px solid var(--stroke-soft); vertical-align: top; }
.cmp-status tr:first-child th, .cmp-status tr:first-child td { border-top: none; }
.cmp-status .dot { display: inline-block; }
.cmp-status code { color: var(--fg); }
.cmp-status .path { display: block; margin-top: 2px; color: var(--muted-fg); font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; }
/* A note is a caveat on an ok row, a to-do on a leftover row: only the latter shouts. */
.cmp-status .note { display: block; margin-top: 2px; color: var(--muted-fg); max-width: 60ch; }
.cmp-status tr.leftover .note { color: var(--warn); }
.cmp-status tr.na td { color: var(--muted-fg); }

/* EXP-941 — the call sites that still draw this semantic by hand. The status
   table above says the control EXISTS on a platform; this says the product
   ignores it anyway, which is the half no file path can imply. */
.leftovers { margin-top: 20px; max-width: 78ch; }
.leftovers-head { font-size: 12px; color: var(--warn); }
.leftovers ul { margin: 6px 0 0; padding-left: 18px; display: grid; gap: 6px; font-size: 12px; }
.leftovers code { color: var(--fg-70); font-size: 12px; }
.leftovers .note { display: block; color: var(--muted-fg); }

/* Lightbox */
dialog.lightbox {
  padding: 0;
  border: none;
  max-width: 96vw;
  max-height: 96vh;
  background: var(--bg-top);
  border-radius: var(--r-lg);
  overflow: auto;
}
dialog.lightbox::backdrop { background: rgba(0, 0, 0, 0.8); }
dialog.lightbox img { display: block; cursor: zoom-out; }

@media (max-width: 900px) {
  .layout { grid-template-columns: 1fr; }
  .sidebar { position: static; height: auto; border-right: none; border-bottom: 1px solid var(--stroke-soft); }
  .main { padding: 20px 16px 48px; }
}
`

export const styles = `${page}${componentStyles}`
