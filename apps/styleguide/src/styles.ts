/**
 * The whole stylesheet, inlined into the single-file page. Colors come from
 * `@exp/design-tokens` so the gallery reads as the same product as the shots it
 * shows; the font is a system stack on purpose (zero runtime dependencies, no
 * webfont fetch, works over `file://`).
 */

import { componentStyles } from "./component-styles.ts"

import { designTokens } from "@exp/design-tokens"

const { palette, glass, radius, size, motion } = designTokens
const bezier = (points: readonly number[]): string => `cubic-bezier(${points.join(`, `)})`
const ease = bezier(motion.ease.standard)

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
  font-family: ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
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
