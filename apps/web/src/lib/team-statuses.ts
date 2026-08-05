// EXP-314 — the web half of the CROSS-PLATFORM status resolution contract.
// iOS (IssueStatusResolution.swift), Android (IssueStatusResolution.kt) and
// desktop (domain/src/statuses.rs) mirror this file's three rules
// byte-for-byte:
//
//  1. teamStatuses order: category `issueStatusCategoryDisplayOrder`
//     (started, unstarted, backlog, completed, cancelled, duplicate), then
//     `sortOrder` asc, then `createdAt` asc, then `id` — a started row's clock
//     position is its index among the started rows in THAT order.
//  2. resolve(issue): the row whose id === issue.statusId, else the row whose
//     builtinKey === issue.status (the dual-written anchor), else a locally
//     CONSTRUCTED default from the contract defaults (synthetic id
//     `builtin:<key>`). Rendering can never fail.
//  3. Colors: builtin rows (and constructed fallbacks) render each platform's
//     LEGACY token colors keyed on the builtin key — byte-identical to
//     pre-EXP-314 rendering. Only custom rows render their synced hex.
//
// Everything here is pure; the React surface lives in
// hooks/use-team-statuses.tsx.

import type { IconName } from "@exp/icons"
import type { IssueStatusRow } from "@/db/schema"
import {
  BUILTIN_STATUS_DEFAULTS,
  ISSUE_STATUS_FALLBACK,
  issueStatusCategoryDisplayOrder,
  issueStatusCategorySettingsOrder,
  issueStatusValues,
  type IssueStatus,
  type IssueStatusCategory,
} from "@/lib/domain"
import { categoryStatusIcon } from "@/lib/status-icons"

// Synthetic id prefix for the CONSTRUCTED fallback rows used before the
// `issue_statuses` shape has synced (the builtin-actions precedent). These ids
// never reach the server: `statusUpdatePayload` sends the anchor enum instead.
export const FALLBACK_STATUS_ID_PREFIX = `builtin:`

export interface StatusRowOption {
  id: string
  name: string
  colorHex: string
  category: IssueStatusCategory
  builtinKey: IssueStatus | null
  sortOrder: number
  icon: IconName
}

// The minimal issue shape resolution needs — `Issue` satisfies it, and so do
// search results and optimistic upserts.
export interface StatusResolvable {
  status: IssueStatus | string
  statusId: string | null
}

// The subset of an `issue_statuses` row the pure builder needs.
export type StatusRowInput = Pick<
  IssueStatusRow,
  `id` | `name` | `color` | `category` | `builtinKey` | `sortOrder`
> & { createdAt?: Date | string | null }

const CATEGORY_RANK = new Map<IssueStatusCategory, number>(
  issueStatusCategoryDisplayOrder.map((category, index) => [category, index])
)

function createdAtMs(value: Date | string | null | undefined): number {
  if (value == null) return 0
  if (value instanceof Date) return value.getTime()
  const parsed = new Date(
    value.replace(` `, `T`).replace(/([+-]\d{2})$/, `$1:00`)
  ).getTime()
  return Number.isNaN(parsed) ? 0 : parsed
}

function compareRows(left: StatusRowInput, right: StatusRowInput): number {
  const categoryDiff =
    (CATEGORY_RANK.get(left.category) ?? Number.MAX_SAFE_INTEGER) -
    (CATEGORY_RANK.get(right.category) ?? Number.MAX_SAFE_INTEGER)
  if (categoryDiff !== 0) return categoryDiff

  if (left.sortOrder !== right.sortOrder) {
    return left.sortOrder - right.sortOrder
  }

  const createdDiff = createdAtMs(left.createdAt) - createdAtMs(right.createdAt)
  if (createdDiff !== 0) return createdDiff

  return left.id < right.id ? -1 : left.id > right.id ? 1 : 0
}

/**
 * Order a team's status rows and attach each one's glyph. Started rows get
 * their pie clock from their POSITION among the started rows (see
 * lib/status-icons.ts) — moving a started status re-derives every sibling's
 * clock automatically.
 */
export function buildStatusOptions(
  rows: readonly StatusRowInput[]
): StatusRowOption[] {
  const ordered = [...rows].sort(compareRows)
  const startedCount = ordered.filter(
    (row) => row.category === `started`
  ).length

  let startedIndex = 0
  return ordered.map((row) => {
    const index = row.category === `started` ? startedIndex++ : 0
    return {
      id: row.id,
      name: row.name,
      colorHex: row.color,
      category: row.category,
      builtinKey: row.builtinKey,
      sortOrder: row.sortOrder,
      icon: categoryStatusIcon(row.category, index, startedCount),
    }
  })
}

/**
 * The locally-constructed default set — used while the `issue_statuses` shape
 * hasn't delivered its first snapshot (and by unit tests). Ids are synthetic
 * (`builtin:<key>`); writes from these rows send the anchor enum.
 */
export function fallbackStatusOptions(): StatusRowOption[] {
  return buildStatusOptions(
    BUILTIN_STATUS_DEFAULTS.map((entry, index) => ({
      id: `${FALLBACK_STATUS_ID_PREFIX}${entry.key}`,
      name: entry.name,
      color: entry.color,
      category: entry.category,
      builtinKey: entry.key,
      sortOrder: entry.sortOrder,
      // Deterministic, contract-declaration-ordered tiebreak.
      createdAt: new Date(index),
    }))
  )
}

const FALLBACK_OPTIONS = fallbackStatusOptions()
const FALLBACK_BY_KEY = new Map<string, StatusRowOption>(
  FALLBACK_OPTIONS.filter((option) => option.builtinKey !== null).map(
    (option) => [option.builtinKey as string, option]
  )
)
const FALLBACK_BACKLOG = FALLBACK_BY_KEY.get(ISSUE_STATUS_FALLBACK)!

/** The frozen constructed default set (already ordered + icon-resolved). */
export function defaultStatusOptions(): StatusRowOption[] {
  return FALLBACK_OPTIONS
}

/**
 * The four-client fallback chain: statusId row → anchor-enum row →
 * constructed default → constructed Backlog. Never throws.
 */
export function resolveIssueStatus(
  issue: StatusResolvable,
  options: readonly StatusRowOption[],
  byId?: Map<string, StatusRowOption>
): StatusRowOption {
  if (issue.statusId) {
    const byIdHit = byId
      ? byId.get(issue.statusId)
      : options.find((option) => option.id === issue.statusId)
    if (byIdHit) return byIdHit
  }
  // Unknown forward-compat anchors normalize to backlog BEFORE the row
  // lookup, so such an issue joins the team's REAL Backlog group instead of
  // spawning a second, constructed one (the cross-platform rule — iOS/
  // Android/desktop mirror it).
  const anchor = (issueStatusValues as readonly string[]).includes(issue.status)
    ? issue.status
    : `backlog`
  const anchored = options.find((option) => option.builtinKey === anchor)
  if (anchored) return anchored
  return FALLBACK_BY_KEY.get(anchor) ?? FALLBACK_BACKLOG
}

/** True for a CONSTRUCTED fallback row (its id is not a real row uuid). */
export function isFallbackStatusOption(option: StatusRowOption): boolean {
  return option.id.startsWith(FALLBACK_STATUS_ID_PREFIX)
}

/**
 * The tRPC patch a status picker sends. Real rows write `statusId`; a
 * constructed fallback row (shape not synced yet) writes the anchor `status`
 * enum instead — synthetic `builtin:<key>` ids must never reach the server.
 */
export function statusUpdatePayload(
  option: StatusRowOption
): { statusId: string } | { status: IssueStatus } {
  if (isFallbackStatusOption(option) && option.builtinKey) {
    return { status: option.builtinKey }
  }
  return { statusId: option.id }
}

const SETTINGS_CATEGORY_RANK = new Map<IssueStatusCategory, number>(
  issueStatusCategorySettingsOrder.map((category, index) => [category, index])
)

/**
 * Re-group display-ordered options into the SETTINGS category order (backlog,
 * unstarted, started, completed, cancelled, duplicate) for SET-STATUS pickers
 * only — a picker reads top-down as the issue's life cycle, whereas board
 * grouping keeps `displayOrder` so the work in flight leads the page.
 *
 * The sort is stable, so intra-category order (and with it every started row's
 * baked-in pie-clock position) survives untouched — nothing is recomputed.
 */
export function sortStatusesForPicker(
  options: readonly StatusRowOption[]
): StatusRowOption[] {
  return [...options].sort(
    (left, right) =>
      (SETTINGS_CATEGORY_RANK.get(left.category) ?? Number.MAX_SAFE_INTEGER) -
      (SETTINGS_CATEGORY_RANK.get(right.category) ?? Number.MAX_SAFE_INTEGER)
  )
}

/**
 * Marking a brand-new issue as a duplicate is nonsense (there is nothing yet
 * to dedupe), so create/edit chip rows never offer the duplicate category —
 * it is only reachable via the duplicate-picker interception.
 */
export function creatableStatusOptions(
  options: readonly StatusRowOption[]
): StatusRowOption[] {
  return options.filter((option) => option.category !== `duplicate`)
}

/**
 * The URL filter token a status row contributes. Real rows use their uuid; a
 * CONSTRUCTED fallback row uses its anchor enum — the URL allowlist
 * (`isValidStatusToken`) accepts uuids and enum values only, so a synthetic
 * `builtin:<key>` id must never reach `?status=`.
 */
export function statusFilterToken(option: StatusRowOption): string {
  if (isFallbackStatusOption(option) && option.builtinKey) {
    return option.builtinKey
  }
  return option.id
}

/**
 * Filter-token matching (lib/filters.ts dual-token model): a token is either a
 * status row uuid or a legacy anchor-enum value from an older URL.
 */
export function statusOptionMatchesToken(
  option: StatusRowOption,
  token: string
): boolean {
  return token === option.id || token === option.builtinKey
}
