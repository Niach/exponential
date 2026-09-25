// EXP-630: an `ImportPreview` derived from a bundle alone — what the raw
// bundle source shows, and the base every adapter's preview extends.
//
// EXP-1076: besides the flat counts every status, label and user also carries
// the BOARD KEYS its content sits on, so the wizard can hide what a skipped
// board would have taken with it (lib/import/preview-view.ts). Archive issues
// already carry their `archive:<teamId>` board key — nothing special here.
import type { ImportBundle, ImportPreview } from "@/lib/import/bundle"

type CountsByBoard = Map<string, Map<string, number>>

export function previewFromBundle(bundle: ImportBundle): ImportPreview {
  const issueCountByBoard = new Map<string, number>()
  const issueCountByStatus = new Map<string, number>()
  const issueCountByLabel = new Map<string, number>()
  const issueCountByUser = new Map<string, number>()
  const commentCountByUser = new Map<string, number>()
  // key → boardKey → count
  const statusBoards: CountsByBoard = new Map()
  const labelBoards: CountsByBoard = new Map()
  const userIssueBoards: CountsByBoard = new Map()
  const userCommentBoards: CountsByBoard = new Map()
  let comments = 0
  let assets = 0
  let assetBytes = 0
  let events = 0
  const bump = (map: Map<string, number>, key: string | null | undefined) => {
    if (!key) return
    map.set(key, (map.get(key) ?? 0) + 1)
  }
  const bumpBoard = (
    map: CountsByBoard,
    key: string | null | undefined,
    boardKey: string
  ) => {
    if (!key) return
    const inner = map.get(key) ?? new Map<string, number>()
    inner.set(boardKey, (inner.get(boardKey) ?? 0) + 1)
    map.set(key, inner)
  }
  for (const issue of bundle.issues) {
    const board = issue.boardKey
    bump(issueCountByBoard, board)
    bump(issueCountByStatus, issue.statusKey)
    bumpBoard(statusBoards, issue.statusKey, board)
    for (const key of issue.labelKeys) {
      bump(issueCountByLabel, key)
      bumpBoard(labelBoards, key, board)
    }
    bump(issueCountByUser, issue.assigneeKey)
    // Relevance, not the displayed number: an issue counts for its assignee
    // AND its creator (the same person twice only once).
    bumpBoard(userIssueBoards, issue.assigneeKey, board)
    if (issue.creatorKey && issue.creatorKey !== issue.assigneeKey) {
      bumpBoard(userIssueBoards, issue.creatorKey, board)
    }
    comments += issue.comments.length
    for (const comment of issue.comments) {
      bump(commentCountByUser, comment.authorKey)
      bumpBoard(userCommentBoards, comment.authorKey, board)
    }
    assets += issue.assets.length
    assetBytes += issue.assets.reduce((sum, asset) => sum + (asset.sizeBytes ?? 0), 0)
    events += issue.events.length
  }
  const record = (map: CountsByBoard, key: string): Record<string, number> =>
    Object.fromEntries(map.get(key) ?? [])
  return {
    sourceLabel: bundle.sourceLabel,
    workspace: { name: bundle.sourceLabel, url: null },
    teams: bundle.boards
      .filter((board) => !board.archive)
      .map((board) => ({
        key: board.key,
        name: board.name,
        prefix: board.prefix,
        issueCount: issueCountByBoard.get(board.key) ?? 0,
      })),
    projects: [],
    archives: bundle.boards
      .filter((board) => board.archive)
      .map((board) => ({
        key: board.key,
        teamKey: null,
        name: board.name,
        prefix: board.prefix,
        issueCount: issueCountByBoard.get(board.key) ?? 0,
      })),
    statuses: bundle.statuses.map((status) => ({
      key: status.key,
      teamKey: null,
      name: status.name,
      category: status.category,
      color: status.color,
      issueCount: issueCountByStatus.get(status.key) ?? 0,
      issueCountByBoard: record(statusBoards, status.key),
    })),
    labels: bundle.labels.map((label) => ({
      key: label.key,
      name: label.name,
      color: label.color,
      issueCount: issueCountByLabel.get(label.key) ?? 0,
      issueCountByBoard: record(labelBoards, label.key),
    })),
    users: bundle.users.map((user) => ({
      key: user.key,
      name: user.name,
      email: user.email ?? null,
      active: user.active ?? true,
      issueCount: issueCountByUser.get(user.key) ?? 0,
      commentCount: commentCountByUser.get(user.key) ?? 0,
      issueCountByBoard: record(userIssueBoards, user.key),
      commentCountByBoard: record(userCommentBoards, user.key),
    })),
    counts: { issues: bundle.issues.length, comments, assets, assetBytes, events },
    supportsProjectRouting: false,
    warnings: [],
  }
}
