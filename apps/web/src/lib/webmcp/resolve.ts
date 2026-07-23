// Pure resolvers + serializers for the WebMCP tool layer (EXP-245). Agents
// reference entities the human way — board slug/name, issue identifier,
// assignee email/name, label names — and these resolve those references
// against synced collection snapshots. Misses throw with the valid options in
// the message so the calling agent can self-correct.
import type { Board, Comment, Issue, Label, User } from "@/db/schema"

export function resolveBoard<B extends Pick<Board, `slug` | `name`>>(
  ref: string | undefined,
  currentBoardSlug: string | null,
  boards: B[]
): B {
  const available = boards.map((b) => b.slug).join(`, `) || `none`
  const wanted = ref ?? currentBoardSlug
  if (wanted == null) {
    throw new Error(
      `No board specified and none is open — pass a board slug. Available boards: ${available}`
    )
  }
  const needle = wanted.toLowerCase()
  const bySlug = boards.find((b) => b.slug.toLowerCase() === needle)
  if (bySlug) return bySlug
  const byName = boards.filter((b) => b.name.toLowerCase() === needle)
  if (byName.length === 1 && byName[0]) return byName[0]
  if (byName.length > 1) {
    throw new Error(
      `Board name "${wanted}" is ambiguous — use a slug. Available boards: ${available}`
    )
  }
  throw new Error(
    `No board "${wanted}" in this team. Available boards: ${available}`
  )
}

export function resolveIssue<I extends Pick<Issue, `identifier`>>(
  ref: string | undefined,
  currentIssueIdentifier: string | null,
  issues: I[]
): I {
  const wanted = ref ?? currentIssueIdentifier
  if (wanted == null) {
    throw new Error(
      `No issue specified and none is open — pass an issue identifier like EXP-42`
    )
  }
  const needle = wanted.toLowerCase()
  const match = issues.find((i) => i.identifier.toLowerCase() === needle)
  if (!match) {
    throw new Error(
      `No issue "${wanted}" in this team — check the identifier (e.g. via list_issues or search_issues)`
    )
  }
  return match
}

// Resolves an assignee reference (email or display name) to a user id.
// `null` passes through as the explicit unassign sentinel.
export function resolveAssignee(
  ref: string | null,
  users: Pick<User, `id` | `name` | `email`>[]
): string | null {
  if (ref === null) return null
  const needle = ref.toLowerCase()
  const byEmail = users.find((u) => u.email.toLowerCase() === needle)
  if (byEmail) return byEmail.id
  const byName = users.filter((u) => u.name.toLowerCase() === needle)
  if (byName.length === 1 && byName[0]) return byName[0].id
  if (byName.length > 1) {
    throw new Error(
      `Assignee name "${ref}" is ambiguous — use an email address instead`
    )
  }
  const members = users.map((u) => `${u.name} <${u.email}>`).join(`, `)
  throw new Error(
    `No team member matching "${ref}". Members: ${members || `none`}`
  )
}

export function resolveLabels<L extends Pick<Label, `name`>>(
  names: string[],
  labels: L[]
): L[] {
  return names.map((name) => {
    const needle = name.toLowerCase()
    const match = labels.find((l) => l.name.toLowerCase() === needle)
    if (!match) {
      const available = labels.map((l) => l.name).join(`, `) || `none`
      throw new Error(
        `No label "${name}" in this team. Available labels: ${available}`
      )
    }
    return match
  })
}

function toIso(value: Date | string | null | undefined): string | null {
  if (value == null) return null
  return value instanceof Date ? value.toISOString() : value
}

export function serializeBoard(
  board: Pick<Board, `slug` | `name` | `prefix`>,
  issueCount: number
) {
  return {
    slug: board.slug,
    name: board.name,
    prefix: board.prefix,
    issueCount,
  }
}

export function serializeIssue(
  issue: Pick<
    Issue,
    | `identifier`
    | `title`
    | `status`
    | `priority`
    | `assigneeId`
    | `dueDate`
    | `prUrl`
    | `prState`
    | `createdAt`
    | `updatedAt`
  >,
  labelNames: string[],
  userNameById: Map<string, string>
) {
  return {
    identifier: issue.identifier,
    title: issue.title,
    status: issue.status,
    priority: issue.priority,
    assignee: issue.assigneeId
      ? (userNameById.get(issue.assigneeId) ?? issue.assigneeId)
      : null,
    dueDate: issue.dueDate ?? null,
    labels: labelNames,
    prUrl: issue.prUrl ?? null,
    prState: issue.prState ?? null,
    createdAt: toIso(issue.createdAt),
    updatedAt: toIso(issue.updatedAt),
  }
}

export function serializeComment(
  comment: Pick<Comment, `id` | `authorId` | `body` | `createdAt`>,
  userNameById: Map<string, string>
) {
  return {
    id: comment.id,
    author: userNameById.get(comment.authorId) ?? comment.authorId,
    body: comment.body,
    createdAt: toIso(comment.createdAt),
  }
}
