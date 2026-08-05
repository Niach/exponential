// EXP-423 — the 12 status glyphs as CSS `mask-image` values, so a pure
// pseudo-element can paint them.
//
// The `#IDENTIFIER` pills are ProseMirror inline DECORATIONS: the document
// text stays the bare token, and everything the chip shows beyond it (title,
// now the status glyph) rides an attribute rendered by CSS. A React icon
// component cannot live inside a `::before`, so the geometry is mirrored here
// as raw SVG and tinted through `--issue-ref-status-color` via the mask's
// alpha.
//
// The bodies below are hand-mirrored from the icon registry — the same
// "shared literals, lock-tested" pattern the four clients' status icons
// already use. `status-icon-svg.test.ts` renders `ICON_COMPONENTS` and
// asserts every element and attribute here matches, so a registry change that
// leaves this file behind fails the suite instead of shipping a stale glyph.

import type { IconName } from "@exp/icons"

const STATUS_ICON_NAMES = [
  `circle-dashed`,
  `circle`,
  `progress-1-4`,
  `progress-2-4`,
  `progress-3-4`,
  `progress-1-5`,
  `progress-2-5`,
  `progress-3-5`,
  `progress-4-5`,
  `circle-check`,
  `circle-x`,
  `copy`,
] as const satisfies readonly IconName[]

export type StatusIconName = (typeof STATUS_ICON_NAMES)[number]

export const STATUS_ICON_NAME_LIST: readonly StatusIconName[] =
  STATUS_ICON_NAMES

/** Lucide's shipped root attributes — every registry glyph is drawn with them. */
export const STATUS_ICON_SVG_ATTRS = {
  viewBox: `0 0 24 24`,
  fill: `none`,
  stroke: `currentColor`,
  "stroke-width": `2`,
  "stroke-linecap": `round`,
  "stroke-linejoin": `round`,
} as const

/** The child elements of each glyph, in registry order. */
export const STATUS_ICON_BODIES: Record<StatusIconName, string> = {
  "circle-dashed": [
    `<path d="M10.1 2.182a10 10 0 0 1 3.8 0"/>`,
    `<path d="M13.9 21.818a10 10 0 0 1-3.8 0"/>`,
    `<path d="M17.609 3.721a10 10 0 0 1 2.69 2.7"/>`,
    `<path d="M2.182 13.9a10 10 0 0 1 0-3.8"/>`,
    `<path d="M20.279 17.609a10 10 0 0 1-2.7 2.69"/>`,
    `<path d="M21.818 10.1a10 10 0 0 1 0 3.8"/>`,
    `<path d="M3.721 6.391a10 10 0 0 1 2.7-2.69"/>`,
    `<path d="M6.391 20.279a10 10 0 0 1-2.69-2.7"/>`,
  ].join(``),
  circle: `<circle cx="12" cy="12" r="10"/>`,
  // The pie clocks: the ring plus a filled wedge. `fill="currentColor"` is
  // what the registry declares; under a mask only the alpha matters, so the
  // wedge reads as solid either way.
  "progress-1-4": `<circle cx="12" cy="12" r="10"/><path d="M12 12 L12 6 A6 6 0 0 1 18 12 Z" fill="currentColor" stroke="none"/>`,
  "progress-2-4": `<circle cx="12" cy="12" r="10"/><path d="M12 12 L12 6 A6 6 0 0 1 12 18 Z" fill="currentColor" stroke="none"/>`,
  "progress-3-4": `<circle cx="12" cy="12" r="10"/><path d="M12 12 L12 6 A6 6 0 1 1 6 12 Z" fill="currentColor" stroke="none"/>`,
  "progress-1-5": `<circle cx="12" cy="12" r="10"/><path d="M12 12 L12 6 A6 6 0 0 1 17.7063 10.1459 Z" fill="currentColor" stroke="none"/>`,
  "progress-2-5": `<circle cx="12" cy="12" r="10"/><path d="M12 12 L12 6 A6 6 0 0 1 15.5267 16.8541 Z" fill="currentColor" stroke="none"/>`,
  "progress-3-5": `<circle cx="12" cy="12" r="10"/><path d="M12 12 L12 6 A6 6 0 1 1 8.4733 16.8541 Z" fill="currentColor" stroke="none"/>`,
  "progress-4-5": `<circle cx="12" cy="12" r="10"/><path d="M12 12 L12 6 A6 6 0 1 1 6.2937 10.1459 Z" fill="currentColor" stroke="none"/>`,
  "circle-check": `<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>`,
  "circle-x": `<circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/>`,
  copy: `<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>`,
}

/** The full SVG document for a glyph (also what the lock test parses). */
export function statusIconSvg(name: StatusIconName): string {
  const attrs = Object.entries(STATUS_ICON_SVG_ATTRS)
    .map(([key, value]) => `${key}="${value}"`)
    .join(` `)
  return `<svg xmlns="http://www.w3.org/2000/svg" ${attrs}>${STATUS_ICON_BODIES[name]}</svg>`
}

function isStatusIconName(name: string): name is StatusIconName {
  return name in STATUS_ICON_BODIES
}

/**
 * Each glyph as a ready-to-use CSS `url(…)` value for `mask-image`. Percent
 * encoding is not optional: the value travels inside a `style` attribute, and
 * a raw `#` or `;` would end the declaration.
 *
 * Built once — the decoration pass runs per keystroke over every pill in the
 * document, so it must be a lookup, not an encode.
 */
export const STATUS_ICON_DATA_URIS: Record<StatusIconName, string> =
  Object.fromEntries(
    STATUS_ICON_NAMES.map((name) => [
      name,
      `url("data:image/svg+xml,${encodeURIComponent(statusIconSvg(name))}")`,
    ])
  ) as Record<StatusIconName, string>

/**
 * The mask value for a status glyph. Unknown names fall back to
 * `circle-dashed` — the same never-fails rule the rest of the status chain
 * follows (lib/team-statuses.ts).
 */
export function statusIconDataUri(name: IconName | string): string {
  return STATUS_ICON_DATA_URIS[isStatusIconName(name) ? name : `circle-dashed`]
}
