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
export const svgEllipsis = glyph(
  `<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>`
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
export const svgImage = glyph(
  `<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>`
)
export const svgPaperclip = glyph(
  `<path d="M21.44 11.05 12.25 20.24a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>`
)
export const svgHash = glyph(
  `<path d="M4 9h16"/><path d="M4 15h16"/><path d="M10 3 8 21"/><path d="M16 3l-2 18"/>`
)
export const svgSmile = glyph(
  `<circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><path d="M9 9h.01"/><path d="M15 9h.01"/>`
)
export const svgSend = glyph(
  `<path d="M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z"/><path d="m21.854 2.147-10.94 10.939"/>`
)
export const svgArrowUpRight = glyph(`<path d="M7 7h10v10"/><path d="M7 17 17 7"/>`)
export const svgChevronDown = glyph(`<path d="m6 9 6 6 6-6"/>`)
export const svgChevronUp = glyph(`<path d="m18 15-6-6-6 6"/>`)
export const svgMessageCircle = glyph(`<path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/>`)
export const svgX = glyph(`<path d="M18 6 6 18"/><path d="m6 6 12 12"/>`)
export const svgCheck = glyph(`<path d="M20 6 9 17l-5-5"/>`)
export const svgCircleHelp = glyph(
  `<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/>`
)
