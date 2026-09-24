// EXP-630: the applier — phase orchestration over `ApplyPorts`, provider- and
// database-blind. Every phase is idempotent through the entity map and
// commits before the next, so a job that dies anywhere resumes at the next
// unmapped entity: users → boards → statuses → labels → issues (batched,
// ascending by number so counters clamp monotonically) → links (duplicate /
// parent / blocks / related, a second pass because both ends must exist) →
// archiving (the boards holding the source's archived issues). The drizzle
// implementation of the ports lives in apply-db.ts; apply.test.ts runs this
// file against in-memory ports.
import type { IssueEstimation, IssueStatusCategory } from "@exp/db-schema/domain"
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
import { IMPORT_BATCH_ASSET_FLUSH_BYTES } from "@/lib/import/limits"
import { referencedKeys } from "@/lib/import/plan"
import {
  planIssueWrite,
  type IssueWriteContext,
  type PlannedIssueWrite,
  type ResolvedStatus,
  type ResolvedUser,
} from "@/lib/import/issue-write"

export interface ApplyTeamState {
  boards: { id: string; name: string; prefix: string; archived?: boolean }[]
  estimationType?: IssueEstimation
  statuses: ResolvedStatus[]
  labels: { id: string; name: string }[]
  members: { userId: string; email: string; name: string }[]
}

export interface FetchedAsset {
  bytes: Uint8Array
  contentType: string
  filename: string | null
}

export interface PlannedLink {
  type: `duplicate` | `parent` | `blocks` | `related`
  // Canonical direction (lib/issue-relations.ts): for `parent` the PARENT is
  // `issueId` and the sub-issue `relatedIssueId`.
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
  // Email invite = a placeholder member at once (EXP-630); null when the
  // address belongs to an account that must accept the link first.
  createInvite(input: { email: string; name: string }): Promise<{ memberUserId: string | null }>
  // Idempotent: an already archived board is left alone.
  archiveBoard(boardId: string): Promise<void>
  // Switches the team's estimate scale on (only ever called when it is off).
  setEstimation(type: IssueEstimation): Promise<void>
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
  // Downloaded asset bytes that close a batch early (IMPORT_BATCH_ASSET_FLUSH_BYTES).
  assetFlushBytes?: number
  // Warnings carried over from discovery so the final progress keeps them.
  initialWarnings?: string[]
  // Back-off between asset fetch attempts (tests pass zeros).
  fetchRetryDelaysMs?: number[]
}

const DEFAULT_FETCH_RETRY_DELAYS_MS = [1_000, 4_000]

// A source's file store drops connections now and then (an ECONNRESET
// 900 issues into a 1400-issue run, seen against uploads.linear.app); one
// bad download must cost the file, never the job. Retries, then null.
async function fetchAssetResilient(
  ports: ApplyPorts,
  ref: string,
  delays: number[]
): Promise<{ asset: FetchedAsset | null; error: string | null }> {
  let lastError: unknown = null
  for (let attempt = 0; attempt <= delays.length; attempt += 1) {
    try {
      return { asset: await ports.fetchAsset(ref), error: null }
    } catch (err) {
      lastError = err
      const delay = delays[attempt]
      if (delay === undefined) break
      if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay))
    }
  }
  return { asset: null, error: errorMessage(lastError) }
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
  const assetFlushBytes = Math.max(1, options.assetFlushBytes ?? IMPORT_BATCH_ASSET_FLUSH_BYTES)
  const fetchDelays = options.fetchRetryDelaysMs ?? DEFAULT_FETCH_RETRY_DELAYS_MS
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
  const mappedUsers = await ports.loadMapped(`user`)
  const invitedThisRun = new Map<string, string>()
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
    } else if (entry?.mode === `invite`) {
      // Already on the roster (a member, or a placeholder from an earlier
      // run) → that member; otherwise invite now and remember the result.
      const email = norm(entry.email)
      const member = state.members.find((row) => norm(row.email) === email)
      const mapped = mappedUsers.get(user.key)
      if (member) {
        resolved.userId = member.userId
      } else if (mapped && state.members.some((row) => row.userId === mapped)) {
        resolved.userId = mapped
      } else if (invitedThisRun.has(email)) {
        resolved.userId = invitedThisRun.get(email)!
      } else {
        try {
          const { memberUserId } = await ports.createInvite({
            email: entry.email.trim(),
            name: entry.name.trim() || user.name,
          })
          counts.invites += 1
          if (memberUserId) {
            resolved.userId = memberUserId
            invitedThisRun.set(email, memberUserId)
            await ports.recordMap([
              { kind: `user`, externalId: user.key, externalRef: email, localId: memberUserId },
            ])
          } else {
            warn(
              `${user.name} already has an account and was invited; their content is attributed to you until they accept.`
            )
          }
        } catch (err) {
          warn(`Could not invite ${entry.email}: ${errorMessage(err)}. Their content is attributed to you.`)
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
  // Estimates come along only when the team shows them: a team with
  // estimates off adopts the source's scale before the first batch.
  const estimation = bundle.estimation ?? null
  if (
    estimation &&
    estimation !== `none` &&
    (state.estimationType ?? `none`) === `none` &&
    pending.some((issue) => issue.estimate !== null && issue.estimate !== undefined)
  ) {
    await ports.setEstimation(estimation)
    warn(`Estimates switched on for this team with the ${estimation} scale.`)
    state = await ports.loadTeamState()
  }
  await report(`issues`, done, total)
  // A batch closes at `batchSize` issues OR once the downloaded assets it
  // holds in memory pass `assetFlushBytes`, whichever comes first — a run of
  // 50 MB screen recordings must not pin a whole batch's worth of them.
  let writes: PlannedIssueWrite[] = []
  let assets = new Map<string, FetchedAsset>()
  let assetBytes = 0
  const flush = async () => {
    if (writes.length === 0) return
    await ports.writeBatch({ writes, assets })
    for (const planned of writes) {
      counts.issues += 1
      counts.comments += planned.comments.length
      counts.attachments += planned.attachments.length
      counts.events += planned.events.length
    }
    done += writes.length
    writes = []
    assets = new Map()
    assetBytes = 0
    await report(`issues`, done, total)
  }
  for (const issue of pending) {
    const ids = {
      issueId: ports.newId(),
      commentIds: new Map(issue.comments.map((comment) => [comment.key, ports.newId()])),
      attachmentIds: new Map(issue.assets.map((asset) => [asset.key, ports.newId()])),
    }
    const available = new Set<string>()
    for (const asset of issue.assets) {
      const { asset: fetched, error } = await fetchAssetResilient(ports, asset.ref, fetchDelays)
      if (error) {
        warn(`${issue.externalRef}: ${asset.filename ?? asset.ref} could not be downloaded (${error}); the original link is kept.`)
      }
      if (!fetched) continue
      const attachmentId = ids.attachmentIds.get(asset.key)!
      assets.set(attachmentId, {
        ...fetched,
        filename: fetched.filename ?? asset.filename ?? null,
        contentType: asset.contentType ?? fetched.contentType,
      })
      assetBytes += fetched.bytes.byteLength
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
    if (writes.length >= batchSize || assetBytes >= assetFlushBytes) await flush()
  }
  await flush()

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
    if (issue.parentKey) {
      const parentId = issueIds.get(issue.parentKey)
      if (parentId && parentId !== issueId) {
        links.push({ type: `parent`, issueId: parentId, relatedIssueId: issueId, externalRef: issue.externalRef })
      }
    }
    for (const key of issue.blocksKeys) push(`blocks`, key)
    for (const key of issue.relatedKeys) push(`related`, key)
  }
  await report(`links`, 0, links.length)
  if (links.length > 0) {
    const result = await ports.linkRelations(links)
    counts.relations += result.written
    for (const warning of result.warnings) warn(warning)
  }

  // --- archiving ----------------------------------------------------------
  // Boards this plan CREATED for the source's archived issues are archived
  // now that every issue and link is in — including one an earlier run of
  // this job created (the entity map names it) and never got to archive; an
  // existing board the operator picked stays as it is (they chose a live
  // board on purpose).
  const toArchive: { name: string; boardId: string }[] = []
  for (const board of bundle.boards) {
    if (!board.archive || plan.boards[board.key]?.mode !== `create`) continue
    const boardId = boards.get(board.key)?.boardId ?? mappedBoards.get(board.key)
    if (!boardId) continue
    const row = state.boards.find((candidate) => candidate.id === boardId)
    if (!row || row.archived) continue
    toArchive.push({ name: board.name, boardId })
  }
  await report(`archiving`, 0, toArchive.length)
  for (const [index, board] of toArchive.entries()) {
    try {
      await ports.archiveBoard(board.boardId)
    } catch (err) {
      warn(`Could not archive "${board.name}": ${errorMessage(err)}`)
    }
    await report(`archiving`, index + 1, toArchive.length)
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
