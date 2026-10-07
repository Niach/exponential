import type { Board, IssueDraft } from "@/db/schema"
import type { IssuePriority } from "@/lib/domain"
import { ISSUE_DRAFT_COPY } from "@/lib/issue-draft-page"

// EXP-878 / EXP-1170: issue drafts — the pure half. The New issue PAGE
// autosaves to its draft row SILENTLY (no discard confirm on any path but the
// explicit "Discard draft"), so every decision the page and the Drafts list
// make has to be a function somebody can read: what counts as content, what a
// write carries, what a reopened page is allowed to restore, and which rows a
// list may show.

/** The list's stand-in for a draft with no title yet. */
export const draftTitleLabel = ISSUE_DRAFT_COPY.untitled

/** Everything the page holds that decides the draft row. */
export interface DraftSnapshot {
  id: string
  teamId: string
  boardId: string
  title: string
  description: string
  statusId: string | null
  priority: IssuePriority
  assigneeId: string | null
  labelIds: string[]
  dueDate: string | null
  /** Rows already uploaded against this draft (eager uploads, EXP-878). */
  attachmentCount?: number
}

/**
 * Is there anything worth keeping? A trimmed title, any description text, or
 * at least one attachment already uploaded against the draft. Deliberately
 * ignores the property chips: a status or priority pick on an otherwise empty
 * form is not a draft, it is a stray click.
 */
export function hasDraftContent(snapshot: {
  title: string
  description: string
  attachmentCount?: number
}): boolean {
  return (
    snapshot.title.trim().length > 0 ||
    snapshot.description.trim().length > 0 ||
    (snapshot.attachmentCount ?? 0) > 0
  )
}

/** The `issueDrafts.upsert` payload for a snapshot. */
export function toUpsertInput(snapshot: DraftSnapshot) {
  return {
    id: snapshot.id,
    teamId: snapshot.teamId,
    boardId: snapshot.boardId,
    title: snapshot.title.trim(),
    description: snapshot.description,
    statusId: snapshot.statusId,
    priority: snapshot.priority,
    assigneeId: snapshot.assigneeId,
    labelIds: snapshot.labelIds,
    dueDate: snapshot.dueDate,
  }
}

/** What a reopened draft page restores. */
export interface DraftDialogSeed {
  id: string
  boardId: string
  title: string
  description: string
  statusId: string | null
  priority: IssuePriority
  assigneeId: string | null
  labelIds: string[]
  dueDate: string | null
}

/**
 * Restore a draft into the page, DROPPING anything that no longer resolves:
 * `label_ids` carries no foreign key and the assignee may have left the team,
 * so a stale id would either vanish on save or be refused by the create.
 * Resolving here (against the same synced collections the pickers read) keeps
 * the reopened page honest about what it will actually file.
 */
export function toDialogSeed(
  draft: IssueDraft,
  resolvable: {
    boards: readonly Pick<Board, `id`>[]
    labels: readonly { id: string }[]
    users: readonly { id: string }[]
  }
): DraftDialogSeed | null {
  const board = resolvable.boards.find((row) => row.id === draft.boardId)
  if (!board) return null

  const labelIds = new Set(resolvable.labels.map((row) => row.id))
  const userIds = new Set(resolvable.users.map((row) => row.id))

  return {
    id: draft.id,
    boardId: draft.boardId,
    title: draft.title ?? ``,
    description: draft.description ?? ``,
    statusId: draft.statusId,
    priority: draft.priority,
    assigneeId:
      draft.assigneeId && userIds.has(draft.assigneeId)
        ? draft.assigneeId
        : null,
    labelIds: (draft.labelIds ?? []).filter((id) => labelIds.has(id)),
    dueDate: draft.dueDate ?? null,
  }
}

/**
 * EXP-1231: what the synced store says about the draft a page is editing.
 * The same draft may be open on several clients (another tab, another
 * device); whichever creates the issue or discards the draft consumes the
 * row for all of them.
 *
 *  * `created` — an issue carries this draft's id (`issues.draft_id`, stamped
 *    by `issues.create({ draftId })`). Proof enough on its own: the page
 *    stops writing and becomes that issue's detail, as its own Create would.
 *  * `gone` — the row this page has SEEN synced (`seen`) is not there now
 *    (`present` false) and no issue claims it. Discarded elsewhere, or a
 *    moment of resync — the page waits `ISSUE_DRAFT_DISCARDED_GRACE_MS`
 *    before leaving, and resumes if the row returns.
 *  * `open` — nothing to conclude. A row never seen synced can never be
 *    `gone`: a brand-new page whose first write has not landed is `open`.
 */
export type DraftFate =
  | { kind: `open` }
  | { kind: `created`; issueId: string }
  | { kind: `gone` }

export function draftFate(
  seen: boolean,
  present: boolean,
  createdIssueId: string | null | undefined
): DraftFate {
  if (createdIssueId) return { kind: `created`, issueId: createdIssueId }
  if (seen && !present) return { kind: `gone` }
  return { kind: `open` }
}

/** One renderable row of the Drafts list. */
export interface DraftEntry {
  draft: IssueDraft
  board: Board
  title: string
  /** True while the draft has no title of its own (rendered muted). */
  untitled: boolean
}

/**
 * The drafts a team's list may show: this team's rows whose board still
 * resolves (a trashed, archived or left-behind board simply has none), newest
 * edit first — the order somebody who just left a draft expects.
 */
export function resolveDraftEntries(
  drafts: readonly IssueDraft[],
  boards: readonly Board[],
  teamId: string | undefined
): DraftEntry[] {
  if (!teamId) return []
  const boardsById = new Map(boards.map((board) => [board.id, board]))

  return drafts
    .filter((draft) => draft.teamId === teamId)
    .flatMap((draft) => {
      const board = boardsById.get(draft.boardId)
      if (!board) return []
      const title = draft.title.trim()
      return [
        {
          draft,
          board,
          title: title || draftTitleLabel,
          untitled: title.length === 0,
        },
      ]
    })
    .sort(
      (left, right) =>
        new Date(right.draft.updatedAt).getTime() -
        new Date(left.draft.updatedAt).getTime()
    )
}
