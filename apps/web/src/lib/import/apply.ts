// EXP-630: the applier — phase orchestration over `ApplyPorts`, provider- and
// database-blind. Every phase is idempotent through the entity map and
// commits before the next, so a job that dies anywhere resumes at the next
// unmapped entity: users → boards → statuses → labels → issues (batched,
// ascending by number so counters clamp monotonically) → links (duplicate /
// blocks / related, a second pass because both ends must exist). The drizzle
// implementation of the ports lives in apply-db.ts; apply.test.ts runs this
// file against in-memory ports.
import type { IssueStatusCategory } from "@exp/db-schema/domain"
import {
  emptyImportCounts,
  pushWarning,
  type ImportBundle,
  type ImportCounts,
  type ImportEntityKind,
  type ImportPhase,
  type ImportPlan,
  type ImportProgress,
} from "@/lib/import/bundle"
import { referencedKeys } from "@/lib/import/plan"
import {
  planIssueWrite,
  type IssueWriteContext,
  type PlannedIssueWrite,
  type ResolvedStatus,
  type ResolvedUser,
} from "@/lib/import/issue-write"

export interface ApplyTeamState {
  boards: { id: string; name: string; prefix: string }[]
  statuses: ResolvedStatus[]
  labels: { id: string; name: string }[]
  members: { userId: string; email: string; name: string }[]
  pendingInviteEmails: string[]
}

export interface FetchedAsset {
  bytes: Uint8Array
  contentType: string
  filename: string | null
}

export interface PlannedLink {
  type: `duplicate` | `blocks` | `related`
  issueId: string
  relatedIssueId: string
  // For the warning when a link is refused (a cycle, a missing end).
  externalRef: string
}

export interface MapRowInput {
  kind: ImportEntityKind
  externalId: string
  externalRef: string | null
  localId: string
}

export interface ApplyPorts {
  loadTeamState(): Promise<ApplyTeamState>
  loadMapped(kind: ImportEntityKind): Promise<Map<string, string>>
  recordMap(rows: MapRowInput[]): Promise<void>
  createBoard(input: {
    name: string
    prefix: string
    icon: string | null
  }): Promise<{ id: string }>
  createStatus(input: {
    name: string
    color: string
    category: IssueStatusCategory
  }): Promise<{ id: string; name: string }>
  createLabel(input: { name: string; color: string }): Promise<{ id: string }>
  createInvite(email: string): Promise<void>
  fetchAsset(ref: string): Promise<FetchedAsset | null>
  // Uploads the batch's assets, then inserts every row in ONE transaction
  // (issues, attachments, labels, comments, events, subscribers, map rows)
  // under the preserve-timestamps guard; rolls the uploads back on failure.
  writeBatch(batch: {
    writes: PlannedIssueWrite[]
    assets: Map<string, FetchedAsset>
  }): Promise<void>
  linkRelations(links: PlannedLink[]): Promise<{ written: number; warnings: string[] }>
  // False = the worker lost its claim; the applier aborts at once.
  reportProgress(progress: ImportProgress): Promise<boolean>
  // True = the operator cancelled; the applier stops after the current batch.
  shouldStop(): Promise<boolean>
  newId(): string
}

export class ImportAborted extends Error {
  constructor(readonly reason: `cancelled` | `claim_lost`) {
    super(reason === `cancelled` ? `Import cancelled` : `Import claim lost`)
    this.name = `ImportAborted`
  }
}

export interface ApplyOptions {
  teamId: string
  importerId: string
  batchSize?: number
  // Warnings carried over from discovery so the final progress keeps them.
  initialWarnings?: string[]
}

export interface ApplyResult {
  counts: ImportCounts
  warnings: string[]
}

function norm(value: string | null | undefined): string {
  return (value ?? ``).trim().toLowerCase()
}

export async function applyBundle(
  bundle: ImportBundle,
  plan: ImportPlan,
  ports: ApplyPorts,
  options: ApplyOptions
): Promise<ApplyResult> {
  const batchSize = Math.max(1, options.batchSize ?? 25)
  const counts = emptyImportCounts()
  const warnings = [...(options.initialWarnings ?? [])]
  const warn = (message: string) => pushWarning(warnings, message)

  const report = async (phase: ImportPhase, done: number, total: number) => {
    if (await ports.shouldStop()) throw new ImportAborted(`cancelled`)
    const alive = await ports.reportProgress({ phase, done, total, warnings })
    if (!alive) throw new ImportAborted(`claim_lost`)
  }

  let state = await ports.loadTeamState()

  // --- users --------------------------------------------------------------
  await report(`users`, 0, bundle.users.length)
  const users = new Map<string, ResolvedUser>()
  const invitedThisRun = new Set<string>()
  const mappedInvites = await ports.loadMapped(`user`)
  for (const user of bundle.users) {
    const entry = plan.users[user.key]
    const resolved: ResolvedUser = {
      userId: null,
      name: user.name,
      email: user.email ?? null,
    }
    if (entry?.mode === `member`) {
      if (state.members.some((member) => member.userId === entry.userId)) {
        resolved.userId = entry.userId
      } else {
        warn(
          `${user.name} was mapped to a member who left the team; their content is attributed to you.`
        )
      }
    } else if (entry?.mode === `invite` && user.email) {
      const email = norm(user.email)
      const pending =
        mappedInvites.has(user.key) ||
        invitedThisRun.has(email) ||
        state.pendingInviteEmails.some((row) => norm(row) === email)
      if (!pending) {
        try {
          await ports.createInvite(user.email)
          counts.invites += 1
          invitedThisRun.add(email)
          await ports.recordMap([
            { kind: `user`, externalId: user.key, externalRef: user.email, localId: `invite` },
          ])
        } catch (err) {
          warn(`Could not invite ${user.email}: ${errorMessage(err)}`)
        }
      }
    }
    users.set(user.key, resolved)
  }

  // --- boards -------------------------------------------------------------
  // Only boards a still-to-import issue lands on get resolved (a re-run over
  // an imported source must not create empty boards); already-imported
  // issues stay candidates for the links pass through the entity map.
  await report(`boards`, 0, bundle.boards.length)
  const boards = new Map<string, { boardId: string; numbering: `preserve` | `allocate` }>()
  const mappedBoards = await ports.loadMapped(`board`)
  const mappedIssues = await ports.loadMapped(`issue`)
  const neededBoardKeys = new Set(
    bundle.issues
      .filter((issue) => !mappedIssues.has(issue.key))
      .map((issue) => issue.boardKey)
  )
  let boardsDone = 0
  for (const board of bundle.boards) {
    boardsDone += 1
    const entry = plan.boards[board.key]
    if (!entry || entry.mode === `skip` || !neededBoardKeys.has(board.key)) continue
    if (entry.mode === `existing`) {
      if (!state.boards.some((row) => row.id === entry.boardId)) {
        throw new Error(`Target board for "${board.name}" no longer exists`)
      }
      boards.set(board.key, { boardId: entry.boardId, numbering: entry.numbering })
      continue
    }
    const prefix = entry.prefix.toUpperCase()
    let boardId = mappedBoards.get(board.key) ?? null
    if (boardId && !state.boards.some((row) => row.id === boardId)) boardId = null
    if (!boardId) {
      // A resume whose map row never landed: adopt the board by prefix.
      const adopted = state.boards.find((row) => row.prefix.toUpperCase() === prefix)
      if (adopted) boardId = adopted.id
    }
    if (!boardId) {
      const created = await ports.createBoard({
        name: entry.name,
        prefix,
        icon: entry.icon ?? board.icon ?? null,
      })
      boardId = created.id
      counts.boards += 1
      state = await ports.loadTeamState()
    }
    await ports.recordMap([
      { kind: `board`, externalId: board.key, externalRef: board.prefix, localId: boardId },
    ])
    boards.set(board.key, { boardId, numbering: entry.numbering })
    await report(`boards`, boardsDone, bundle.boards.length)
  }

  // Only what a non-skipped issue references gets created (the statuses and
  // labels of a skipped team stay out).
  const skippedBoardKeys = new Set(
    bundle.boards.filter((board) => !boards.has(board.key)).map((board) => board.key)
  )
  const used = referencedKeys(
    { ...bundle, issues: bundle.issues.filter((issue) => !mappedIssues.has(issue.key)) },
    skippedBoardKeys,
    plan.importHistory
  )

  // --- statuses -----------------------------------------------------------
  await report(`statuses`, 0, bundle.statuses.length)
  const statuses = new Map<string, ResolvedStatus>()
  const mappedStatuses = await ports.loadMapped(`status`)
  for (const status of bundle.statuses) {
    if (!used.statusKeys.has(status.key)) continue
    const entry = plan.statuses[status.key]
    if (!entry) throw new Error(`No target status for "${status.name}"`)
    let row: ResolvedStatus | undefined
    if (entry.mode === `builtin`) {
      row = state.statuses.find((candidate) => candidate.builtinKey === entry.builtinKey)
      if (!row) throw new Error(`Builtin status ${entry.builtinKey} is missing`)
    } else if (entry.mode === `existing`) {
      row = state.statuses.find((candidate) => candidate.id === entry.statusId)
      if (!row) throw new Error(`Target status for "${status.name}" no longer exists`)
    } else {
      const mappedId = mappedStatuses.get(status.key)
      row = mappedId
        ? state.statuses.find((candidate) => candidate.id === mappedId)
        : undefined
      if (!row) {
        row = state.statuses.find(
          (candidate) => norm(candidate.name) === norm(entry.name)
        )
        if (row && row.category !== entry.category) {
          throw new Error(
            `A status named "${entry.name}" already exists with another category`
          )
        }
      }
      if (!row) {
        const created = await ports.createStatus({
          name: entry.name,
          color: entry.color,
          category: entry.category,
        })
        counts.statuses += 1
        state = await ports.loadTeamState()
        row = state.statuses.find((candidate) => candidate.id === created.id)
        if (!row) throw new Error(`Created status "${entry.name}" did not load back`)
      }
      await ports.recordMap([
        { kind: `status`, externalId: status.key, externalRef: status.name, localId: row.id },
      ])
    }
    statuses.set(status.key, row)
  }

  // --- labels -------------------------------------------------------------
  await report(`labels`, 0, bundle.labels.length)
  const labels = new Map<string, string | null>()
  const mappedLabels = await ports.loadMapped(`label`)
  for (const label of bundle.labels) {
    if (!used.labelKeys.has(label.key)) continue
    // No decision = create (see evaluatePlan).
    const entry = plan.labels[label.key] ?? { mode: `create` as const }
    if (entry.mode === `skip`) {
      labels.set(label.key, null)
      continue
    }
    let labelId: string | null = null
    if (entry.mode === `existing`) {
      if (!state.labels.some((row) => row.id === entry.labelId)) {
        throw new Error(`Target label for "${label.name}" no longer exists`)
      }
      labelId = entry.labelId
    } else {
      const mappedId = mappedLabels.get(label.key)
      labelId =
        (mappedId && state.labels.some((row) => row.id === mappedId) ? mappedId : null) ??
        state.labels.find((row) => norm(row.name) === norm(label.name))?.id ??
        null
      if (!labelId) {
        const created = await ports.createLabel({ name: label.name, color: label.color })
        labelId = created.id
        counts.labels += 1
        state = await ports.loadTeamState()
      }
      await ports.recordMap([
        { kind: `label`, externalId: label.key, externalRef: label.name, localId: labelId },
      ])
    }
    labels.set(label.key, labelId)
  }

  // --- issues -------------------------------------------------------------
  const cancelledStatus = state.statuses.find((row) => row.builtinKey === `cancelled`)
  if (!cancelledStatus) throw new Error(`Builtin status cancelled is missing`)
  const ctx: IssueWriteContext = {
    teamId: options.teamId,
    importerId: options.importerId,
    sourceLabel: bundle.sourceLabel,
    importHistory: plan.importHistory,
    boards,
    statuses,
    labels,
    users,
    cancelledStatus,
  }
  const candidates = bundle.issues.filter((issue) => {
    if (mappedIssues.has(issue.key)) return true
    if (!boards.has(issue.boardKey)) {
      counts.skippedIssues += 1
      return false
    }
    return true
  })
  const candidateKeys = new Set(candidates.map((issue) => issue.key))
  const pending = candidates
    .filter((issue) => !mappedIssues.has(issue.key))
    .sort(compareIssues)
  const total = pending.length
  let done = 0
  await report(`issues`, done, total)
  for (let index = 0; index < pending.length; index += batchSize) {
    const slice = pending.slice(index, index + batchSize)
    const writes: PlannedIssueWrite[] = []
    const assets = new Map<string, FetchedAsset>()
    for (const issue of slice) {
      const ids = {
        issueId: ports.newId(),
        commentIds: new Map(issue.comments.map((comment) => [comment.key, ports.newId()])),
        attachmentIds: new Map(issue.assets.map((asset) => [asset.key, ports.newId()])),
      }
      const available = new Set<string>()
      for (const asset of issue.assets) {
        const fetched = await ports.fetchAsset(asset.ref)
        if (!fetched) continue
        const attachmentId = ids.attachmentIds.get(asset.key)!
        assets.set(attachmentId, {
          ...fetched,
          filename: fetched.filename ?? asset.filename ?? null,
          contentType: asset.contentType ?? fetched.contentType,
        })
        available.add(asset.key)
      }
      const planned = planIssueWrite(issue, ctx, ids, {
        availableAssetKeys: available,
        canonicalAvailable:
          issue.duplicateOfKey !== null &&
          issue.duplicateOfKey !== undefined &&
          candidateKeys.has(issue.duplicateOfKey),
      })
      for (const warning of planned.warnings) warn(warning)
      writes.push(planned)
    }
    await ports.writeBatch({ writes, assets })
    for (const planned of writes) {
      counts.issues += 1
      counts.comments += planned.comments.length
      counts.attachments += planned.attachments.length
      counts.events += planned.events.length
    }
    done += slice.length
    await report(`issues`, done, total)
  }

  // --- links --------------------------------------------------------------
  const issueIds = await ports.loadMapped(`issue`)
  const links: PlannedLink[] = []
  for (const issue of candidates) {
    const issueId = issueIds.get(issue.key)
    if (!issueId) continue
    const push = (type: PlannedLink[`type`], otherKey: string) => {
      const relatedIssueId = issueIds.get(otherKey)
      if (!relatedIssueId || relatedIssueId === issueId) return
      links.push({ type, issueId, relatedIssueId, externalRef: issue.externalRef })
    }
    if (issue.duplicateOfKey) push(`duplicate`, issue.duplicateOfKey)
    for (const key of issue.blocksKeys) push(`blocks`, key)
    for (const key of issue.relatedKeys) push(`related`, key)
  }
  await report(`links`, 0, links.length)
  if (links.length > 0) {
    const result = await ports.linkRelations(links)
    counts.relations += result.written
    for (const warning of result.warnings) warn(warning)
  }
  await report(`done`, links.length, links.length)

  return { counts, warnings }
}

function compareIssues(
  left: ImportBundle[`issues`][number],
  right: ImportBundle[`issues`][number]
): number {
  if (left.boardKey !== right.boardKey) {
    return left.boardKey < right.boardKey ? -1 : 1
  }
  const leftNumber = left.number ?? Number.MAX_SAFE_INTEGER
  const rightNumber = right.number ?? Number.MAX_SAFE_INTEGER
  if (leftNumber !== rightNumber) return leftNumber - rightNumber
  return left.createdAt.localeCompare(right.createdAt)
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}
