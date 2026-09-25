/**
 * EXP-1029 contract — the styleguide's four ordered sections.
 *
 * `sections.json` is THE index: the web page (this module) and the IDE
 * styleguide (`apps/desktop/crates/ui/src/styleguide`) both read it, so the
 * two can never disagree on what exists. Every entry id here names one
 * placeholder file in `../entries/` (`entries/index.ts` imports them by
 * fixed name), pre-registered so a leaf only FILLS its file and never
 * touches this index or that one. Ownership rides in `owners`.
 *
 * EXP-1019 owns the structure from here on: it moves the EXISTING
 * `COMPONENTS` entries into the four sections and wires `render.ts` to draw
 * sections instead of the three modes. `sections.test.ts` gates the skeleton
 * (order, titles, files, owners, no id collisions).
 */

import index from "./sections.json"

export type SectionId = `style` | `general` | `special` | `views`

export interface StyleguideSection {
  id: SectionId
  /** 1..4 — the page order. */
  order: number
  /** The numbered heading, `1 Style`. */
  title: string
  blurb: string
  /** Entry ids in page order. */
  entries: readonly string[]
}

interface SectionIndex {
  sections: StyleguideSection[]
  owners: Record<string, string>
}

const parsed = index as unknown as SectionIndex

/** The four sections, in page order. */
export const SECTIONS: readonly StyleguideSection[] = [...parsed.sections].sort(
  (a, b) => a.order - b.order
)

/** Entry id → the issue that fills it. */
export const ENTRY_OWNERS: Readonly<Record<string, string>> = parsed.owners

/** Every registered entry id, in page order. */
export const SECTION_ENTRY_IDS: readonly string[] = SECTIONS.flatMap(
  (section) => section.entries
)

export function sectionOf(entryId: string): StyleguideSection | undefined {
  return SECTIONS.find((section) => section.entries.includes(entryId))
}
