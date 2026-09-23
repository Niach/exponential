// EXP-630: the pure half of the wizard's Map + Dry run steps.
//
// `buildDefaultPlan` pre-selects the obvious mapping (a status onto the
// builtin with the same category and name, a label onto the existing one
// with the same name, a user onto the member with the same email) so the
// operator only overrides what auto-match got wrong. `evaluatePlan` is the
// dry run: every rule the applier would trip over mid-run becomes a BLOCKER
// here (the started-status cap, a prefix collision, overlapping issue
// numbers, storage), and every lossy decision a WARNING. Nothing in this file
// touches the database — the router assembles `TeamState` and the tests feed
// it by hand.
import {
  BUILTIN_STATUS_DEFAULTS,
  ISSUE_STATUS_STARTED_MAX,
  type IssueStatus,
  type IssueStatusCategory,
} from "@exp/db-schema/domain"
import {
  BOARD_PREFIX_PATTERN,
  type BoardPlan,
  type DryRunResult,
  type ImportBundle,
  type ImportPlan,
  type ImportPreview,
  type ImportRouting,
  type StatusPlan,
  type UserPlan,
} from "@/lib/import/bundle"

export interface TeamStateBoard {
  id: string
  name: string
  prefix: string
  // EXP-500 archived: hidden from every list, prefix still reserved.
  archived?: boolean
  // Issue numbers already taken on the board — only loaded for boards the
  // plan targets (the overlap rule needs the exact set).
  numbers?: number[]
  issueCount: number
}

export interface TeamStateStatus {
  id: string
  name: string
  category: IssueStatusCategory
  builtinKey: IssueStatus | null
  color: string
}

export interface TeamStateLabel {
  id: string
  name: string
  color: string
}

export interface TeamStateMember {
  userId: string
  email: string
  name: string
}

export interface TeamState {
  teamId: string
  boards: TeamStateBoard[]
  statuses: TeamStateStatus[]
  labels: TeamStateLabel[]
  members: TeamStateMember[]
  // Emails with a pending invite already (case-insensitive).
  pendingInviteEmails: string[]
  // Cloud seat gate: false = `teamInvites.create` would refuse.
  canInvite: boolean
  // Cloud storage limit; null = unlimited.
  storage: { limitBytes: number | null; usedBytes: number }
  // Bundle issue keys the entity map already holds (a resume / re-run).
  importedIssueKeys: ReadonlySet<string>
  // Bundle board keys an earlier run created, with the board they became:
  // a `create` entry for one of them is a resume, not a prefix collision.
  importedBoards: ReadonlyMap<string, string>
}

function norm(value: string | null | undefined): string {
  return (value ?? ``).trim().toLowerCase()
}

// Builtin names accept a few spellings so "Canceled" (Linear's) lands on our
// "Cancelled" and "Complete" on "Done" without an override.
const BUILTIN_SYNONYMS: Record<IssueStatus, string[]> = {
  backlog: [`backlog`],
  in_progress: [`in progress`, `in-progress`, `doing`, `started`],
  in_review: [`in review`, `review`, `reviewing`],
  done: [`done`, `completed`, `complete`, `closed`, `finished`],
  cancelled: [`cancelled`, `canceled`],
  duplicate: [`duplicate`, `duplicated`],
}

function builtinFor(status: {
  category: IssueStatusCategory
  name: string
  builtinKey?: IssueStatus | null
}): IssueStatus | null {
  if (status.builtinKey) return status.builtinKey
  // The duplicate category takes no customs: it always lands on the builtin.
  if (status.category === `duplicate`) return `duplicate`
  const name = norm(status.name)
  for (const builtin of BUILTIN_STATUS_DEFAULTS) {
    if (builtin.category !== status.category) continue
    if (BUILTIN_SYNONYMS[builtin.key].includes(name)) return builtin.key
  }
  return null
}

// A prefix for a board created from a project name: the initials of its
// words, letter-led, at most 4 chars, falling back to the first letters.
export function derivePrefix(name: string, taken: ReadonlySet<string>): string {
  const words = name
    .split(/[^A-Za-z0-9]+/)
    .filter((word) => word.length > 0)
  const initials = words.map((word) => word[0]!.toUpperCase()).join(``)
  const compact = name.replace(/[^A-Za-z0-9]/g, ``).toUpperCase()
  const candidates = [initials, compact.slice(0, 4), compact.slice(0, 3)]
  for (const candidate of candidates) {
    const trimmed = candidate.replace(/^[^A-Z]+/, ``).slice(0, 4)
    if (BOARD_PREFIX_PATTERN.test(trimmed) && !taken.has(trimmed)) {
      return trimmed
    }
  }
  const base = (initials.replace(/^[^A-Z]+/, ``).slice(0, 3) || `IMP`).padEnd(
    1,
    `P`
  )
  for (let index = 1; index < 100; index++) {
    const candidate = `${base}${index}`.slice(0, 4)
    if (BOARD_PREFIX_PATTERN.test(candidate) && !taken.has(candidate)) {
      return candidate
    }
  }
  return `IMP`
}

function defaultBoardPlan(
  target: { name: string; prefix: string },
  state: TeamState,
  takenPrefixes: Set<string>,
  options: { allowArchived: boolean } = { allowArchived: false }
): BoardPlan {
  // A live board with the same prefix is the obvious target; an ARCHIVED one
  // only for an archive target (an earlier run's archive board), never for
  // live issues — importing into a hidden board would surprise.
  const existing = state.boards.find(
    (board) =>
      norm(board.prefix) === norm(target.prefix) &&
      (options.allowArchived || !board.archived)
  )
  if (existing) {
    return {
      mode: `existing`,
      boardId: existing.id,
      numbering: existing.issueCount === 0 ? `preserve` : `allocate`,
    }
  }
  const upper = target.prefix.toUpperCase()
  const prefix =
    BOARD_PREFIX_PATTERN.test(upper) && !takenPrefixes.has(upper)
      ? upper
      : derivePrefix(target.name, takenPrefixes)
  takenPrefixes.add(prefix)
  return { mode: `create`, name: target.name, prefix, numbering: `preserve` }
}

export function buildDefaultPlan(
  preview: ImportPreview,
  state: TeamState,
  options: { routing?: ImportRouting; importHistory?: boolean; importArchived?: boolean } = {}
): ImportPlan {
  const takenPrefixes = new Set(state.boards.map((board) => board.prefix.toUpperCase()))
  const boards: ImportPlan[`boards`] = {}
  for (const team of preview.teams) {
    // A team with nothing in it would only create an empty board.
    boards[team.key] =
      team.issueCount === 0
        ? { mode: `skip` }
        : defaultBoardPlan(team, state, takenPrefixes)
  }
  for (const project of preview.projects) {
    // Projects have no prefix of their own: derive one, never match an
    // existing board by accident.
    const prefix = derivePrefix(project.name, takenPrefixes)
    takenPrefixes.add(prefix)
    boards[project.key] = {
      mode: `create`,
      name: project.name,
      prefix,
      numbering: `preserve`,
    }
  }
  for (const archive of preview.archives) {
    // The archive board of an earlier run is archived by now: adopt it.
    boards[archive.key] =
      archive.issueCount === 0
        ? { mode: `skip` }
        : defaultBoardPlan(archive, state, takenPrefixes, { allowArchived: true })
  }

  const statuses: ImportPlan[`statuses`] = {}
  for (const status of preview.statuses) {
    const builtinKey = builtinFor(status)
    if (builtinKey) {
      statuses[status.key] = { mode: `builtin`, builtinKey }
      continue
    }
    const existing = state.statuses.find(
      (row) =>
        row.builtinKey === null &&
        row.category === status.category &&
        norm(row.name) === norm(status.name)
    )
    statuses[status.key] = existing
      ? { mode: `existing`, statusId: existing.id }
      : {
          mode: `create`,
          name: status.name,
          color: status.color,
          category: status.category,
        }
  }

  const labels: ImportPlan[`labels`] = {}
  for (const label of preview.labels) {
    const existing = state.labels.find(
      (row) => norm(row.name) === norm(label.name)
    )
    labels[label.key] = existing
      ? { mode: `existing`, labelId: existing.id }
      : { mode: `create` }
  }

  const users: ImportPlan[`users`] = {}
  for (const user of preview.users) {
    const member = user.email
      ? state.members.find((row) => norm(row.email) === norm(user.email))
      : undefined
    users[user.key] = member
      ? { mode: `member`, userId: member.userId }
      : { mode: `self` }
  }

  return {
    routing: options.routing ?? `team`,
    importHistory: options.importHistory ?? true,
    importArchived: options.importArchived ?? true,
    boards,
    statuses,
    labels,
    users,
  }
}

// ---------------------------------------------------------------------------
// Dry run
// ---------------------------------------------------------------------------

function statusPlanCategory(
  plan: StatusPlan,
  state: TeamState
): IssueStatusCategory | null {
  switch (plan.mode) {
    case `builtin`: {
      const builtin = BUILTIN_STATUS_DEFAULTS.find(
        (row) => row.key === plan.builtinKey
      )
      return builtin?.category ?? null
    }
    case `existing`:
      return state.statuses.find((row) => row.id === plan.statusId)?.category ?? null
    case `create`:
      return plan.category
  }
}

function userPlanTarget(plan: UserPlan | undefined): `member` | `importer` {
  return plan?.mode === `member` ? `member` : `importer`
}

export function evaluatePlan(
  bundle: ImportBundle,
  plan: ImportPlan,
  state: TeamState
): DryRunResult {
  const blockers: string[] = []
  const warnings: string[] = []

  // --- boards -------------------------------------------------------------
  // Only boards that a still-to-import issue lands on matter: a re-run over
  // an already imported source must not create empty boards.
  const createPrefixes = new Map<string, string>()
  const boardKeysInUse = new Set(
    bundle.issues
      .filter((issue) => !state.importedIssueKeys.has(issue.key))
      .map((issue) => issue.boardKey)
  )
  const skippedBoardKeys = new Set<string>()
  const targetBoardIds = new Map<string, string>()
  let boardsToCreate = 0
  for (const board of bundle.boards) {
    const entry = plan.boards[board.key]
    if (!entry) {
      if (boardKeysInUse.has(board.key)) {
        blockers.push(`No target board chosen for "${board.name}".`)
      }
      continue
    }
    if (!boardKeysInUse.has(board.key)) {
      if (entry.mode === `skip`) skippedBoardKeys.add(board.key)
      continue
    }
    if (entry.mode === `skip`) {
      skippedBoardKeys.add(board.key)
      continue
    }
    if (entry.mode === `existing`) {
      const target = state.boards.find((row) => row.id === entry.boardId)
      if (!target) {
        blockers.push(
          `The board chosen for "${board.name}" no longer exists in this team.`
        )
        continue
      }
      targetBoardIds.set(board.key, target.id)
      continue
    }
    const adopted = state.importedBoards.get(board.key)
    if (adopted && state.boards.some((row) => row.id === adopted)) {
      // Created by the run this one resumes; the applier reuses it.
      targetBoardIds.set(board.key, adopted)
      continue
    }
    boardsToCreate += 1
    const prefix = entry.prefix.toUpperCase()
    if (!BOARD_PREFIX_PATTERN.test(prefix)) {
      blockers.push(
        `Prefix "${entry.prefix}" for "${entry.name}" must be 1-4 letters or digits and start with a letter.`
      )
    }
    const holder = state.boards.find((row) => row.prefix.toUpperCase() === prefix)
    if (holder) {
      blockers.push(
        holder.archived
          ? `Prefix "${prefix}" belongs to the archived board "${holder.name}". Unarchive it and pick it as the target, or choose another prefix.`
          : `Prefix "${prefix}" is already used by a board in this team. Pick that board as the target or choose another prefix.`
      )
    }
    const clash = createPrefixes.get(prefix)
    if (clash) {
      blockers.push(
        `Prefix "${prefix}" is used for both "${clash}" and "${entry.name}".`
      )
    }
    createPrefixes.set(prefix, entry.name)
  }

  // Only what a non-skipped issue references gets resolved: the statuses
  // and labels of a skipped team must not be created for nothing.
  const { statusKeys: usedStatusKeys, labelKeys: usedLabelKeys } = referencedKeys(
    { ...bundle, issues: bundle.issues.filter((issue) => !state.importedIssueKeys.has(issue.key)) },
    skippedBoardKeys,
    plan.importHistory
  )

  // --- statuses -----------------------------------------------------------
  let statusesToCreate = 0
  let startedToCreate = 0
  const createStatusNames = new Map<string, IssueStatusCategory>()
  for (const status of bundle.statuses) {
    if (!usedStatusKeys.has(status.key)) continue
    const entry = plan.statuses[status.key]
    if (!entry) {
      blockers.push(`No target status chosen for "${status.name}".`)
      continue
    }
    if (entry.mode === `builtin`) {
      if (!state.statuses.some((row) => row.builtinKey === entry.builtinKey)) {
        blockers.push(
          `The builtin status for "${status.name}" is missing from this team.`
        )
      }
      continue
    }
    if (entry.mode === `existing`) {
      if (!state.statuses.some((row) => row.id === entry.statusId)) {
        blockers.push(
          `The status chosen for "${status.name}" no longer exists in this team.`
        )
      }
      continue
    }
    if (entry.category === `duplicate`) {
      blockers.push(
        `"${entry.name}" cannot be created: the duplicate category takes no custom statuses. Map it onto the builtin Duplicate.`
      )
      continue
    }
    const name = norm(entry.name)
    const sameName = state.statuses.find((row) => norm(row.name) === name)
    if (sameName) {
      // The applier adopts a same-named row of the same category (a resume
      // finds its own statuses this way); another category is a real clash.
      if (sameName.category === entry.category) {
        warnings.push(`Status "${sameName.name}" already exists in this team and will be reused.`)
      } else {
        blockers.push(
          `A status named "${entry.name}" already exists in this team with another category. Map "${status.name}" onto it instead of creating a new one.`
        )
      }
      continue
    }
    const sharedCategory = createStatusNames.get(name)
    if (sharedCategory !== undefined) {
      // Two source statuses (one per Linear team) creating the same name
      // share ONE row when their category agrees; the applier adopts it.
      if (sharedCategory !== entry.category) {
        blockers.push(
          `"${entry.name}" would be created twice with different categories. Map one of them onto the other.`
        )
      }
      continue
    }
    createStatusNames.set(name, entry.category)
    statusesToCreate += 1
    if (entry.category === `started`) startedToCreate += 1
  }
  const existingStarted = state.statuses.filter(
    (row) => row.category === `started`
  ).length
  if (existingStarted + startedToCreate > ISSUE_STATUS_STARTED_MAX) {
    blockers.push(
      `A team can have at most ${ISSUE_STATUS_STARTED_MAX} started statuses; this plan would create ${startedToCreate} on top of ${existingStarted}. Map some onto existing started statuses.`
    )
  }

  // --- labels -------------------------------------------------------------
  let labelsToCreate = 0
  const skippedLabelKeys = new Set<string>()
  const createLabelNames = new Set<string>()
  for (const label of bundle.labels) {
    if (!usedLabelKeys.has(label.key)) continue
    // No decision = create (a label an adapter derived, like a project's,
    // has no preview row to decide on).
    const entry = plan.labels[label.key] ?? { mode: `create` as const }
    if (entry.mode === `skip`) {
      skippedLabelKeys.add(label.key)
      continue
    }
    if (entry.mode === `existing`) {
      if (!state.labels.some((row) => row.id === entry.labelId)) {
        blockers.push(
          `The label chosen for "${label.name}" no longer exists in this team.`
        )
      }
      continue
    }
    if (state.labels.some((row) => norm(row.name) === norm(label.name))) {
      warnings.push(
        `Label "${label.name}" already exists in this team and will be reused.`
      )
      continue
    }
    if (createLabelNames.has(norm(label.name))) continue
    createLabelNames.add(norm(label.name))
    labelsToCreate += 1
  }

  // --- users --------------------------------------------------------------
  let invites = 0
  const fallbackUsers: string[] = []
  for (const user of bundle.users) {
    const entry = plan.users[user.key]
    const label = user.email ? `${user.name} (${user.email})` : user.name
    if (!entry) {
      blockers.push(`No decision for user ${label}.`)
      continue
    }
    if (entry.mode === `member`) {
      if (!state.members.some((row) => row.userId === entry.userId)) {
        blockers.push(`The member chosen for ${label} is no longer in this team.`)
      }
      continue
    }
    if (entry.mode === `invite`) {
      if (!user.email) {
        blockers.push(`${label} has no email address to invite.`)
        continue
      }
      if (!state.canInvite) {
        blockers.push(
          `Inviting ${label} would exceed the team's seats. Attribute their content to yourself or add seats first.`
        )
        continue
      }
      if (!state.pendingInviteEmails.some((row) => norm(row) === norm(user.email))) {
        invites += 1
      }
    }
    fallbackUsers.push(label)
  }

  // --- issues -------------------------------------------------------------
  let issues = 0
  let alreadyImported = 0
  let skippedIssues = 0
  let comments = 0
  let attachments = 0
  let assetBytes = 0
  let events = 0
  let fallbackComments = 0
  let allocateIssues = 0
  const numbersByBoard = new Map<string, number[]>()
  for (const issue of bundle.issues) {
    if (skippedBoardKeys.has(issue.boardKey)) {
      skippedIssues += 1
      continue
    }
    if (state.importedIssueKeys.has(issue.key)) {
      alreadyImported += 1
      continue
    }
    issues += 1
    comments += issue.comments.length
    attachments += issue.assets.length
    assetBytes += issue.assets.reduce(
      (sum, asset) => sum + (asset.sizeBytes ?? 0),
      0
    )
    if (plan.importHistory) events += issue.events.length
    for (const comment of issue.comments) {
      if (
        !comment.authorKey ||
        userPlanTarget(plan.users[comment.authorKey]) === `importer`
      ) {
        fallbackComments += 1
      }
    }
    const boardPlan = plan.boards[issue.boardKey]
    if (boardPlan && boardPlan.mode !== `skip`) {
      if (boardPlan.numbering === `allocate`) allocateIssues += 1
      else if (issue.number) {
        const list = numbersByBoard.get(issue.boardKey) ?? []
        list.push(issue.number)
        numbersByBoard.set(issue.boardKey, list)
      }
    }
  }

  // Preserved numbers must not collide with what the target board already
  // holds — the unique index would abort the batch mid-run otherwise.
  for (const [boardKey, numbers] of numbersByBoard) {
    const boardId = targetBoardIds.get(boardKey)
    if (!boardId) continue
    const target = state.boards.find((row) => row.id === boardId)
    if (!target?.numbers?.length) continue
    const taken = new Set(target.numbers)
    const overlap = numbers.filter((number) => taken.has(number))
    if (overlap.length > 0) {
      const name = bundle.boards.find((row) => row.key === boardKey)?.name ?? boardKey
      blockers.push(
        `${overlap.length} issue number(s) from "${name}" (e.g. ${overlap[0]}) are already taken on the target board. Switch that board to "Allocate new numbers".`
      )
    }
  }

  // --- storage ------------------------------------------------------------
  if (
    state.storage.limitBytes !== null &&
    state.storage.usedBytes + assetBytes > state.storage.limitBytes
  ) {
    const mb = (bytes: number) => Math.round(bytes / (1024 * 1024))
    blockers.push(
      `Attachments need about ${mb(assetBytes)} MB but only ${mb(
        Math.max(0, state.storage.limitBytes - state.storage.usedBytes)
      )} MB of storage is left on this plan.`
    )
  }

  // --- warnings -----------------------------------------------------------
  if (fallbackUsers.length > 0) {
    warnings.push(
      `Content by ${fallbackUsers.join(`, `)} will be attributed to you (comments keep an "originally by" line).`
    )
  }
  if (fallbackComments > 0) {
    warnings.push(
      `${fallbackComments} comment(s) will carry an attribution line instead of their original author.`
    )
  }
  if (allocateIssues > 0) {
    warnings.push(
      `${allocateIssues} issue(s) get new numbers on their target board; their original identifiers are kept in the import record.`
    )
  }
  if (skippedIssues > 0) {
    warnings.push(`${skippedIssues} issue(s) are skipped with their board.`)
  }
  if (alreadyImported > 0) {
    warnings.push(
      `${alreadyImported} issue(s) were imported by an earlier run and are left untouched.`
    )
  }
  if (skippedLabelKeys.size > 0) {
    warnings.push(`${skippedLabelKeys.size} label(s) are skipped.`)
  }
  if (!plan.importHistory) {
    warnings.push(`Activity history is not imported.`)
  }
  if (!plan.importArchived) {
    warnings.push(`Archived issues are not imported.`)
  }
  const archiveBoards = bundle.boards.filter(
    (board) => board.archive && boardKeysInUse.has(board.key) && !skippedBoardKeys.has(board.key)
  )
  if (archiveBoards.length > 0) {
    warnings.push(
      `Archived issues go to ${archiveBoards.map((board) => `"${board.name}"`).join(`, `)}, archived after the import (restore under Settings → Archived boards).`
    )
  }

  return {
    blockers,
    warnings,
    counts: {
      boardsToCreate,
      statusesToCreate,
      labelsToCreate,
      invites,
      issues,
      alreadyImported,
      skippedIssues,
      comments,
      attachments,
      assetBytes,
      events,
    },
  }
}

// Which status category an issue lands in under the plan — the writer uses
// it for the completedAt rule, the tests for expectations.
export function plannedStatusCategory(
  plan: StatusPlan | undefined,
  state: TeamState
): IssueStatusCategory | null {
  return plan ? statusPlanCategory(plan, state) : null
}

// Status and label keys any non-skipped issue (or, with history on, any of
// its events) references.
export function referencedKeys(
  bundle: ImportBundle,
  skippedBoardKeys: ReadonlySet<string>,
  importHistory: boolean
): { statusKeys: Set<string>; labelKeys: Set<string> } {
  const statusKeys = new Set<string>()
  const labelKeys = new Set<string>()
  for (const issue of bundle.issues) {
    if (skippedBoardKeys.has(issue.boardKey)) continue
    statusKeys.add(issue.statusKey)
    for (const key of issue.labelKeys) labelKeys.add(key)
    if (!importHistory) continue
    for (const event of issue.events) {
      if (event.type === `status_changed`) {
        if (event.fromStatusKey) statusKeys.add(event.fromStatusKey)
        if (event.toStatusKey) statusKeys.add(event.toStatusKey)
      } else if (event.type === `label_added` || event.type === `label_removed`) {
        labelKeys.add(event.labelKey)
      }
    }
  }
  return { statusKeys, labelKeys }
}
