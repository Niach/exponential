// EXP-314 — category → glyph resolution for issue statuses, including the
// Linear-style pie clocks for the `started` category: with N started statuses
// the i-th (0-based) renders a clock filled by position — N≤2 → [2/4, 3/4]
// (the builtin In Progress / In Review pair keeps today's half + ¾ look),
// N=3 → [1/4, 2/4, 3/4], N≥4 → [1/5..4/5] (the server caps N at
// ISSUE_STATUS_STARTED_MAX = 4; a transiently over-cap team clamps).
//
// This table is the CROSS-PLATFORM parity contract — iOS
// (IssueStatusResolution.swift), Android (IssueStatusResolution.kt) and
// desktop (domain/src/statuses.rs) mirror it byte-for-byte, locked by each
// platform's unit tests (the IssueSorting precedent: no shared code, shared
// literals).

import type { IconName } from "@exp/icons"
import type { IssueStatusCategory } from "@exp/db-schema/domain"

const CLOCKS_2: IconName[] = [`progress-2-4`, `progress-3-4`]
const CLOCKS_3: IconName[] = [`progress-1-4`, `progress-2-4`, `progress-3-4`]
const CLOCKS_4: IconName[] = [
  `progress-1-5`,
  `progress-2-5`,
  `progress-3-5`,
  `progress-4-5`,
]

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

export function startedClockIcon(index0: number, count: number): IconName {
  const table = count <= 2 ? CLOCKS_2 : count === 3 ? CLOCKS_3 : CLOCKS_4
  return table[clamp(index0, 0, table.length - 1)]
}

// The non-started categories each have ONE fixed glyph; started picks its
// clock from the status's position among the team's started statuses.
export function categoryStatusIcon(
  category: IssueStatusCategory,
  startedIndex0: number,
  startedCount: number
): IconName {
  switch (category) {
    case `backlog`:
      return `circle-dashed`
    case `unstarted`:
      return `circle`
    case `started`:
      return startedClockIcon(startedIndex0, startedCount)
    case `completed`:
      return `circle-check`
    case `cancelled`:
      return `circle-x`
    case `duplicate`:
      return `copy`
  }
}

// Group-header wash for custom-status hexes — the inline-style analog of the
// `/10` Tailwind classes the builtin token washes use.
export function hexWithAlpha(hex: string, alpha: number): string {
  const r = Number.parseInt(hex.slice(1, 3), 16)
  const g = Number.parseInt(hex.slice(3, 5), 16)
  const b = Number.parseInt(hex.slice(5, 7), 16)
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}
