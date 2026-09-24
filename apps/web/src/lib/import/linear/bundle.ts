// EXP-630: `LinearSnapshot` → `ImportBundle`, pure. This is the ONLY place
// that knows what a Linear state type, priority number or upload URL means.
//
// Routing (the per-run choice): under `team` every issue lands on its
// team's board (`team:<id>`) and its project becomes a label
// (`project:<id>`); under `project` an issue lands on its project's board
// (`project:<id>`), project-less issues on the team's fallback board
// (`team:<id>`), and no project label is written — the board already says it.
//
// Archived issues (Linear auto-archives old completed work) route to ONE
// archive board per team (`archive:<teamId>`, flagged `archive` so the
// applier archives it at the end) under either routing, keeping their
// project as a label; `importArchived: false` drops them altogether.
import type { IssueEstimation, IssuePriority, IssueStatusCategory } from "@exp/db-schema/domain"
import {
  IMPORT_BUNDLE_VERSION,
  type BundleAsset,
  type BundleEvent,
  type ImportBundle,
  type ImportPreview,
  type ImportRouting,
} from "@/lib/import/bundle"
import { previewFromBundle } from "@/lib/import/preview"
import { extractUploadUrls, type LinearSnapshot } from "@/lib/import/linear/snapshot"

export const LINEAR_SOURCE = `linear`
export const LINEAR_SOURCE_LABEL = `Linear`

export const LINEAR_PRIORITIES: Record<number, IssuePriority> = {
  0: `none`,
  1: `urgent`,
  2: `high`,
  3: `medium`,
  4: `low`,
}

export const LINEAR_STATE_CATEGORIES: Record<string, IssueStatusCategory> = {
  triage: `backlog`,
  backlog: `backlog`,
  unstarted: `unstarted`,
  started: `started`,
  completed: `completed`,
  canceled: `cancelled`,
  cancelled: `cancelled`,
  duplicate: `duplicate`,
}

const PROJECT_LABEL_COLOR = `#6366f1`

export const teamBoardKey = (teamId: string) => `team:${teamId}`
export const projectBoardKey = (projectId: string) => `project:${projectId}`
export const archiveBoardKey = (teamId: string) => `archive:${teamId}`
export const stateKey = (stateId: string) => `state:${stateId}`
export const labelKey = (labelId: string) => `label:${labelId}`
export const projectLabelKey = (projectId: string) => `project:${projectId}`
export const userKey = (userId: string) => `user:${userId}`
export const issueKey = (issueId: string) => `issue:${issueId}`
export const commentKey = (commentId: string) => `comment:${commentId}`

function normalizeColor(color: string): string {
  const hex = color.trim()
  if (/^#[0-9a-fA-F]{6}$/.test(hex)) return hex.toLowerCase()
  if (/^#[0-9a-fA-F]{3}$/.test(hex)) {
    return `#${hex[1]}${hex[1]}${hex[2]}${hex[2]}${hex[3]}${hex[3]}`.toLowerCase()
  }
  return PROJECT_LABEL_COLOR
}

// The archive board's prefix: the team key plus an A, clipped to our
// 4-char cap ("MET" → "META", "SOVA" → "SOVA"); the plan builder derives
// another one when it is taken.
export function archivePrefix(teamKey: string): string {
  const base = teamKey.replace(/[^A-Za-z0-9]/g, ``).toUpperCase()
  return `${base.slice(0, 3)}A`
}

// Linear's estimate is a float on whichever scale the team picked
// (exponential/fibonacci/linear/t-shirt all store the point value).
export function linearEstimate(value: number | null | undefined): number | null {
  if (value === null || value === undefined || !Number.isFinite(value)) return null
  return Math.max(0, Math.round(value))
}

export const LINEAR_ESTIMATION: Record<string, IssueEstimation> = {
  notUsed: `none`,
  exponential: `exponential`,
  fibonacci: `fibonacci`,
  linear: `linear`,
  tShirt: `tshirt`,
}

// A Linear bot/integration account: no real person to map.
export function isLinearBotUser(user: { email: string }): boolean {
  return /@linear\.linear\.app$/i.test(user.email)
}

// Filename for an upload URL: the markdown alt/link text naming it when the
// text has one, else the URL's last path segment.
function assetFilename(text: string, url: string): string | null {
  const escaped = url.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)
  const named = new RegExp(`!?\\[([^\\]]*)\\]\\(${escaped}\\)`).exec(text)
  const alt = named?.[1]?.trim()
  if (alt) return alt.slice(0, 500)
  const tail = url.split(`/`).pop()
  return tail ? tail.slice(0, 500) : null
}

export function toLinearBundle(
  snapshot: LinearSnapshot,
  options: { routing: ImportRouting; importArchived?: boolean }
): ImportBundle {
  const routing = options.routing
  const importArchived = options.importArchived ?? true
  const teams = snapshot.teams
  const projectsById = new Map(snapshot.projects.map((project) => [project.id, project]))
  const labelsById = new Map(snapshot.labels.map((label) => [label.id, label]))
  const statesById = new Map(snapshot.states.map((state) => [state.id, state]))
  const sourceIssues = importArchived
    ? snapshot.issues
    : snapshot.issues.filter((issue) => !issue.archivedAt)
  const archivedTeamIds = new Set(
    sourceIssues.filter((issue) => issue.archivedAt).map((issue) => issue.teamId)
  )
  // Archived issues keep their project as a label under either routing: the
  // archive board says nothing about it.
  const projectLabelsNeeded =
    routing === `team` ||
    sourceIssues.some((issue) => issue.archivedAt && issue.projectId)

  const boards: ImportBundle[`boards`] = teams.map((team) => ({
    key: teamBoardKey(team.id),
    name: team.name,
    prefix: team.key,
  }))
  if (routing === `project`) {
    for (const project of snapshot.projects) {
      boards.push({ key: projectBoardKey(project.id), name: project.name, prefix: `` })
    }
  }
  for (const team of teams) {
    if (!archivedTeamIds.has(team.id)) continue
    boards.push({
      key: archiveBoardKey(team.id),
      name: `${team.name} Archive`,
      prefix: archivePrefix(team.key),
      icon: `archive`,
      archive: true,
    })
  }

  const statuses: ImportBundle[`statuses`] = snapshot.states.map((state) => ({
    key: stateKey(state.id),
    category: LINEAR_STATE_CATEGORIES[state.type] ?? `backlog`,
    name: state.name,
    color: normalizeColor(state.color),
    builtinKey: state.type === `duplicate` ? `duplicate` : null,
  }))

  const labels: ImportBundle[`labels`] = snapshot.labels.map((label) => {
    const parent = label.parentId ? labelsById.get(label.parentId) : undefined
    return {
      key: labelKey(label.id),
      name: parent ? `${parent.name}/${label.name}` : label.name,
      color: normalizeColor(label.color),
    }
  })
  if (projectLabelsNeeded) {
    for (const project of snapshot.projects) {
      labels.push({
        key: projectLabelKey(project.id),
        name: project.name,
        color: PROJECT_LABEL_COLOR,
      })
    }
  }

  const users: ImportBundle[`users`] = snapshot.users.map((user) => ({
    key: userKey(user.id),
    name: user.displayName || user.name || user.email,
    email: isLinearBotUser(user) ? null : user.email,
    active: user.active,
  }))

  const commentsByIssue = new Map<string, LinearSnapshot[`comments`]>()
  for (const comment of snapshot.comments) {
    const list = commentsByIssue.get(comment.issueId) ?? []
    list.push(comment)
    commentsByIssue.set(comment.issueId, list)
  }
  const issueIds = new Set(sourceIssues.map((issue) => issue.id))
  const duplicateOf = new Map<string, string>()
  const blocks = new Map<string, string[]>()
  const related = new Map<string, string[]>()
  for (const relation of snapshot.relations) {
    if (!issueIds.has(relation.issueId) || !issueIds.has(relation.relatedIssueId)) continue
    const push = (map: Map<string, string[]>) => {
      const list = map.get(relation.issueId) ?? []
      list.push(relation.relatedIssueId)
      map.set(relation.issueId, list)
    }
    if (relation.type === `duplicate`) duplicateOf.set(relation.issueId, relation.relatedIssueId)
    else if (relation.type === `blocks`) push(blocks)
    // Linear's `similar` is a weaker `related`; we have one symmetric kind.
    else if (relation.type === `related` || relation.type === `similar`) push(related)
  }

  const issues: ImportBundle[`issues`] = sourceIssues.map((issue) => {
    const state = statesById.get(issue.stateId)
    const category = state ? (LINEAR_STATE_CATEGORIES[state.type] ?? `backlog`) : `backlog`
    const project = issue.projectId ? projectsById.get(issue.projectId) : undefined
    const archived = issue.archivedAt !== null
    const boardKey = archived
      ? archiveBoardKey(issue.teamId)
      : routing === `project` && project
        ? projectBoardKey(project.id)
        : teamBoardKey(issue.teamId)
    const labelKeys = issue.labelIds
      .filter((id) => labelsById.has(id))
      .map((id) => labelKey(id))
    if ((routing === `team` || archived) && project) labelKeys.push(projectLabelKey(project.id))

    const comments = (commentsByIssue.get(issue.id) ?? []).map((comment) => ({
      key: commentKey(comment.id),
      authorKey: comment.userId ? userKey(comment.userId) : null,
      body: comment.body,
      createdAt: comment.createdAt,
      updatedAt: comment.updatedAt,
      editedAt: comment.editedAt,
      parentKey: comment.parentId ? commentKey(comment.parentId) : null,
    }))

    const assets: BundleAsset[] = []
    const seenRefs = new Set<string>()
    let assetIndex = 0
    const collect = (text: string | null, commentRef: string | null) => {
      for (const url of extractUploadUrls(text)) {
        if (seenRefs.has(url)) continue
        seenRefs.add(url)
        assetIndex += 1
        assets.push({
          key: `asset:${issue.id}:${assetIndex}`,
          ref: url,
          filename: assetFilename(text ?? ``, url),
          sizeBytes: snapshot.assetSizes[url] ?? null,
          commentKey: commentRef,
        })
      }
    }
    collect(issue.description, null)
    for (const comment of commentsByIssue.get(issue.id) ?? []) {
      collect(comment.body, commentKey(comment.id))
    }

    const events: BundleEvent[] = []
    for (const entry of issue.history) {
      const base = {
        key: `history:${entry.id}`,
        actorKey: entry.actorId ? userKey(entry.actorId) : null,
        createdAt: entry.createdAt,
      }
      if (entry.toStateId && entry.fromStateId !== entry.toStateId) {
        events.push({
          ...base,
          type: `status_changed`,
          fromStatusKey: entry.fromStateId ? stateKey(entry.fromStateId) : null,
          toStatusKey: stateKey(entry.toStateId),
        })
      }
      if (entry.fromAssigneeId !== entry.toAssigneeId && (entry.fromAssigneeId || entry.toAssigneeId)) {
        events.push({
          ...base,
          key: `${base.key}:assignee`,
          type: `assignee_changed`,
          fromUserKey: entry.fromAssigneeId ? userKey(entry.fromAssigneeId) : null,
          toUserKey: entry.toAssigneeId ? userKey(entry.toAssigneeId) : null,
        })
      }
      if (entry.toPriority !== null && entry.fromPriority !== entry.toPriority) {
        events.push({
          ...base,
          key: `${base.key}:priority`,
          type: `priority_changed`,
          from: entry.fromPriority === null ? null : (LINEAR_PRIORITIES[entry.fromPriority] ?? `none`),
          to: LINEAR_PRIORITIES[entry.toPriority] ?? `none`,
        })
      }
      for (const id of entry.addedLabelIds ?? []) {
        if (!labelsById.has(id)) continue
        events.push({ ...base, key: `${base.key}:add:${id}`, type: `label_added`, labelKey: labelKey(id) })
      }
      for (const id of entry.removedLabelIds ?? []) {
        if (!labelsById.has(id)) continue
        events.push({ ...base, key: `${base.key}:rm:${id}`, type: `label_removed`, labelKey: labelKey(id) })
      }
    }

    const duplicateTarget = duplicateOf.get(issue.id)
    const isDuplicate =
      duplicateTarget !== undefined && (category === `cancelled` || category === `duplicate`)

    return {
      key: issueKey(issue.id),
      boardKey,
      number: issue.number,
      title: issue.title,
      description: issue.description,
      externalRef: issue.identifier,
      externalUrl: `https://linear.app/${snapshot.organization.urlKey}/issue/${issue.identifier}`,
      statusKey: stateKey(issue.stateId),
      priority: LINEAR_PRIORITIES[issue.priority] ?? `none`,
      assigneeKey: issue.assigneeId ? userKey(issue.assigneeId) : null,
      creatorKey: issue.creatorId ? userKey(issue.creatorId) : null,
      createdAt: issue.createdAt,
      updatedAt: issue.updatedAt,
      completedAt: issue.completedAt ?? issue.canceledAt ?? null,
      dueDate: issue.dueDate,
      estimate: linearEstimate(issue.estimate),
      labelKeys,
      duplicateOfKey: isDuplicate ? issueKey(duplicateTarget) : null,
      // A sub-issue whose parent is in the import becomes a `parent`
      // relation; a parent outside it (skipped, or archived while archived
      // issues are left out) leaves it a root.
      parentKey: issue.parentId && issueIds.has(issue.parentId) ? issueKey(issue.parentId) : null,
      relatedKeys: (related.get(issue.id) ?? [])
        .filter((id) => !(isDuplicate && id === duplicateTarget))
        .map(issueKey),
      blocksKeys: (blocks.get(issue.id) ?? []).map(issueKey),
      archived,
      comments,
      events,
      assets,
    }
  })

  // The scale of the first team whose issues carry estimates (one
  // Exponential team = one scale; Linear teams that disagree keep their
  // point values, which read on the adopted scale).
  const estimatedTeamIds = new Set(
    sourceIssues.filter((issue) => issue.estimate !== null).map((issue) => issue.teamId)
  )
  const estimation =
    teams
      .filter((team) => estimatedTeamIds.has(team.id))
      .map((team) => LINEAR_ESTIMATION[team.estimationType ?? ``] ?? null)
      .find((scale) => scale !== null && scale !== `none`) ?? null

  return {
    version: IMPORT_BUNDLE_VERSION,
    source: LINEAR_SOURCE,
    sourceLabel: LINEAR_SOURCE_LABEL,
    boards,
    statuses,
    labels,
    users,
    issues,
    estimation,
  }
}

// What the wizard shows after discovery. Counts come from the team-routed
// bundle (the default); the projects list carries its own counts so the
// routing choice can show where issues would go.
export function linearPreview(snapshot: LinearSnapshot): ImportPreview {
  const bundle = toLinearBundle(snapshot, { routing: `team`, importArchived: true })
  const base = previewFromBundle(bundle)
  const projectCounts = new Map<string, number>()
  let orphanSubIssues = 0
  const issueIds = new Set(snapshot.issues.map((issue) => issue.id))
  for (const issue of snapshot.issues) {
    // Projects count their LIVE issues: archived ones never route by project.
    if (issue.projectId && !issue.archivedAt) {
      projectCounts.set(issue.projectId, (projectCounts.get(issue.projectId) ?? 0) + 1)
    }
    if (issue.parentId && !issueIds.has(issue.parentId)) orphanSubIssues += 1
  }
  const warnings: string[] = []
  if (orphanSubIssues > 0) {
    warnings.push(
      `${orphanSubIssues} sub-issue(s) have a parent outside this workspace export; they are imported as top-level issues.`
    )
  }
  const bots = snapshot.users.filter(isLinearBotUser)
  if (bots.length > 0) {
    warnings.push(`Content by Linear's own integration account is attributed to you.`)
  }
  const teamByProject = new Map<string, string | null>()
  for (const project of snapshot.projects) {
    teamByProject.set(project.id, project.teamIds[0] ?? null)
  }
  return {
    ...base,
    workspace: {
      name: snapshot.organization.name,
      url: `https://linear.app/${snapshot.organization.urlKey}`,
    },
    // Project-only labels are a routing artefact, not workspace labels: keep
    // the preview to Linear's own labels.
    labels: base.labels.filter((label) => !label.key.startsWith(`project:`)),
    statuses: base.statuses.map((status) => ({
      ...status,
      teamKey: teamBoardKey(
        snapshot.states.find((state) => stateKey(state.id) === status.key)?.teamId ?? ``
      ),
    })),
    projects: snapshot.projects.map((project) => ({
      key: projectBoardKey(project.id),
      name: project.name,
      teamKey: teamByProject.get(project.id) ? teamBoardKey(teamByProject.get(project.id)!) : null,
      issueCount: projectCounts.get(project.id) ?? 0,
    })),
    archives: base.archives.map((archive) => ({
      ...archive,
      teamKey: teamBoardKey(archive.key.replace(/^archive:/, ``)),
    })),
    supportsProjectRouting: snapshot.projects.length > 0,
    warnings,
  }
}
