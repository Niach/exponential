// EXP-850 §12: the per-turn file card. DERIVED, never on the wire — for each
// assistant turn segment (a `user_message`, or the start of the run, up to the
// next `user_message` or the end of the feed) the `edit`/`delete`/`move` tool
// rows that SETTLED with a diff become one card: "3 files edited", the paths
// with their `+a -d`, and a click that opens the diff pane at that file.
// Hand-mirrored on the desktop (`crates/ui` session screen).
import { splitUnifiedDiff, type UnifiedDiffFile } from "@/lib/unified-diff"
import { splitTruncatedDiff, type ToolKind } from "@/lib/agent-feed"

/** The tool kinds whose settled diff counts as a file edit. */
export const FILE_CARD_KINDS: readonly ToolKind[] = [`edit`, `delete`, `move`]

/** How many paths a card lists before it folds the rest behind "N more". */
export const FILE_CARD_PREVIEW = 5

/** One row of a card. */
export interface SessionFileEntry {
  path: string
  additions: number
  deletions: number
}

/** One card: the files a turn segment touched, anchored AFTER the feed row
 *  that closed the segment. */
export interface SessionFileCard {
  /** The id of the last feed row of the segment — the card renders behind it. */
  afterId: number
  files: SessionFileEntry[]
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
 *  unified diff (`--- a/path` / `+++ b/path`, no `diff --git` header), while
 *  the session diff on the "Changes" wire is full `git diff` output — this
 *  reads both, so one derivation serves both shapes. */
export function toolDiffFiles(diff: string): UnifiedDiffFile[] {
  const body = splitTruncatedDiff(diff).diff
  const git = splitUnifiedDiff(body)
  if (git.length > 0) return git
  return splitBareUnifiedDiff(body)
}

/** `--- a/x` / `+++ b/x` sections, the form `steer::unified_diff` writes. */
function splitBareUnifiedDiff(diff: string): UnifiedDiffFile[] {
  const files: UnifiedDiffFile[] = []
  let current: UnifiedDiffFile | null = null
  let inBody = false
  const patch: string[] = []
  const flush = () => {
    if (!current) return
    current.patch = patch.length > 0 ? patch.join(`\n`) : undefined
    files.push(current)
    patch.length = 0
  }
  for (const line of diff.split(`\n`)) {
    if (line.startsWith(`--- `)) {
      flush()
      const old = pathOf(line.slice(4), `a/`)
      current = {
        filename: old ?? ``,
        status: old === null ? `added` : `modified`,
        additions: 0,
        deletions: 0,
      }
      inBody = false
      continue
    }
    if (!current) continue
    if (line.startsWith(`+++ `)) {
      const next = pathOf(line.slice(4), `b/`)
      if (next === null) current.status = `removed`
      else current.filename = next
      continue
    }
    if (!inBody && line.startsWith(`@@`)) inBody = true
    if (!inBody) continue
    patch.push(line)
    if (line.startsWith(`+`)) current.additions++
    else if (line.startsWith(`-`)) current.deletions++
  }
  flush()
  return files.filter((file) => file.filename !== ``)
}

/** `a/src/x.ts` → `src/x.ts`; `/dev/null` → null (a create or a delete). */
function pathOf(raw: string, prefix: `a/` | `b/`): string | null {
  let path = raw.trim()
  if (path.startsWith(`"`) && path.endsWith(`"`) && path.length >= 2) {
    path = path.slice(1, -1)
  }
  if (path === `/dev/null`) return null
  return path.startsWith(prefix) ? path.slice(prefix.length) : path
}

/** The cards for a whole feed, in transcript order. A segment with no settled
 *  edit yields no card; a file edited twice in one segment is ONE row with
 *  both patches' counts summed. Subagent rows are skipped — their work
 *  belongs to their own card inside the group, not to the main transcript. */
export function sessionFileCards(
  feed: readonly FileCardFeedItem[]
): SessionFileCard[] {
  const cards: SessionFileCard[] = []
  let files = new Map<string, SessionFileEntry>()
  let lastId: number | null = null
  const close = () => {
    if (lastId !== null && files.size > 0) {
      cards.push({ afterId: lastId, files: [...files.values()] })
    }
    files = new Map()
  }
  for (const item of feed) {
    if (item.subagentId !== undefined) continue
    if (item.kind === `user_message`) {
      close()
      lastId = item.id
      continue
    }
    lastId = item.id
    if (item.kind !== `tool`) continue
    if (item.settled !== true || !item.diff) continue
    if (
      item.toolKind === undefined ||
      !FILE_CARD_KINDS.includes(item.toolKind)
    ) {
      continue
    }
    for (const file of toolDiffFiles(item.diff)) {
      const path = file.filename || item.detail?.trim()
      if (!path) continue
      const held = files.get(path)
      if (held) {
        held.additions += file.additions
        held.deletions += file.deletions
        continue
      }
      files.set(path, {
        path,
        additions: file.additions,
        deletions: file.deletions,
      })
    }
  }
  close()
  return cards
}

/** The card's title — `1 file edited` / `4 files edited`, ×4. */
export function fileCardTitle(count: number): string {
  return `${count} ${count === 1 ? `file` : `files`} edited`
}

/** The fold row under the first `FILE_CARD_PREVIEW` paths, or null. */
export function fileCardMoreLabel(count: number): string | null {
  const rest = count - FILE_CARD_PREVIEW
  return rest > 0 ? `${rest} more` : null
}
