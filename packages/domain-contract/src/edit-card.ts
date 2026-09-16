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
// - the row's id is its FIRST item's id (stable while a live run grows); the
//   window `start` (EXP-783) opens a fresh card at its boundary like every
//   other group.
//
// The card (`editCard`):
// - one row per PATH: every member's patch is parsed and folded with
//   `mergeFilesByPath` (a file touched twice = one row, counts summed, hunks
//   concatenated); a pathless patch (bare hunks) borrows its call's `detail`;
// - a member WITHOUT a patch still has a row on its `detail` — `pending`
//   while the call runs, `failed` once it settled without one — unless a
//   patch for that path already exists in the card;
// - order: the ready rows in first-touch order, THEN the pending/failed
//   stubs in first-touch order;
// - `liveIndex` names the row of the card's LAST member when that member is
//   the transcript's live tool row (`liveToolRowId`): the one row a client
//   opens by itself, the diff inline. Everything else starts collapsed, and a
//   tap toggles a row in place — a card never navigates anywhere.

import contractJson from "../contract.json" with { type: "json" }
import { mergeFilesByPath, parseDiff, type DiffFile } from "./diff"

const diffUi = (
  contractJson as unknown as {
    diffUi: {
      editedFilesOne: string
      editedFilesMany: string
      moreFiles: string
      cardPreviewFiles: number
    }
  }
).diffUi

/** The tool kinds whose calls form an edited-files card. */
export const EDIT_CARD_KINDS: readonly string[] = [`edit`, `delete`, `move`]

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

export type EditRowState = `ready` | `pending` | `failed`

export interface EditCardRow {
  path: string
  state: EditRowState
  /** The merged patch for the path; null for a `pending`/`failed` row. */
  file: DiffFile | null
}

export interface EditCardView {
  /** `1 file edited` / `N files edited`. */
  title: string
  rows: EditCardRow[]
  /** The row the client opens by itself, or null. */
  liveIndex: number | null
}

/** The path a member names: its patch's first file, else its `detail`. */
function itemPath(item: EditCardFeedItem): string | null {
  if (item.diff) {
    const first = parseDiff(item.diff).files[0]
    if (first?.path) return first.path
  }
  const detail = item.detail?.trim()
  return detail ? detail : null
}

export function editCard(
  items: readonly EditCardFeedItem[],
  liveItemId: number | null = null
): EditCardView {
  const ready: DiffFile[] = []
  const stubs = new Map<string, EditRowState>()
  for (const item of items) {
    if (item.diff) {
      for (const file of parseDiff(item.diff).files) {
        // A pathless section (hunks with no header) borrows the call's own
        // subject — the engine names the file in `detail`.
        const path = file.path || item.detail?.trim()
        if (!path) continue
        ready.push(file.path === path ? file : { ...file, path })
      }
      continue
    }
    const path = item.detail?.trim()
    if (!path) continue
    const state: EditRowState =
      item.settled === true ? `failed` : `pending`
    // First touch wins the position; a later settle may still flip the
    // state (a pending call that failed without a patch).
    if (!stubs.has(path) || (state === `failed` && stubs.get(path) === `pending`)) {
      stubs.set(path, state)
    }
  }
  const merged = mergeFilesByPath(ready)
  const readyPaths = new Set(merged.map((file) => file.path))
  const rows: EditCardRow[] = [
    ...merged.map((file) => ({ path: file.path, state: `ready` as const, file })),
    ...[...stubs]
      .filter(([path]) => !readyPaths.has(path))
      .map(([path, state]) => ({ path, state, file: null })),
  ]
  const last = items[items.length - 1]
  let liveIndex: number | null = null
  if (last && liveItemId !== null && liveItemId === last.id) {
    const path = itemPath(last)
    const at = path === null ? -1 : rows.findIndex((row) => row.path === path)
    liveIndex = at < 0 ? null : at
  }
  return { title: editCardTitle(rows.length), rows, liveIndex }
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
 * path failed | live=path`. Deliberately ASCII (`-d`, unlike
 * `deletionsLabel`), like `renderDiff`.
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
  return parts.join(` | `)
}
