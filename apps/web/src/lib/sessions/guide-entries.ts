import {
  SESSION_RESULT_FILES_MAX,
  SESSION_RESULTS_FILES_TOTAL_MAX,
  SESSION_RESULTS_MAX,
  SESSION_RESULTS_REPORT_TOTAL_MAX,
  type GuideEntry,
} from "@exp/db-schema/domain"

// EXP-1291: the pure list algebra of a thread's ONE Guide (the root's
// `sessions.guide`), ported from the EXP-879/933/1154/1251 results writes.
// `lib/sessions/guide.ts` runs it inside its `FOR UPDATE` transaction; kept
// apart so the ordering rules (replace in place, append at the end, delete a
// label or a whole topic) are testable without a database.
//
// Every session of the tree writes into the same array, so each entry is
// stamped with its writer (`sessionId`) and a writer touches only its own
// entries (`owns`): a parallel child never overwrites another child's
// `Summary`, it files its own beside it, and the caps count per session.
// Entries without a stamp (none are written any more) belong to everyone.

/** Which entries the writer may touch; absent = all of them. */
export type GuideEntryFilter = (row: GuideEntry) => boolean

const everyEntry: GuideEntryFilter = () => true

/** The writer's ownership rule: its own stamp, or no stamp at all. */
export function ownedBy(sessionId: string): GuideEntryFilter {
  return (row) => !row?.sessionId || row.sessionId === sessionId
}

/** EXP-1397: the writer's rule over its LOGICAL session — its own row and
 *  the ended rows it continues ([`logicalSessionIds`]). */
export function ownedByAny(sessionIds: readonly string[]): GuideEntryFilter {
  const ids = new Set(sessionIds)
  return (row) => !row?.sessionId || ids.has(row.sessionId)
}

/** EXP-1397: a session row as the Guide's ownership reads the tree. */
export interface GuideTreeRow {
  id: string
  parentId: string | null
  branch?: string | null
  status?: string | null
}

/** EXP-1397: the ids of `sessionId`'s logical session: itself and every
 *  ENDED row of the tree it continues — a continuation (an ended child's
 *  sibling, a moved session) keeps the branch, and so the worktree. */
export function logicalSessionIds(sessionId: string, tree: readonly GuideTreeRow[]): string[] {
  const self = tree.find((row) => row.id === sessionId)
  const branch = self?.branch?.trim()
  if (!branch) return [sessionId]
  return [
    sessionId,
    ...tree
      .filter((row) => row.id !== sessionId && row.status === `ended` && row.branch?.trim() === branch)
      .map((row) => row.id),
  ]
}

/** EXP-1351: a REMOVE reaches the caller's whole subtree, so the root can
 *  clear what an ended child filed; never a sibling's or an ancestor's.
 *  EXP-1397: so does an edit; `sessionId` may be a logical session's ids. */
export function ownedBySubtree(
  sessionId: string | readonly string[],
  tree: readonly { id: string; parentId: string | null }[]
): GuideEntryFilter {
  const ids = new Set(typeof sessionId === `string` ? [sessionId] : sessionId)
  for (let grew = true; grew; ) {
    grew = false
    for (const row of tree) {
      if (row.parentId && ids.has(row.parentId) && !ids.has(row.id)) {
        ids.add(row.id)
        grew = true
      }
    }
  }
  return (row) => !row?.sessionId || ids.has(row.sessionId)
}

/** A text entry = the topic's report (label + attachmentId null). */
export function isTextEntry(row: GuideEntry | null | undefined): boolean {
  return !!row && typeof row.text === `string` && !row.attachmentId
}

export function reportTextLength(results: readonly GuideEntry[]): number {
  let total = 0
  for (const row of results) if (isTextEntry(row)) total += row.text?.length ?? 0
  return total
}

export function reportFileCount(results: readonly GuideEntry[]): number {
  let total = 0
  for (const row of results) {
    if (isTextEntry(row) && Array.isArray(row.files)) total += row.files.length
  }
  return total
}

/** A topic's files as stored: strings only, trimmed, blanks and duplicates
 *  dropped (first position kept), capped per topic. */
export function cleanGuideFiles(files: readonly unknown[]): string[] {
  const out: string[] = []
  const seen = new Set<string>()
  for (const raw of files) {
    if (typeof raw !== `string`) continue
    const path = raw.trim()
    if (!path || seen.has(path)) continue
    seen.add(path)
    out.push(path)
    if (out.length >= SESSION_RESULT_FILES_MAX) break
  }
  return out
}

export function exceedsGuideCap(results: readonly GuideEntry[]): boolean {
  return results.length > SESSION_RESULTS_MAX
}

/**
 * File (or replace) a topic's report text. One text entry per topic and
 * writer: replaced IN PLACE, else appended (a topic that already has
 * pictures gets its text at the topic's first position). `files`: undefined
 * keeps the replaced entry's, [] clears them. `prUrl`: undefined keeps the
 * replaced entry's tag, null/blank clears it. Null = the write would grow
 * past a cap (a write that shrinks an over-cap report still lands).
 */
export function upsertGuideText(
  current: GuideEntry[] | null,
  topic: string,
  text: string,
  files: readonly string[] | undefined,
  meta: { at: number; sessionId: string; prUrl?: string | null },
  /** Whose entries the write replaces and is capped by (EXP-1397: a
   *  logical session's); default = `meta.sessionId`'s. */
  owned: GuideEntryFilter = ownedBy(meta.sessionId)
): GuideEntry[] | null {
  const before = current ?? []
  const results = [...before]
  const index = results.findIndex(
    (row) => isTextEntry(row) && row.topic === topic && owned(row)
  )
  const kept =
    files === undefined
      ? index === -1 || !Array.isArray(results[index]?.files)
        ? []
        : cleanGuideFiles(results[index]!.files!)
      : cleanGuideFiles(files)
  const prUrl =
    meta.prUrl === undefined
      ? index === -1
        ? null
        : (results[index]?.prUrl ?? null)
      : meta.prUrl?.trim() || null
  const entry: GuideEntry = {
    topic,
    label: null,
    attachmentId: null,
    width: null,
    height: null,
    text,
    ...(kept.length ? { files: kept } : {}),
    ...(prUrl ? { prUrl } : {}),
    at: meta.at,
    sessionId: meta.sessionId,
  }
  if (index === -1) {
    const first = results.findIndex((row) => row?.topic === topic && owned(row))
    if (first === -1) results.push(entry)
    else results.splice(first, 0, entry)
  } else {
    results[index] = entry
  }
  const mine = (rows: readonly GuideEntry[]) =>
    rows.filter((row) => row === entry || owned(row))
  const after = mine(results)
  const prior = mine(before)
  if (exceedsGuideCap(after)) return null
  const textTotal = reportTextLength(after)
  if (textTotal > SESSION_RESULTS_REPORT_TOTAL_MAX && textTotal > reportTextLength(prior)) {
    return null
  }
  const fileTotal = reportFileCount(after)
  if (fileTotal > SESSION_RESULTS_FILES_TOTAL_MAX && fileTotal > reportFileCount(prior)) {
    return null
  }
  return results
}

/** `(topic, label)` is a picture's upsert key: a re-publish REPLACES the
 *  writer's picture of that pair in place. The displaced attachment comes
 *  back so the caller reclaims its row + blob after commit. */
export function upsertGuidePicture(
  current: GuideEntry[] | null,
  entry: GuideEntry,
  owns: GuideEntryFilter = everyEntry
): { results: GuideEntry[]; displacedAttachmentId: string | null } {
  const results = [...(current ?? [])]
  const index = results.findIndex(
    (row) =>
      !isTextEntry(row) &&
      row?.topic === entry.topic &&
      row?.label === entry.label &&
      owns(row)
  )
  if (index === -1) {
    results.push(entry)
    return { results, displacedAttachmentId: null }
  }
  const displaced = results[index]?.attachmentId ?? null
  results[index] = entry
  return {
    results,
    displacedAttachmentId: displaced && displaced !== entry.attachmentId ? displaced : null,
  }
}

/** `label` removes one picture; without it the WHOLE topic goes (its text
 *  too); `text: true` removes only the topic's report text. Owned entries
 *  only. */
export function removeGuideEntries(
  current: GuideEntry[] | null,
  target: { topic: string; label?: string | null; text?: boolean },
  owns: GuideEntryFilter = everyEntry
): { results: GuideEntry[]; removedAttachmentIds: string[] } {
  const results: GuideEntry[] = []
  const removedAttachmentIds: string[] = []
  for (const row of current ?? []) {
    const hit =
      row?.topic === target.topic &&
      owns(row) &&
      (target.text
        ? isTextEntry(row)
        : target.label == null || (!isTextEntry(row) && row?.label === target.label))
    if (hit) {
      if (typeof row?.attachmentId === `string` && row.attachmentId) {
        removedAttachmentIds.push(row.attachmentId)
      }
      continue
    }
    results.push(row)
  }
  return { results, removedAttachmentIds }
}

export type GuideSummaryRow = { topic: string; text?: number; label?: string }

/** The compact list every Guide write answers with: what the writer filed
 *  so far, without ids or pixel sizes. */
export function guideSummary(results: readonly GuideEntry[] | null): GuideSummaryRow[] {
  const out: GuideSummaryRow[] = []
  for (const row of results ?? []) {
    if (typeof row?.topic !== `string`) continue
    if (isTextEntry(row)) out.push({ topic: row.topic, text: row.text?.length ?? 0 })
    else if (typeof row.label === `string`) out.push({ topic: row.topic, label: row.label })
  }
  return out
}

/** A label-less picture gets the topic's next free `Shot N`. */
export function nextShowLabel(results: readonly GuideEntry[] | null, topic: string): string {
  const taken = new Set<string>()
  for (const row of results ?? []) {
    if (row?.topic === topic && !isTextEntry(row) && typeof row.label === `string`) {
      taken.add(row.label)
    }
  }
  let n = taken.size + 1
  while (taken.has(`Shot ${n}`)) n += 1
  return `Shot ${n}`
}

/** The listed paths a branch/PR diff does not have (a path matches a diff
 *  file's `path` or its rename source). */
export function missingGuideFiles(
  listed: readonly string[],
  diffFiles: readonly { path: string; previousPath?: string | null }[]
): string[] {
  return listed.filter(
    (path) =>
      !diffFiles.some(
        (file) => file.path === path || (!!file.previousPath && file.previousPath === path)
      )
  )
}

/** The canonical `https://github.com/<owner>/<repo>/pull/<n>` form of a
 *  GitHub PR URL, or null. A topic's `prUrl` is matched byte for byte
 *  against `pull_requests.url`, so every Guide write goes through this. */
export function canonicalGithubPrUrl(raw: string): string | null {
  const trimmed = raw.trim()
  if (!trimmed) return null
  let url: URL
  try {
    url = new URL(trimmed)
  } catch {
    return null
  }
  if (url.protocol !== `https:` && url.protocol !== `http:`) return null
  const host = url.hostname.toLowerCase()
  if (host !== `github.com` && host !== `www.github.com`) return null
  const match = url.pathname.match(/^\/([^/]+)\/([^/]+)\/pull\/(\d+)(?:\/.*)?$/)
  if (!match) return null
  return `https://github.com/${match[1]}/${match[2]}/pull/${Number(match[3])}`
}

/** When a session opens a NEW PR, the text topics it filed without a
 *  `prUrl` belong to that PR: tag them, so a later stacked PR's body never
 *  claims them. Only the writer's own (`owns`). */
export function stampUntaggedGuideEntries(
  results: readonly GuideEntry[] | null,
  prUrl: string,
  owns: GuideEntryFilter = everyEntry
): { results: GuideEntry[]; changed: boolean } {
  let changed = false
  const next = (results ?? []).map((row) => {
    if (!isTextEntry(row) || (typeof row.prUrl === `string` && row.prUrl.trim())) return row
    if (!owns(row)) return row
    changed = true
    return { ...row, prUrl }
  })
  return { results: next, changed }
}

/** Every attachment id a Guide references (the orphan sweep keeps them). */
export function guideAttachmentIds(results: readonly GuideEntry[] | null): string[] {
  const out: string[] = []
  for (const row of results ?? []) {
    if (typeof row?.attachmentId === `string` && row.attachmentId) out.push(row.attachmentId)
  }
  return out
}
