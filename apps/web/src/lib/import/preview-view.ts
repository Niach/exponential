// EXP-1076: the wizard's read model over a preview — pure, client-side, no
// tRPC and no database.
//
// A Linear workspace shows every status, label and person of EVERY team; an
// import that only takes one team should not ask the operator to map the
// other's. These helpers narrow the preview to what the plan's non-skipped
// boards actually carry (the `*ByBoard` relevance maps `previewFromBundle`
// writes) and collapse the statuses several teams share into ONE row: Linear
// gives each team its own "Todo", Exponential has one per team.
//
// Old previews (stored before the maps existed) carry no map at all — every
// helper then shows the row, so a resumed job never loses its mapping UI.
import {
  issueStatusCategoryDisplayOrder,
  type IssueStatusCategory,
} from "@exp/db-schema/domain"
import type { ImportPlan, ImportPreview } from "@/lib/import/bundle"

/**
 * The board keys whose content the plan still imports: every `plan.boards`
 * entry that is not `skip`, with a PROJECT key (project routing) resolved to
 * the team board key the preview's counts are keyed by — the preview is
 * always built team-routed. `importArchived: false` drops the archive boards.
 */
export function visibleBoardKeys(preview: ImportPreview, plan: ImportPlan): Set<string> {
  const teamByProject = new Map<string, string | null>(
    preview.projects.map((project) => [project.key, project.teamKey ?? null])
  )
  const visible = new Set<string>()
  for (const [key, entry] of Object.entries(plan.boards)) {
    if (!entry || entry.mode === `skip`) continue
    if (plan.importArchived === false && key.startsWith(`archive:`)) continue
    if (teamByProject.has(key)) {
      // Project boards only carry anything under project routing; the plan
      // keeps its (unused) entries for the other one. Their content is
      // counted under the project's TEAM, which is what the maps name.
      if (plan.routing !== `project`) continue
      const teamKey = teamByProject.get(key)
      visible.add(teamKey ?? key)
      continue
    }
    visible.add(key)
  }
  return visible
}

/**
 * Does a relevance map have any content left on the visible boards? An absent
 * or empty map means "we do not know" — the row stays.
 */
export function isVisible(
  counts: Record<string, number> | undefined,
  visible: ReadonlySet<string>
): boolean {
  if (!counts) return true
  // A map that IS there but empty means "referenced by nothing": the source
  // team's unused default state, hidden even before any board is skipped.
  const entries = Object.entries(counts)
  for (const [boardKey, count] of entries) {
    if (count > 0 && visible.has(boardKey)) return true
  }
  return false
}

export interface PreviewStatusGroup {
  // `${normalized name}|${category}` — stable across renders, the plan writes
  // the same decision to every key in the group.
  id: string
  name: string
  category: IssueStatusCategory
  color: string
  // Every preview status key this row stands for, in preview order.
  keys: string[]
  issueCount: number
  // The source teams the collapsed statuses came from, deduped.
  teamNames: string[]
}

function sumOverVisible(
  counts: Record<string, number> | undefined,
  visible: ReadonlySet<string>,
  fallback: number
): number {
  if (!counts) return fallback
  const entries = Object.entries(counts)
  let total = 0
  for (const [boardKey, count] of entries) {
    if (visible.has(boardKey)) total += count
  }
  return total
}

/**
 * The statuses the Map step shows: the ones with content on a visible board,
 * collapsed by (name, category) so one decision covers every source team that
 * spells it the same way. Ordered by category (the ONE display order, EXP-448)
 * then name.
 */
export function groupPreviewStatuses(
  statuses: ImportPreview[`statuses`],
  visible: ReadonlySet<string>,
  teamNameByKey: ReadonlyMap<string, string>
): PreviewStatusGroup[] {
  const groups = new Map<string, PreviewStatusGroup>()
  // Every same-named key joins its group (the decision must reach the keys
  // only history events reference), but a group shows only when one of its
  // members is referenced on a visible board, and the hint names only those.
  const shown = new Set<string>()
  for (const status of statuses) {
    const memberVisible = isVisible(status.issueCountByBoard, visible)
    const id = `${status.name.trim().toLowerCase()}|${status.category}`
    let group = groups.get(id)
    if (!group) {
      group = {
        id,
        name: status.name,
        category: status.category,
        color: status.color,
        keys: [],
        issueCount: 0,
        teamNames: [],
      }
      groups.set(id, group)
    }
    group.keys.push(status.key)
    group.issueCount += sumOverVisible(
      status.issueCountByBoard,
      visible,
      status.issueCount
    )
    if (!memberVisible) continue
    shown.add(id)
    const teamName = status.teamKey ? teamNameByKey.get(status.teamKey) : undefined
    if (teamName && !group.teamNames.includes(teamName)) group.teamNames.push(teamName)
  }
  return [...groups.values()].filter((group) => shown.has(group.id)).sort((left, right) => {
    const order =
      issueStatusCategoryDisplayOrder.indexOf(left.category) -
      issueStatusCategoryDisplayOrder.indexOf(right.category)
    if (order !== 0) return order
    return left.name.localeCompare(right.name)
  })
}

export function visiblePreviewLabels(
  labels: ImportPreview[`labels`],
  visible: ReadonlySet<string>
): ImportPreview[`labels`] {
  return labels.filter((label) => isVisible(label.issueCountByBoard, visible))
}

/**
 * The people worth mapping: anyone who assigned, created or commented on
 * something that is still being imported.
 */
export function visiblePreviewUsers(
  users: ImportPreview[`users`],
  visible: ReadonlySet<string>
): ImportPreview[`users`] {
  return users.filter((user) => {
    // An empty map says "no content of this kind", an absent one "unknown" —
    // both drop out of the decision. Nothing known at all (an old preview, a
    // person the source lists without any content) leaves the row visible.
    const known = [user.issueCountByBoard, user.commentCountByBoard].filter(
      (counts): counts is Record<string, number> =>
        counts !== undefined && Object.keys(counts).length > 0
    )
    if (known.length === 0) return true
    return known.some((counts) => isVisible(counts, visible))
  })
}
