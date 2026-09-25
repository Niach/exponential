/**
 * The two things both halves of the page need: escaping, and a handful of
 * inline glyphs.
 *
 * There is deliberately NO icon library here. The gallery is ONE self-contained
 * HTML file that has to work over `file://` with zero runtime dependencies, so
 * the few paths the component demos need are literals — copied from the same
 * Lucide set `packages/icons` generates from, which is what the product ships.
 */

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
}

/** Lucide's stroke geometry, verbatim — 24-box, round caps, currentColor. */
function glyph(body: string): string {
  return [
    `<svg class="glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor"`,
    ` stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">`,
    body,
    `</svg>`,
  ].join(``)
}

export const svgChevronRight = glyph(`<path d="m9 6 6 6-6 6"/>`)
export const svgChevronDown = glyph(`<path d="m6 9 6 6 6-6"/>`)
/** The `nav-workflows` concept (lucide `workflow`) and `pr-stack` (`layers`) —
 *  the two group rows of the session tree (EXP-996). */
export const svgWorkflow = glyph(
  `<rect width="8" height="8" x="3" y="3" rx="2"/><path d="M7 11v4a2 2 0 0 0 2 2h4"/><rect width="8" height="8" x="13" y="13" rx="2"/>`
)
export const svgLayers = glyph(
  [
    `<path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z"/>`,
    `<path d="M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12"/>`,
    `<path d="M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17"/>`,
  ].join(``)
)
export const svgPlay = glyph(`<polygon points="6 3 20 12 6 21 6 3"/>`)
export const svgPlus = glyph(`<path d="M5 12h14"/><path d="M12 5v14"/>`)
export const svgGitMerge = glyph(
  `<circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M6 21V9a9 9 0 0 0 9 9"/>`
)
export const svgInbox = glyph(
  `<polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>`
)
export const svgBell = glyph(
  `<path d="M10.268 21a2 2 0 0 0 3.464 0"/><path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>`
)
export const svgTrash = glyph(
  `<path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>`
)
export const svgListTodo = glyph(
  `<rect x="3" y="5" width="6" height="6" rx="1"/><path d="m3 17 2 2 4-4"/><path d="M13 6h8"/><path d="M13 12h8"/><path d="M13 18h8"/>`
)
export const svgFlag = glyph(
  `<path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><line x1="4" x2="4" y1="22" y2="15"/>`
)
export const svgTag = glyph(
  `<path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z"/><path d="M7.5 7.5h.01"/>`
)
export const svgCircleUser = glyph(
  `<circle cx="12" cy="12" r="10"/><circle cx="12" cy="10" r="3"/><path d="M7 20.662V19a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v1.662"/>`
)
export const svgTerminal = glyph(`<polyline points="4 17 10 11 4 5"/><path d="M12 19h8"/>`)
export const svgPaperclip = glyph(
  `<path d="M21.44 11.05 12.25 20.24a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>`
)
export const svgMessageCircle = glyph(`<path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/>`)
export const svgX = glyph(`<path d="M18 6 6 18"/><path d="m6 6 12 12"/>`)
export const svgCheck = glyph(`<path d="M20 6 9 17l-5-5"/>`)
export const svgCircleHelp = glyph(
  `<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/>`
)
/** Lucide `github` — the GitHub connection block and the repo picker rows. */
export const svgGithub = glyph(
  `<path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/>`
)
export const svgExternalLink = glyph(
  `<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>`
)
export const svgRefresh = glyph(
  `<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>`
)
export const svgBuilding = glyph(
  `<path d="M6 22V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v18Z"/><path d="M6 12H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2"/><path d="M18 9h2a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-2"/><path d="M10 6h4"/><path d="M10 10h4"/><path d="M10 14h4"/><path d="M10 18h4"/>`
)
export const svgUser = glyph(
  `<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>`
)
export const svgLock = glyph(
  `<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>`
)
export const svgTriangleAlert = glyph(
  `<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>`
)

/** `editor-issue-ref` — the issue picker's glyph (Lucide `hash`). */
export const svgHash = glyph(
  `<line x1="4" x2="20" y1="9" y2="9"/><line x1="4" x2="20" y1="15" y2="15"/><line x1="10" x2="8" y1="3" y2="21"/><line x1="16" x2="14" y1="3" y2="21"/>`
)

/** `ui-submit` — the round send on every composer (Lucide `circle-arrow-up`). */
export const svgCircleArrowUp = glyph(
  `<circle cx="12" cy="12" r="10"/><path d="m16 12-4-4-4 4"/><path d="M12 16V8"/>`
)
