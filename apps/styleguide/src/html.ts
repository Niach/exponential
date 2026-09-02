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
