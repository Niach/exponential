// EXP-630: an `ImportPreview` derived from a bundle alone — what the raw
// bundle source shows, and the base every adapter's preview extends.
import type { ImportBundle, ImportPreview } from "@/lib/import/bundle"

export function previewFromBundle(bundle: ImportBundle): ImportPreview {
  const issueCountByBoard = new Map<string, number>()
  const issueCountByStatus = new Map<string, number>()
  const issueCountByLabel = new Map<string, number>()
  const issueCountByUser = new Map<string, number>()
  const commentCountByUser = new Map<string, number>()
  let comments = 0
  let assets = 0
  let assetBytes = 0
  let events = 0
  const bump = (map: Map<string, number>, key: string | null | undefined) => {
    if (!key) return
    map.set(key, (map.get(key) ?? 0) + 1)
  }
  for (const issue of bundle.issues) {
    bump(issueCountByBoard, issue.boardKey)
    bump(issueCountByStatus, issue.statusKey)
    for (const key of issue.labelKeys) bump(issueCountByLabel, key)
    bump(issueCountByUser, issue.assigneeKey)
    comments += issue.comments.length
    for (const comment of issue.comments) bump(commentCountByUser, comment.authorKey)
    assets += issue.assets.length
    assetBytes += issue.assets.reduce((sum, asset) => sum + (asset.sizeBytes ?? 0), 0)
    events += issue.events.length
  }
  return {
    sourceLabel: bundle.sourceLabel,
    workspace: { name: bundle.sourceLabel, url: null },
    teams: bundle.boards.map((board) => ({
      key: board.key,
      name: board.name,
      prefix: board.prefix,
      issueCount: issueCountByBoard.get(board.key) ?? 0,
    })),
    projects: [],
    statuses: bundle.statuses.map((status) => ({
      key: status.key,
      teamKey: null,
      name: status.name,
      category: status.category,
      color: status.color,
      issueCount: issueCountByStatus.get(status.key) ?? 0,
    })),
    labels: bundle.labels.map((label) => ({
      key: label.key,
      name: label.name,
      color: label.color,
      issueCount: issueCountByLabel.get(label.key) ?? 0,
    })),
    users: bundle.users.map((user) => ({
      key: user.key,
      name: user.name,
      email: user.email ?? null,
      active: user.active ?? true,
      issueCount: issueCountByUser.get(user.key) ?? 0,
      commentCount: commentCountByUser.get(user.key) ?? 0,
    })),
    counts: { issues: bundle.issues.length, comments, assets, assetBytes, events },
    supportsProjectRouting: false,
    warnings: [],
  }
}
