/**
 * The whole stylesheet, inlined into the single-file page. Colors come from
 * `@exp/design-tokens` so the gallery reads as the same product as the shots it
 * shows; the font is a system stack on purpose (zero runtime dependencies, no
 * webfont fetch, works over `file://`).
 */

import { designTokens } from "@exp/design-tokens"

const { palette, glass, radius, motion } = designTokens
const ease = `cubic-bezier(${motion.ease.standard.join(`, `)})`

export const styles = `
:root {
  color-scheme: dark;
  --bg-top: ${glass.backgroundTop};
  --bg-bottom: ${glass.backgroundBottom};
  --fg: ${palette.foreground};
  --muted-fg: ${palette.mutedForeground};
  --sidebar: ${palette.sidebar};
  --card: ${glass.fillCard};
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
.dot.na { background: transparent; border: 1px solid var(--stroke-strong); }
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
