// EXP-850 §12: the per-turn file card. DERIVED, never on the wire — for each
// assistant turn segment (a `user_message`, or the start of the run, up to the
// next `user_message` or the end of the feed) the `edit`/`delete`/`move` tool
// rows that SETTLED with a diff become one card: "3 files edited", the paths
// with their `+a -d`, and a click that opens the diff pane at that file.
// Hand-mirrored on the desktop (`crates/ui` session screen).
import {
  mergeFilesByPath,
  parseDiff,
  type DiffFile,
} from "@exp/domain-contract/diff"
import type { ToolKind } from "@/lib/agent-feed"

/** The tool kinds whose settled diff counts as a file edit. */
export const FILE_CARD_KINDS: readonly ToolKind[] = [`edit`, `delete`, `move`]

/** How many paths a card lists before it folds the rest behind "N more". */
export const FILE_CARD_PREVIEW = 5

/** One row of a card IS a `DiffFile` (EXP-895): the row carries the per-call
 *  hunks of the call(s) that wrote it, so a click can open the Changes face
 *  SCOPED to this turn without going back to the whole-branch diff. A file
 *  written twice in the segment keeps both hunks (`mergeFilesByPath`). */
export type SessionFileEntry = DiffFile

/** One card: the files a turn segment touched, anchored AFTER the feed row
 *  that closed the segment. */
export interface SessionFileCard {
  /** The id of the row that OPENED the segment (its `user_message`, or the
   *  run's first row) — the card's STABLE identity. `afterId` moves with
   *  every row a live turn adds, so a diff scope keyed on it would evaporate
   *  mid-turn; this one names the turn for as long as the turn exists. */
  turnId: number
  /** The id of the last feed row of the segment — the card renders behind it. */
  afterId: number
  files: DiffFile[]
}

/** A row the derivation reads: a feed item, narrowed to what it needs. */
export interface FileCardFeedItem {
  id: number
  kind: string
  toolKind?: ToolKind
  settled?: boolean
  diff?: string
  detail?: string
  subagentId?: string
}

/** The files a per-call patch touches. The engine's per-call diff is a BARE
 *  unified diff (`--- a/path` / `+++ b/path`, no `diff --git` header) while the
 *  session diff on the "Changes" wire is full `git diff` output — the shared
 *  parser auto-detects both, and strips the publisher's truncation marker. */
export function toolDiffFiles(diff: string): DiffFile[] {
  return parseDiff(diff).files
}

/** EXP-862: whether a turn-scoped Changes face may widen BACK to the whole run
 *  — only once the run has published a session diff. Until then the turn's
 *  files are everything there is, and offering the chip would blank the face
 *  on click. */
export function canWidenDiffScope(sessionFileCount: number): boolean {
  return sessionFileCount > 0
}

/** The cards for a whole feed, in transcript order. A segment with no settled
 *  edit yields no card; a file edited twice in one segment is ONE row with
 *  both patches' counts summed. Subagent rows are skipped — their work
 *  belongs to their own card inside the group, not to the main transcript. */
export function sessionFileCards(
  feed: readonly FileCardFeedItem[]
): SessionFileCard[] {
  const cards: SessionFileCard[] = []
  let files: DiffFile[] = []
  let lastId: number | null = null
  let turnId: number | null = null
  const close = () => {
    if (lastId !== null && turnId !== null && files.length > 0) {
      // ONE row per path, hunks concatenated in arrival order — the contract's
      // own fold, not a string concatenation of two patches.
      cards.push({ turnId, afterId: lastId, files: mergeFilesByPath(files) })
    }
    files = []
  }
  for (const item of feed) {
    if (item.subagentId !== undefined) continue
    if (item.kind === `user_message`) {
      close()
      lastId = item.id
      turnId = item.id
      continue
    }
    lastId = item.id
    if (turnId === null) turnId = item.id
    if (item.kind !== `tool`) continue
    if (item.settled !== true || !item.diff) continue
    if (
      item.toolKind === undefined ||
      !FILE_CARD_KINDS.includes(item.toolKind)
    ) {
      continue
    }
    for (const file of toolDiffFiles(item.diff)) {
      // A pathless section (hunks with no header) borrows the tool row's own
      // subject — the engine names the file in `detail`.
      const path = file.path || item.detail?.trim()
      if (!path) continue
      files.push(file.path === path ? file : { ...file, path })
    }
  }
  close()
  return cards
}

/** The card's title — `1 file edited` / `4 files edited`, ×4. */
export function fileCardTitle(count: number): string {
  return `${count} ${count === 1 ? `file` : `files`} edited`
}

/** The pane's scope chip while it shows one turn — `This turn: 3 files`, ×4. */
export function diffScopeTurnLabel(count: number): string {
  return `This turn: ${count} ${count === 1 ? `file` : `files`}`
}

/** What the chip DOES: back to the whole session's changes, ×4. */
export const DIFF_SCOPE_ALL_LABEL = `Show all changes`

/** The fold row under the first `FILE_CARD_PREVIEW` paths, or null. */
export function fileCardMoreLabel(count: number): string | null {
  const rest = count - FILE_CARD_PREVIEW
  return rest > 0 ? `${rest} more` : null
}
