// EXP-916: the ONE "edited files" card a transcript draws for a run of
// consecutive file edits, and the ONE rule that decides which tool calls form
// it. Hand-mirrored ×4 (web `lib/agent-feed.ts` + `@exp/ui`
// `EditedFilesCard`, desktop `steer::feed` + `domain::edit_card`, iOS
// `ExpCore/Sources/Domain/EditCard.swift`, Android `domain/EditCard.kt`) and
// byte-locked by `fixtures/feed/edit-cards.json`, which every client's feed
// test replays through its own `groupFeedRows`.
//
// Grouping (a render ROW of the feed projection, kind `edits`):
// - an `edits` row is a MAXIMAL run of consecutive feed items of ONE lane
//   (`subagentId`, undefined = the main lane) that are all edit calls: kind
//   `tool`, `toolKind` ∈ `edit|delete|move`, no `workflowId` (EXP-850 §3 keeps
//   a workflow call as its own card);
// - the rule reads ONLY `kind`/`toolKind`/`workflowId`/`subagentId` — never
//   `settled`/`failed`/`diff` — so the card exists before any patch lands and
//   a later `tool_update` can never re-split it;
// - ANY other item ends the run: narration, a user turn, a question, a tool of
//   another kind, a workflow call, another lane's item. "Nothing between" is
//   literal;
// - a run whose card lists NO file (`editCard` dropped every member: none has
//   a patch or a `detail`, EXP-938) emits no row at all, never "0 files
//   edited";
// - the row's id is its FIRST item's id (stable while a live run grows); the
//   window `start` (EXP-783) opens a fresh card at its boundary like every
//   other group.
//
// The card (`editCard`):
// - one row per PATH: every member's patch is parsed and folded with
//   `mergeFilesByPath` (a file touched twice = one row, counts summed, hunks
//   concatenated); a pathless patch (bare hunks) borrows its call's `detail`;
// - a member WITHOUT a patch still has a row on its `detail` — `pending`
//   while the call runs, `failed` when the call's own `failed` flag is set,
//   `done` once it settled without either (a delete, a move, an edit that
//   changed nothing: the wire carries no patch for those) — unless a patch
//   for that path already exists in the card;
// - order: the ready rows in first-touch order, THEN the stubs in
//   first-touch order; a later settle may upgrade a stub (pending → done /
//   failed) but never moves it;
// - `truncatedLines` = the lines the publisher cut off the members' patches
//   (EXP-786 markers), summed, so the card can say what it is not showing;
// - `liveIndex` names the row of the card's LAST member when that member is
//   the transcript's live tool row (`liveToolRowId`): the one row a client
//   opens by itself, the diff inline. Everything else starts collapsed, and a
//   tap toggles a row in place — a card never navigates anywhere.

import contractJson from "../contract.json" with { type: "json" }
import { DIFF_LINE_MAX, mergeFilesByPath, parseDiff, type DiffFile } from "./diff"

const json = contractJson as unknown as {
  diffUi: {
    editedFilesOne: string
    editedFilesMany: string
    moreFiles: string
    cardPreviewFiles: number
  }
  toolKind: { editKinds: string[] }
}
const diffUi = json.diffUi

/** The tool kinds whose calls form an edited-files card — the contract's
 *  `toolKind.editKinds`, generated ×4 so no mirror restates the list. */
export const EDIT_CARD_KINDS: readonly string[] = json.toolKind.editKinds

/** How many rows a card lists before it folds the rest behind "N more". */
export const EDIT_CARD_PREVIEW: number = diffUi.cardPreviewFiles

/** A feed item, narrowed to what the rule and the card read. */
export interface EditCardFeedItem {
  id: number
  kind: string
  toolKind?: string
  workflowId?: string
  subagentId?: string
  detail?: string
  diff?: string
  settled?: boolean
  failed?: boolean
}

/** Whether a feed item is a call that belongs in an edited-files card. */
export function isEditCall(
  item: Pick<EditCardFeedItem, `kind` | `toolKind` | `workflowId`>
): boolean {
  return (
    item.kind === `tool` &&
    item.workflowId === undefined &&
    item.toolKind !== undefined &&
    EDIT_CARD_KINDS.includes(item.toolKind)
  )
}

/**
 * The inclusive end index of the maximal run of same-lane edit calls that
 * starts at `start` (which must itself be an edit call). A caller's group
 * scan uses it exactly like its tool-run scan.
 */
export function editRunEnd<
  T extends Pick<EditCardFeedItem, `kind` | `toolKind` | `workflowId` | `subagentId`>,
>(feed: readonly T[], start: number): number {
  const lane = feed[start]?.subagentId
  let end = start
  while (
    end + 1 < feed.length &&
    isEditCall(feed[end + 1]) &&
    feed[end + 1].subagentId === lane
  ) {
    end++
  }
  return end
}

export type EditRowState = `ready` | `pending` | `done` | `failed`

export interface EditCardRow {
  path: string
  state: EditRowState
  /** The merged patch for the path; null for a `pending`/`done`/`failed` row. */
  file: DiffFile | null
}

export interface EditCardView {
  /** `1 file edited` / `N files edited`. */
  title: string
  rows: EditCardRow[]
  /** The row the client opens by itself, or null. */
  liveIndex: number | null
  /** Lines the publisher cut off the members' patches, summed (0 = whole). */
  truncatedLines: number
}

export function editCard(
  items: readonly EditCardFeedItem[],
  liveItemId: number | null = null
): EditCardView {
  const ready: DiffFile[] = []
  const stubs = new Map<string, EditRowState>()
  let truncatedLines = 0
  const last = items[items.length - 1]
  // The path the LAST member names — its patch's first file, else its
  // `detail` — recorded while its patch is parsed once, never re-parsed.
  let lastPath: string | null = null
  for (let i = 0; i < items.length; i++) {
    const item = items[i]
    // The LAST member by INDEX, never by identity: one item object may sit in
    // the run twice (a re-render reuses rows), and `item === last` would then
    // name the wrong member the live one.
    const isLast = i === items.length - 1
    if (item.diff) {
      const parsed = parseDiff(item.diff)
      // Every marker is already clamped to `DIFF_LINE_MAX`, so the SUM must
      // saturate there too — the count is a caption, not an accumulator.
      truncatedLines = Math.min(truncatedLines + (parsed.truncatedLines ?? 0), DIFF_LINE_MAX)
      for (const file of parsed.files) {
        // A pathless section (hunks with no header) borrows the call's own
        // subject — the engine names the file in `detail`.
        const path = file.path || item.detail?.trim()
        if (!path) continue
        if (isLast && lastPath === null) lastPath = path
        ready.push(file.path === path ? file : { ...file, path })
      }
      if (isLast && lastPath === null) lastPath = item.detail?.trim() || null
      continue
    }
    const path = item.detail?.trim()
    if (isLast) lastPath = path || null
    if (!path) continue
    const state: EditRowState =
      item.failed === true ? `failed` : item.settled === true ? `done` : `pending`
    // First touch wins the position; a later settle upgrades a pending stub
    // (pending → done / failed) and never demotes a settled one.
    const held = stubs.get(path)
    if (held === undefined || held === `pending`) stubs.set(path, state)
  }
  const merged = mergeFilesByPath(ready)
  const readyPaths = new Set(merged.map((file) => file.path))
  const rows: EditCardRow[] = [
    ...merged.map((file) => ({ path: file.path, state: `ready` as const, file })),
    ...[...stubs]
      .filter(([path]) => !readyPaths.has(path))
      .map(([path, state]) => ({ path, state, file: null })),
  ]
  let liveIndex: number | null = null
  if (last && liveItemId !== null && liveItemId === last.id && lastPath !== null) {
    const at = rows.findIndex((row) => row.path === lastPath)
    liveIndex = at < 0 ? null : at
  }
  return { title: editCardTitle(rows.length), rows, liveIndex, truncatedLines }
}

/** The card's title — `1 file edited` / `4 files edited`, ×4. */
export function editCardTitle(count: number): string {
  return count === 1
    ? diffUi.editedFilesOne
    : diffUi.editedFilesMany.replace(`{n}`, String(count))
}

/** The fold row under the first `EDIT_CARD_PREVIEW` rows, or null. */
export function editCardMoreLabel(count: number): string | null {
  const rest = count - EDIT_CARD_PREVIEW
  return rest > 0 ? diffUi.moreFiles.replace(`{n}`, String(rest)) : null
}

/**
 * The byte-lock projection of a card: `title | path +a -d | path pending |
 * path done | path failed | live=path | truncated=N`. Deliberately ASCII
 * (`-d`, unlike `deletionsLabel`), like `renderDiff`.
 */
export function renderEditCard(view: EditCardView): string {
  const parts = [view.title]
  for (const row of view.rows) {
    parts.push(
      row.state === `ready` && row.file
        ? `${row.path} +${row.file.additions} -${row.file.deletions}`
        : `${row.path} ${row.state}`
    )
  }
  if (view.liveIndex !== null && view.rows[view.liveIndex]) {
    parts.push(`live=${view.rows[view.liveIndex].path}`)
  }
  if (view.truncatedLines > 0) parts.push(`truncated=${view.truncatedLines}`)
  return parts.join(` | `)
}
