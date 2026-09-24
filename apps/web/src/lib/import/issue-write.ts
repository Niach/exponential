// EXP-630: turns one bundle issue into the rows the applier inserts — PURE.
// Ids are handed in (pre-generated, so storage keys and the rewritten
// markdown can name attachments before anything exists), the mapping comes
// resolved (`IssueWriteContext`), and the output is plain row objects the
// drizzle executor (apply-db.ts) inserts in ONE transaction under the
// preserve-timestamps guard. This is where the interesting decisions live:
//
// - timestamps pass through untouched; `completedAt` follows OUR rule (only a
//   completed/cancelled/duplicate target keeps a completion stamp, REV2-27);
// - `status` is the anchor enum of the target row + `statusId` the row itself
//   (the dual write every other issue writer performs);
// - an unmapped comment author becomes the importer with ONE attribution
//   line, markdown-native so every client renders it (comments.author_id is
//   NOT NULL and invites create no users row);
// - threads re-root to one level (a reply to a reply hangs off the root);
// - assets already fetched get their source URL rewritten to the canonical
//   `/api/attachments/{id}`; unfetched ones keep the source URL and a
//   warning;
// - history becomes `issue_events` rows with the payload shapes the
//   timeline fold reads, only when the plan opted in.
import {
  CATEGORY_ANCHOR,
  type IssuePriority,
  type IssueStatus,
  type IssueStatusCategory,
} from "@exp/db-schema/domain"
import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"
import type { BundleIssue } from "@/lib/import/bundle"

export interface ResolvedStatus {
  id: string
  name: string
  category: IssueStatusCategory
  builtinKey: IssueStatus | null
}

export interface ResolvedUser {
  // Null = attributed to the importer (comments) / left empty (issue fields).
  userId: string | null
  name: string
  email: string | null
}

export interface IssueWriteContext {
  teamId: string
  importerId: string
  sourceLabel: string
  importHistory: boolean
  boards: ReadonlyMap<string, { boardId: string; numbering: `preserve` | `allocate` }>
  statuses: ReadonlyMap<string, ResolvedStatus>
  // Null = the label is skipped.
  labels: ReadonlyMap<string, string | null>
  users: ReadonlyMap<string, ResolvedUser>
  // Where a `duplicate`-category issue lands when it has no canonical to
  // point at (REV2-27: status=duplicate never exists without duplicate_of_id).
  cancelledStatus: ResolvedStatus
}

export interface IssueWriteIds {
  issueId: string
  commentIds: ReadonlyMap<string, string>
  attachmentIds: ReadonlyMap<string, string>
}

export interface PlannedIssueRow {
  id: string
  boardId: string
  teamId: string
  number: number | null
  title: string
  description: string | null
  status: IssueStatus
  statusId: string
  priority: IssuePriority
  assigneeId: string | null
  creatorId: string | null
  dueDate: string | null
  estimate: number | null
  completedAt: Date | null
  createdAt: Date
  updatedAt: Date
}

export interface PlannedCommentRow {
  id: string
  issueId: string
  teamId: string
  boardId: string
  authorId: string
  parentId: string | null
  body: string
  editedAt: Date | null
  createdAt: Date
  updatedAt: Date
}

export interface PlannedAttachmentRow {
  id: string
  assetKey: string
  ref: string
  issueId: string
  teamId: string
  boardId: string
  commentId: string | null
  filename: string
  url: string
  createdAt: Date
}

export interface PlannedEventRow {
  issueId: string
  teamId: string
  boardId: string
  actorUserId: string | null
  type:
    | `created`
    | `status_changed`
    | `assignee_changed`
    | `priority_changed`
    | `label_added`
    | `label_removed`
  payload: Record<string, unknown>
  createdAt: Date
}

export interface PlannedMapRow {
  kind: `issue` | `comment` | `attachment`
  externalId: string
  externalRef: string | null
  localId: string
}

export interface PlannedIssueWrite {
  issue: PlannedIssueRow
  labels: { issueId: string; labelId: string; teamId: string; boardId: string }[]
  comments: PlannedCommentRow[]
  attachments: PlannedAttachmentRow[]
  events: PlannedEventRow[]
  // Members to auto-subscribe (mapped assignee + creator), no notification.
  subscribers: { userId: string; source: `assignee` | `creator` }[]
  map: PlannedMapRow[]
  warnings: string[]
}

const COMPLETION_CATEGORIES: ReadonlySet<IssueStatusCategory> = new Set([
  `completed`,
  `cancelled`,
  `duplicate`,
])

function date(value: string): Date {
  return new Date(value)
}

function formatAttributionDate(iso: string): string {
  return new Date(iso).toLocaleDateString(`en-GB`, {
    day: `numeric`,
    month: `short`,
    year: `numeric`,
    timeZone: `UTC`,
  })
}

// `*Imported from Linear — originally by Hannes Robier (hannes@…), 9 Jun 2026*`
export function attributionLine(
  sourceLabel: string,
  author: { name: string; email: string | null } | null,
  createdAt: string
): string {
  const who = author
    ? author.email && author.email !== author.name
      ? `${author.name} (${author.email})`
      : author.name || author.email || `an unknown user`
    : `an unknown user`
  return `*Imported from ${sourceLabel} — originally by ${who}, ${formatAttributionDate(createdAt)}*`
}

// Plain substring replacement: the source refs are opaque absolute URLs, so
// this covers `![alt](url)`, `[name](url)` and bare mentions alike, and
// yields the canonical relative form on every client.
export function rewriteAssetRefs(
  text: string,
  replacements: ReadonlyMap<string, string>
): string {
  let result = text
  for (const [ref, url] of replacements) {
    if (ref.length === 0) continue
    result = result.split(ref).join(url)
  }
  return result
}

function commentRootKey(
  key: string,
  parents: ReadonlyMap<string, string | null | undefined>
): string {
  let current = key
  const seen = new Set<string>()
  while (true) {
    const parent = parents.get(current)
    if (!parent || seen.has(parent) || !parents.has(parent)) return current
    seen.add(current)
    current = parent
  }
}

export function planIssueWrite(
  issue: BundleIssue,
  ctx: IssueWriteContext,
  ids: IssueWriteIds,
  options: {
    availableAssetKeys: ReadonlySet<string>
    // True when `issue.duplicateOfKey` names an issue this import writes.
    canonicalAvailable: boolean
  }
): PlannedIssueWrite {
  const warnings: string[] = []
  const board = ctx.boards.get(issue.boardKey)
  if (!board) {
    throw new Error(`No target board for ${issue.externalRef} (${issue.boardKey})`)
  }
  let status = ctx.statuses.get(issue.statusKey)
  if (!status) {
    throw new Error(`No target status for ${issue.externalRef} (${issue.statusKey})`)
  }
  if (status.category === `duplicate` && !options.canonicalAvailable) {
    // The links pass writes duplicate_of_id + status=duplicate together once
    // both ends exist; an orphan "duplicate" (no relation, or a canonical
    // outside the import) has nothing to point at and lands as cancelled.
    warnings.push(
      `${issue.externalRef}: marked duplicate at the source without a duplicate target; imported as ${ctx.cancelledStatus.name}.`
    )
    status = ctx.cancelledStatus
  }

  const userId = (key: string | null | undefined): string | null =>
    key ? (ctx.users.get(key)?.userId ?? null) : null

  // --- assets → attachment rows + URL rewrite -----------------------------
  const replacements = new Map<string, string>()
  const attachments: PlannedAttachmentRow[] = []
  for (const asset of issue.assets) {
    const attachmentId = ids.attachmentIds.get(asset.key)
    if (!attachmentId || !options.availableAssetKeys.has(asset.key)) {
      warnings.push(
        `${issue.externalRef}: could not fetch ${asset.filename ?? asset.ref}; the original link is kept.`
      )
      continue
    }
    const commentId = asset.commentKey
      ? (ids.commentIds.get(asset.commentKey) ?? null)
      : null
    replacements.set(asset.ref, buildAttachmentUrl(attachmentId))
    attachments.push({
      id: attachmentId,
      assetKey: asset.key,
      ref: asset.ref,
      issueId: ids.issueId,
      teamId: ctx.teamId,
      boardId: board.boardId,
      commentId,
      filename: asset.filename ?? `file`,
      url: buildAttachmentUrl(attachmentId),
      createdAt: date(issue.createdAt),
    })
  }

  // --- the issue row -------------------------------------------------------
  const completedAt =
    COMPLETION_CATEGORIES.has(status.category) && issue.completedAt
      ? date(issue.completedAt)
      : COMPLETION_CATEGORIES.has(status.category)
        ? date(issue.updatedAt)
        : null
  const description = issue.description
    ? rewriteAssetRefs(issue.description, replacements)
    : null
  const issueRow: PlannedIssueRow = {
    id: ids.issueId,
    boardId: board.boardId,
    teamId: ctx.teamId,
    number: board.numbering === `preserve` ? (issue.number ?? null) : null,
    title: issue.title,
    description: description && description.length > 0 ? description : null,
    status: status.builtinKey ?? CATEGORY_ANCHOR[status.category],
    statusId: status.id,
    priority: issue.priority,
    assigneeId: userId(issue.assigneeKey),
    creatorId: userId(issue.creatorKey),
    dueDate: issue.dueDate ?? null,
    estimate: issue.estimate ?? null,
    completedAt,
    createdAt: date(issue.createdAt),
    updatedAt: date(issue.updatedAt),
  }

  // --- labels --------------------------------------------------------------
  const labelIds = new Set<string>()
  for (const key of issue.labelKeys) {
    const labelId = ctx.labels.get(key)
    if (labelId) labelIds.add(labelId)
  }
  const labels = [...labelIds].map((labelId) => ({
    issueId: ids.issueId,
    labelId,
    teamId: ctx.teamId,
    boardId: board.boardId,
  }))

  // --- comments: roots first, replies re-rooted ---------------------------
  const parents = new Map(
    issue.comments.map((comment) => [comment.key, comment.parentKey])
  )
  const ordered = [...issue.comments].sort((left, right) =>
    left.createdAt.localeCompare(right.createdAt)
  )
  const roots = ordered.filter((comment) => commentRootKey(comment.key, parents) === comment.key)
  const replies = ordered.filter((comment) => commentRootKey(comment.key, parents) !== comment.key)
  const comments: PlannedCommentRow[] = []
  for (const comment of [...roots, ...replies]) {
    const id = ids.commentIds.get(comment.key)
    if (!id) throw new Error(`Missing id for comment ${comment.key}`)
    const author = comment.authorKey ? ctx.users.get(comment.authorKey) : undefined
    const mappedAuthor = author?.userId ?? null
    let body = rewriteAssetRefs(comment.body, replacements)
    if (!mappedAuthor) {
      body = `${attributionLine(
        ctx.sourceLabel,
        author ? { name: author.name, email: author.email } : null,
        comment.createdAt
      )}\n\n${body}`
    }
    const rootKey = commentRootKey(comment.key, parents)
    const parentId = rootKey === comment.key ? null : (ids.commentIds.get(rootKey) ?? null)
    comments.push({
      id,
      issueId: ids.issueId,
      teamId: ctx.teamId,
      boardId: board.boardId,
      authorId: mappedAuthor ?? ctx.importerId,
      parentId,
      body,
      editedAt: comment.editedAt ? date(comment.editedAt) : null,
      createdAt: date(comment.createdAt),
      updatedAt: date(comment.updatedAt ?? comment.createdAt),
    })
  }

  // --- events --------------------------------------------------------------
  const events: PlannedEventRow[] = [
    {
      issueId: ids.issueId,
      teamId: ctx.teamId,
      boardId: board.boardId,
      actorUserId: issueRow.creatorId,
      type: `created`,
      payload: {
        status: issueRow.status,
        statusId: issueRow.statusId,
        priority: issueRow.priority,
        source: `user`,
      },
      createdAt: issueRow.createdAt,
    },
  ]
  if (ctx.importHistory) {
    for (const event of issue.events) {
      const base = {
        issueId: ids.issueId,
        teamId: ctx.teamId,
        boardId: board.boardId,
        actorUserId: userId(event.actorKey),
        createdAt: date(event.createdAt),
      }
      switch (event.type) {
        case `status_changed`: {
          const from = event.fromStatusKey ? ctx.statuses.get(event.fromStatusKey) : undefined
          const to = event.toStatusKey ? ctx.statuses.get(event.toStatusKey) : undefined
          if (!to || (from && from.id === to.id)) break
          events.push({
            ...base,
            type: `status_changed`,
            payload: {
              fromStatusId: from?.id ?? null,
              toStatusId: to.id,
              fromName: from?.name ?? null,
              toName: to.name,
            },
          })
          break
        }
        case `assignee_changed`: {
          const from = userId(event.fromUserKey)
          const to = userId(event.toUserKey)
          if (from === to) break
          events.push({ ...base, type: `assignee_changed`, payload: { from, to } })
          break
        }
        case `priority_changed`: {
          if (!event.to || event.from === event.to) break
          events.push({
            ...base,
            type: `priority_changed`,
            payload: { from: event.from ?? `none`, to: event.to },
          })
          break
        }
        case `label_added`:
        case `label_removed`: {
          const labelId = ctx.labels.get(event.labelKey)
          if (!labelId) break
          events.push({ ...base, type: event.type, payload: { labelId } })
          break
        }
      }
    }
  }

  // --- subscribers ---------------------------------------------------------
  const subscribers: PlannedIssueWrite[`subscribers`] = []
  if (issueRow.creatorId) {
    subscribers.push({ userId: issueRow.creatorId, source: `creator` })
  }
  if (issueRow.assigneeId && issueRow.assigneeId !== issueRow.creatorId) {
    subscribers.push({ userId: issueRow.assigneeId, source: `assignee` })
  }

  // --- entity map ----------------------------------------------------------
  const map: PlannedMapRow[] = [
    {
      kind: `issue`,
      externalId: issue.key,
      externalRef: issue.externalRef,
      localId: ids.issueId,
    },
    ...comments.map((comment) => ({
      kind: `comment` as const,
      externalId:
        issue.comments.find((row) => ids.commentIds.get(row.key) === comment.id)?.key ??
        comment.id,
      externalRef: null,
      localId: comment.id,
    })),
    ...attachments.map((attachment) => ({
      kind: `attachment` as const,
      externalId: attachment.assetKey,
      externalRef: attachment.ref,
      localId: attachment.id,
    })),
  ]

  return {
    issue: issueRow,
    labels,
    comments,
    attachments,
    events,
    subscribers,
    map,
    warnings,
  }
}
